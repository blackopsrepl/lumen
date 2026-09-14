# Lumen

A single-binary browser service for agents and humans. One process owns every
agent's isolated Chromium, exposes a [Codex-style capability API](https://github.com/microsoft/playwright-cli),
and serves a live, controllable 1:1 view of each browser to a human.

It replaces the earlier script-and-daemon stack (`browser-broker.js`, the
`playwright-cli show` dashboard, a filesystem feedback watcher, and three
systemd units) with one image, one config, one port, and one unit.

## Architecture

```
        host                                        container: lumen (one process, port 8899)
  human  http://localhost:8899  ── HTTP/WS ──▶  http (API + embedded viewer)
  agent  playwright-cli ───────── CDP ───────▶  supervisor → Chromium/<agent>
                                                 cdp · capabilities · view · input
                                                 feedback (SQLite) · config
```

- **Control plane** — sessions, viewport, tabs, windows, screenshots, page
  scale, visibility, raw CDP, and input, all over HTTP/JSON.
- **View plane** — a CDP screencast fanned out over WebSocket, rendered to a
  canvas. Zoom is view-only and never disturbs the agent; a control token lets
  the human take over, with input forwarded as CDP events.
- **Feedback** — annotations are written straight to SQLite and read back by the
  agent, so nothing depends on a watcher running.

## Quick start

```bash
make bootstrap              # host CLI + image + service + skill + smoke
open http://localhost:8899  # the human viewer
bin/pw.sh -s=alice goto https://example.com
bin/pw.sh -s=alice snapshot
```

`bin/pw.sh` asks Lumen to ensure your session's browser, attaches
`playwright-cli` over CDP, and runs the command. The container shares the host
network, so a dev server on the host is reachable from the page at
`http://127.0.0.1:<port>`. `host.containers.internal` and
`host.docker.internal` are also mapped to the host loopback for compatibility
with tools that use a container-host alias.

## Capability API

| Capability | Endpoint |
| --- | --- |
| sessions | `GET/POST /v1/sessions`, `GET/DELETE /v1/sessions/{name}` |
| viewport set/reset | `PUT/DELETE /v1/sessions/{name}/viewport` |
| page scale | `PUT /v1/sessions/{name}/page-scale` |
| visibility | `PUT /v1/sessions/{name}/visibility` |
| tabs | `GET/POST /v1/sessions/{name}/tabs`, `POST …/{index}/activate`, `DELETE …/{index}` |
| screenshot | `POST /v1/sessions/{name}/screenshot?full=true` |
| navigate | `POST /v1/sessions/{name}/navigate` |
| raw CDP | `POST /v1/sessions/{name}/cdp` |
| audit | `GET /v1/audit` |
| stream + input | `GET /v1/sessions/{name}/stream` (WebSocket) |
| feedback | `GET/POST /v1/sessions/{name}/feedback`, `POST …/ack-all` |

## CLI

```bash
lumen serve                      # run the service (default)
lumen status                     # list sessions
lumen ensure <name>              # ensure a session, print its CDP endpoint
lumen stop <name>                # stop a session's browser
lumen feedback <name> [--consume]  # print a session's pending notes
```

Inside the container the binary is `lumen`; from the host, use
`podman exec lumen lumen …` or `bin/*.sh`.

Sessions carry provenance. `lumen ensure` / `bin/pw.sh` register a session as
**agent-owned** (optionally labelled with `--owner`), so the viewer groups it
under “Agent sessions” and its feedback has a reader. A browser the human
creates from the viewer is **manual** — it is clearly marked “no agent”, and
notes left on it are not read by anyone. An agent that later uses the same name
adopts a manual session.

## Layout

```
Containerfile          multi-stage image: Rust builder -> Playwright runtime
compose.yaml           one service (host network, /data volume)
config/lumen.toml      the only config file
src/                   supervisor, cdp, capabilities, view, feedback, client, http
ui/                    no-build viewer (ES modules + CSS), embedded in the binary
systemd/lumen.service  the only unit
bin/                   bootstrap, build/up/down, install, pw.sh, smoke
skills/lumen/SKILL.md  opencode skill
```

## Development

```bash
cargo test                     # unit tests
cargo run --example spike      # drive a real Chromium and capture live frames
bin/build.sh && bin/up.sh      # container image + service
make ui-test                   # Playwright tests against Lumen
```

`make ui-test` starts a disposable local Lumen service when `LUMEN_URL` is not
set. Set `LUMEN_URL` to test an already-running service. `LUMEN_CHROME` can
point to the Chromium executable used by the disposable test service. The
tests use the Playwright browser runner and exercise the rendered viewer, its WebSocket
stream, navigation, control handoff, feedback annotation, and provenance UI.

## Security

The service binds loopback only, and the container runs with `no-new-privileges`;
Chromium itself needs no sandbox here.

Navigation host policy lives under `[policy]` in `config/lumen.toml`:

```toml
[policy]
allow_hosts = []          # empty = every host; ".example.com" matches subdomains
blocked_hosts = ["evil.test"]
```

How it is enforced: per tab, inside the browser. Lumen installs a navigation
check on every tab it mediates — creates, activates, or adopts while switching.
Any document navigation on such a tab is checked no matter which session issues
it: Lumen's API, an agent's own `playwright-cli` connection over CDP, or a
redirect. Navigations through Lumen's API answer `403`; the same blocked
navigation driven directly over CDP surfaces in the browser as
`net::ERR_BLOCKED_BY_CLIENT`.

Coverage boundary: a tab an agent creates entirely outside Lumen's API is not
checked until Lumen's API touches it. The policy bounds Lumen-mediated browsing;
it is not a sandbox for an agent's direct browser control. If the whole browser
must be bounded regardless of who drives it, make the network the enforcement
point instead.

The audit trail (`GET /v1/audit`) records navigations, tab operations, raw CDP
calls, and feedback that pass through Lumen's API. Actions an agent takes
directly over CDP do not pass through Lumen and are not audited.
