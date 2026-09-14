#!/usr/bin/env node
'use strict';

//
// browser-broker: gives each agent its own isolated Chromium inside the
// container and hands the agent's host-side playwright-cli a CDP endpoint to
// attach to.
//
// Each agent gets:
//   - its own Chromium process (isolated cookies/storage/process)
//   - its own persistent profile under $AGENT_DATA_DIR/<name>
//   - its own random loopback CDP port (Chrome binds 127.0.0.1 only)
//
// The broker is intentionally tiny and has no dependencies: it spawns the
// Chromium binary directly and reads the "DevTools listening on ws://..." line
// from its stderr to learn the CDP port.
//
// HTTP API (bound to BROKER_HOST, default 127.0.0.1):
//   GET    /healthz            -> {status:"ok",agents:N}
//   GET    /sessions           -> [{name,cdpUrl,port}]
//   POST   /sessions {name}    -> {name,cdpUrl,port}   (idempotent)
//   DELETE /sessions/<name>    -> {removed:bool}
//

const http = require('http');
const fs = require('fs');
const path = require('path');
const { spawn } = require('child_process');

const PORT = parseInt(process.env.BROKER_PORT || '8090', 10);
const HOST = process.env.BROKER_HOST || '127.0.0.1';
const CHROME = process.env.CHROME_BIN || 'chromium';
const DATA_DIR = process.env.AGENT_DATA_DIR || '/data/agents';
const MAX_AGENTS = parseInt(process.env.MAX_AGENTS || '8', 10);
const HEADLESS = (process.env.AGENT_HEADLESS ?? '1') !== '0';
const NO_SANDBOX = (process.env.AGENT_NO_SANDBOX ?? '1') !== '0';
const CHROME_ARGS = (process.env.AGENT_EXTRA_ARGS || '').split(/\s+/).filter(Boolean);

const sessions = new Map();

function log(message) {
  process.stderr.write(`[broker] ${message}\n`);
}

function validName(name) {
  return typeof name === 'string' && /^[A-Za-z0-9._-]{1,32}$/.test(name);
}

function chromeArgs(profileDir) {
  const args = [
    '--remote-debugging-port=0',
    `--user-data-dir=${profileDir}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-dev-shm-usage',
    '--disable-background-networking',
  ];
  if (NO_SANDBOX) args.push('--no-sandbox', '--disable-setuid-sandbox');
  if (HEADLESS) args.push('--headless=new');
  args.push(...CHROME_ARGS, 'about:blank');
  return args;
}

function launch(name) {
  const profileDir = path.join(DATA_DIR, name);
  fs.mkdirSync(profileDir, { recursive: true });

  const proc = spawn(CHROME, chromeArgs(profileDir), { stdio: ['ignore', 'ignore', 'pipe'] });
  const entry = { name, pid: proc.pid, proc, profileDir, port: null, cdpUrl: null };
  sessions.set(name, entry);

  let buffered = '';
  proc.stderr.on('data', (chunk) => {
    buffered += chunk.toString();
    const match = buffered.match(/DevTools listening on ws:\/\/127\.0\.0\.1:(\d+)\//);
    if (match && !entry.port) {
      entry.port = parseInt(match[1], 10);
      entry.cdpUrl = `http://127.0.0.1:${entry.port}`;
      log(`agent '${name}' CDP ready at ${entry.cdpUrl}`);
    }
  });

  proc.on('exit', (code) => {
    log(`agent '${name}' chromium exited (${code})`);
    if (sessions.get(name) === entry) sessions.delete(name);
  });

  return entry;
}

function waitForCdp(entry, timeoutMs = 15000) {
  return new Promise((resolve, reject) => {
    const started = Date.now();
    const poll = () => {
      if (entry.cdpUrl) return resolve(entry);
      if (!sessions.has(entry.name)) return reject(new Error(`agent '${entry.name}' exited before CDP was ready`));
      if (Date.now() - started > timeoutMs) return reject(new Error(`agent '${entry.name}' CDP timeout`));
      setTimeout(poll, 150);
    };
    poll();
  });
}

async function ensure(name) {
  const existing = sessions.get(name);
  if (existing && existing.cdpUrl) return existing;
  if (!existing && sessions.size >= MAX_AGENTS) throw new Error(`max agents (${MAX_AGENTS}) reached`);
  const entry = existing || launch(name);
  return waitForCdp(entry);
}

function kill(name) {
  const entry = sessions.get(name);
  if (!entry) return false;
  try { entry.proc.kill('SIGTERM'); } catch { /* ignore */ }
  sessions.delete(name);
  return true;
}

function publicView(entry) {
  return { name: entry.name, port: entry.port, cdpUrl: entry.cdpUrl };
}

function send(res, code, body) {
  const payload = JSON.stringify(body);
  res.writeHead(code, { 'content-type': 'application/json', 'content-length': Buffer.byteLength(payload) });
  res.end(payload);
}

function readBody(req) {
  return new Promise((resolve, reject) => {
    let data = '';
    req.on('data', (chunk) => {
      data += chunk;
      if (data.length > 1e5) req.destroy();
    });
    req.on('end', () => {
      if (!data) return resolve({});
      try { resolve(JSON.parse(data)); } catch (err) { reject(err); }
    });
    req.on('error', reject);
  });
}

const server = http.createServer(async (req, res) => {
  const url = new URL(req.url, `http://${req.headers.host}`);
  const nameFromPath = url.pathname.startsWith('/sessions/')
    ? decodeURIComponent(url.pathname.slice('/sessions/'.length))
    : null;

  try {
    if (req.method === 'GET' && url.pathname === '/healthz') {
      return send(res, 200, { status: 'ok', agents: sessions.size, max: MAX_AGENTS });
    }

    if (req.method === 'GET' && url.pathname === '/sessions') {
      return send(res, 200, [...sessions.values()].map(publicView));
    }

    if (req.method === 'POST' && url.pathname === '/sessions') {
      const body = await readBody(req);
      if (!validName(body.name)) return send(res, 400, { error: 'invalid name (use [A-Za-z0-9._-], 1-32 chars)' });
      const entry = await ensure(body.name);
      return send(res, 200, publicView(entry));
    }

    if (req.method === 'DELETE' && nameFromPath) {
      if (!validName(nameFromPath)) return send(res, 400, { error: 'invalid name' });
      return send(res, 200, { removed: kill(nameFromPath) });
    }

    return send(res, 404, { error: 'not found' });
  } catch (err) {
    log(`request error: ${err.message}`);
    return send(res, 500, { error: err.message });
  }
});

function shutdown() {
  log('shutting down; killing agent browsers');
  for (const entry of sessions.values()) {
    try { entry.proc.kill('SIGTERM'); } catch { /* ignore */ }
  }
  server.close(() => process.exit(0));
  setTimeout(() => process.exit(0), 3000).unref();
}

process.on('SIGTERM', shutdown);
process.on('SIGINT', shutdown);

server.listen(PORT, HOST, () => {
  log(`browser broker listening on http://${HOST}:${PORT} (data: ${DATA_DIR}, chrome: ${CHROME})`);
});
