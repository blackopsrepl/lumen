const el = (id) => document.getElementById(id);

const canvas = el("screen");
const overlay = el("overlay");
const stage = el("stage");
const ctx = canvas.getContext("2d");
const octx = overlay.getContext("2d");
const dpr = window.devicePixelRatio || 1;

const state = {
  session: null,
  ws: null,
  control: "agent",
  frame: null,
  frameW: 0,
  frameH: 0,
  scale: 1,
  ox: 0,
  oy: 0,
  fit: true,
  panning: false,
  last: { x: 0, y: 0 },
  annotating: false,
  drawStart: null,
  drawEnd: null,
  drawing: false,
  feedbackSig: null,
  feedbackPending: 0,
  sessionsSig: null,
  sessionOrigin: null,
  sessionKind: null,
  // Mirrored state for ratatui sessions: the authoritative grid is a list of
  // styled cells the server sends, not a decoded image.
  terminal: null,
  reconnectTimer: null,
  pollInFlight: false,
  pollQueued: false,
  pointerButton: null,
  decoder: null,
};

// --------------------------------------------------------------- utilities

function toast(message, kind = "info") {
  const node = document.createElement("div");
  node.className = `toast ${kind}`;
  node.textContent = message;
  el("toasts").append(node);
  setTimeout(() => node.remove(), 4200);
}

async function api(path, options) {
  const response = await fetch(path, options);
  if (!response.ok) {
    let detail = `${response.status}`;
    try {
      const body = await response.json();
      if (body && body.error) detail = body.error;
    } catch {
      /* non-JSON error body */
    }
    const error = new Error(detail);
    error.status = response.status;
    throw error;
  }
  if (response.status === 204) return null;
  return response.json();
}

const sessionPath = (name) => `/v1/sessions/${encodeURIComponent(name)}`;

// Session kinds whose frames are cell grids rather than encoded images.
const CELL_KINDS = new Set(["ratatui", "terminal"]);
const isCellSession = () => CELL_KINDS.has(state.sessionKind);

// Session kinds with a desktop surface and no browser navigation.
const DESKTOP_KINDS = new Set(["quickshell", "qt"]);
const isDesktopSession = () => DESKTOP_KINDS.has(state.sessionKind);

function setState(next) {
  document.body.dataset.state = next;
}

function setStatus(text, dot) {
  el("conn-status").textContent = text;
  el("conn").classList.toggle("on", dot === "on");
  el("conn").classList.toggle("warn", dot === "warn");
  el("conn").title = text;
}

function setControlsEnabled(enabled) {
  for (const id of ["zoom-label", "fit", "fullscreen", "comment"]) {
    el(id).disabled = !enabled;
  }
  el("url").disabled = !enabled || state.sessionKind !== "browser";
  el("control").disabled = !enabled;
}

// ------------------------------------------------------------- terminal

// The 16 ANSI colours, matching a typical terminal palette. Kept here so the
// viewer and the server agree on what `Color::Red` means without the server
// having to send the palette on every frame.
const ANSI = {
  black: "#1c1c1c",
  red: "#cd3131",
  green: "#0dbc79",
  yellow: "#e5e510",
  blue: "#2472c8",
  magenta: "#bc3fbc",
  cyan: "#11a8cd",
  gray: "#e5e5e5",
  dark_gray: "#666666",
  light_red: "#f14c4c",
  light_green: "#23d18b",
  light_yellow: "#f5f543",
  light_blue: "#3b8eea",
  light_magenta: "#d670d6",
  light_cyan: "#29b8db",
  white: "#ffffff",
};

const CELL_FONT = "14px 'JetBrains Mono', 'Fira Code', ui-monospace, monospace";
const CELL_W = 8.4;
const CELL_H = 18;
const TERM_FG = "#e6e6e6";
const TERM_BG = "#0d0d0d";

function hexToRgb(hex) {
  const value = parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255];
}

function colorRgb(color) {
  if (!color) return hexToRgb(TERM_FG);
  if (typeof color === "string") return hexToRgb(ANSI[color] || TERM_FG);
  if (color.rgb) return color.rgb;
  if (color.indexed !== undefined) return indexedRgb(color.indexed);
  return hexToRgb(TERM_FG);
}

function indexedRgb(index) {
  const base = [
    "#000000", "#800000", "#008000", "#808000", "#000080", "#800080", "#008080", "#c0c0c0",
    "#808080", "#ff0000", "#00ff00", "#ffff00", "#0000ff", "#ff00ff", "#00ffff", "#ffffff",
  ];
  if (index < 16) return hexToRgb(base[index]);
  if (index >= 232) {
    const level = 8 + (index - 232) * 10;
    return [level, level, level];
  }
  const n = index - 16;
  const steps = [0, 95, 135, 175, 215, 255];
  return [steps[Math.floor(n / 36)], steps[Math.floor((n / 6) % 6)], steps[n % 6]];
}

function makeTerminal(cols, rows) {
  return { cols, rows, cx: 0, cy: 0, cv: true, cells: new Map() };
}

// Apply a server snapshot. Snapshots are full, so the viewer never applies a
// delta to a grid it may have fallen behind on.
function applySnapshot(snapshot) {
  const cells = new Map();
  for (const cell of snapshot.cells) {
    cells.set(`${cell.x},${cell.y}`, cell);
  }
  state.terminal = {
    cols: snapshot.cols,
    rows: snapshot.rows,
    cx: snapshot.cx,
    cy: snapshot.cy,
    cv: snapshot.cv,
    cells,
  };
  state.frameW = snapshot.cols * CELL_W;
  state.frameH = snapshot.rows * CELL_H;
  canvas.dataset.frameReady = "true";
  canvas.dataset.frameWidth = String(state.frameW);
  canvas.dataset.frameHeight = String(state.frameH);
  canvas.dataset.termCols = String(snapshot.cols);
  canvas.dataset.termRows = String(snapshot.rows);
  if (!el("spinner").hidden) {
    el("spinner").hidden = true;
    el("empty").hidden = true;
    setControlsEnabled(true);
    setState("live");
    setStatus(`live · ${state.session}`, "on");
  }
  if (state.fit) fitView();
  else draw();
}

// Paint the mirrored grid. The transport is cells, so there is no image to
// decode and no terminal-emulator semantics to get wrong.
function drawTerminal() {
  const term = state.terminal;
  ctx.font = CELL_FONT;
  ctx.textBaseline = "top";
  ctx.fillStyle = TERM_BG;
  const width = term.cols * CELL_W;
  const height = term.rows * CELL_H;
  ctx.fillRect(state.ox, state.oy, width, height);
  ctx.save();
  ctx.translate(state.ox, state.oy);
  ctx.scale(state.scale, state.scale);
  for (const cell of term.cells.values()) {
    const x = cell.x * CELL_W;
    const y = cell.y * CELL_H;
    const [fr, fg, fb] = colorRgb(cell.fg);
    const [br, bg, bb] = colorRgb(cell.bg);
    const reversed = (cell.m & 0x40) !== 0;
    const fgHex = reversed ? `rgb(${br},${bg},${bb})` : `rgb(${fr},${fg},${fb})`;
    const bgHex = reversed ? `rgb(${fr},${fg},${fb})` : `rgb(${br},${bg},${bb})`;
    if (cell.bg !== "reset" || reversed) {
      ctx.fillStyle = bgHex;
      ctx.fillRect(x, y, CELL_W, CELL_H);
    }
    if (cell.s === " ") continue;
    ctx.fillStyle = fgHex;
    if (cell.m & 0x01) ctx.font = `bold ${CELL_FONT}`;
    else if (cell.m & 0x02) ctx.font = `300 ${CELL_FONT}`;
    else ctx.font = CELL_FONT;
    ctx.globalAlpha = cell.m & 0x80 ? 0.25 : 1;
    ctx.fillText(cell.s, x, y);
    ctx.globalAlpha = 1;
    if (cell.m & 0x08) {
      ctx.strokeStyle = fgHex;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, y + CELL_H - 2);
      ctx.lineTo(x + CELL_W, y + CELL_H - 2);
      ctx.stroke();
    }
  }
  if (term.cv) {
    ctx.fillStyle = TERM_FG;
    ctx.fillRect(term.cx * CELL_W, term.cy * CELL_H, 2, CELL_H);
  }
  ctx.restore();
}

// ------------------------------------------------------------------- view

function stageSize() {
  return { w: stage.clientWidth, h: stage.clientHeight };
}

function resizeCanvas() {
  const { w, h } = stageSize();
  canvas.width = Math.max(1, Math.round(w * dpr));
  canvas.height = Math.max(1, Math.round(h * dpr));
  overlay.width = canvas.width;
  overlay.height = canvas.height;
  draw();
  drawOverlay();
}

function draw() {
  const { w, h } = stageSize();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  frameMeta();
  // View offset, exposed like the frame size so the pan gesture is observable.
  canvas.dataset.offsetX = String(Math.round(state.ox));
  canvas.dataset.offsetY = String(Math.round(state.oy));
  if (state.terminal) {
    drawTerminal();
    frameMeta();
    return;
  }
  if (!state.frame) return;
  const width = state.frameW * state.scale;
  const height = state.frameH * state.scale;
  ctx.save();
  ctx.shadowColor = "rgba(0, 0, 0, 0.45)";
  ctx.shadowBlur = 20;
  ctx.shadowOffsetY = 4;
  ctx.imageSmoothingEnabled = state.scale < 1;
  ctx.drawImage(state.frame, state.ox, state.oy, width, height);
  ctx.restore();
  // Hairline frame around the page, like a monitor bezel.
  ctx.strokeStyle = "rgba(232, 230, 225, 0.16)";
  ctx.lineWidth = 1;
  ctx.strokeRect(
    Math.round(state.ox) + 0.5,
    Math.round(state.oy) + 0.5,
    Math.round(width) - 1,
    Math.round(height) - 1,
  );
}

// Keep the monitor readout pinned to the frame's bottom-right corner.
function frameMeta() {
  const meta = el("frame-meta");
  if (!state.frame && !state.terminal) {
    meta.hidden = true;
    return;
  }
  const { w, h } = stageSize();
  const width = state.frameW * state.scale;
  const height = state.frameH * state.scale;
  meta.textContent = `${state.frameW}×${state.frameH} · ${Math.round(state.scale * 100)}%`;
  let top = state.oy + height + 8;
  if (top + 18 > h) top = state.oy + height - 20; // drop inside when the stage is tight
  const right = Math.min(Math.max(state.ox + width, 120), w - 8);
  meta.style.left = `${right}px`;
  meta.style.top = `${Math.max(4, top)}px`;
  meta.hidden = false;
}

function center() {
  const { w, h } = stageSize();
  state.ox = (w - state.frameW * state.scale) / 2;
  state.oy = (h - state.frameH * state.scale) / 2;
}

function zoomLabel() {
  el("zoom-label").textContent = `${Math.round(state.scale * 100)}%`;
}

function fitView() {
  if (!state.frame && !state.terminal) return;
  const { w, h } = stageSize();
  state.scale = Math.min(w / state.frameW, h / state.frameH, 1);
  state.fit = true;
  center();
  draw();
  zoomLabel();
}

function actualSize() {
  if (!state.frame && !state.terminal) return;
  state.scale = 1;
  state.fit = false;
  center();
  draw();
  zoomLabel();
}

function zoomAt(px, py, factor) {
  if (!state.frame && !state.terminal) return;
  const previous = state.scale;
  const next = Math.min(4, Math.max(0.1, previous * factor));
  if (next === previous) return;
  state.ox = px - (px - state.ox) * (next / previous);
  state.oy = py - (py - state.oy) * (next / previous);
  state.scale = next;
  state.fit = false;
  draw();
  zoomLabel();
}

function zoomCenter(factor) {
  const { w, h } = stageSize();
  zoomAt(w / 2, h / 2, factor);
}

function toPage(clientX, clientY) {
  const rect = canvas.getBoundingClientRect();
  return {
    x: (clientX - rect.left - state.ox) / state.scale,
    y: (clientY - rect.top - state.oy) / state.scale,
  };
}

// ------------------------------------------------------------------- input

function setControl(owner) {
  state.control = owner;
  const controlling = owner === "human";
  document.body.classList.toggle("controlling", controlling);
  el("banner").hidden = !controlling;
  const button = el("control");
  button.textContent = controlling ? "Release control" : "Take control";
  button.classList.toggle("human", controlling);
}

function send(message) {
  if (state.ws && state.ws.readyState === WebSocket.OPEN) {
    state.ws.send(JSON.stringify(message));
  }
}

// -------------------------------------------------------------- annotation

function drawOverlay() {
  const { w, h } = stageSize();
  octx.setTransform(dpr, 0, 0, dpr, 0, 0);
  octx.clearRect(0, 0, w, h);
  // The in-progress annotation drag uses the human brass, never the agent
  // accent: it is the one rectangle the human is drawing right now.
  if (!state.drawStart || !state.drawEnd) return;
  const x = Math.min(state.drawStart.x, state.drawEnd.x);
  const y = Math.min(state.drawStart.y, state.drawEnd.y);
  const width = Math.abs(state.drawEnd.x - state.drawStart.x);
  const height = Math.abs(state.drawEnd.y - state.drawStart.y);
  octx.fillStyle = "rgba(216, 160, 78, 0.14)";
  octx.strokeStyle = "#d8a04e";
  octx.lineWidth = 1.5;
  octx.fillRect(x, y, width, height);
  octx.strokeRect(x, y, width, height);
}

function setAnnotating(on) {
  state.annotating = on;
  document.body.classList.toggle("annotating", on);
  el("comment").classList.toggle("active", on);
  if (!on) {
    state.drawStart = null;
    state.drawEnd = null;
    state.drawing = false;
    el("composer").hidden = true;
    drawOverlay();
  }
}

function overlayPos(event) {
  const rect = overlay.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

// The drawn rectangle in stage pixels, or null when nothing is selected.
function selectionRect() {
  if (!state.drawStart || !state.drawEnd) return null;
  const a = state.drawStart;
  const b = state.drawEnd;
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
  };
}

// Crop the selected rectangle out of the frame the human is looking at, as a
// base64 PNG. Capturing the pixels now is what keeps the note meaningful after
// the page navigates or reflows; a live region reference would not.
function selectionScreenshot() {
  const rect = selectionRect();
  if (!rect) return null;
  if (state.terminal) return terminalScreenshot(rect);
  if (!state.frame) return null;
  const sx = (rect.x - state.ox) / state.scale;
  const sy = (rect.y - state.oy) / state.scale;
  const left = Math.max(0, Math.floor(sx));
  const top = Math.max(0, Math.floor(sy));
  const right = Math.min(state.frameW, Math.ceil(sx + rect.width / state.scale));
  const bottom = Math.min(state.frameH, Math.ceil(sy + rect.height / state.scale));
  const width = right - left;
  const height = bottom - top;
  if (width <= 0 || height <= 0) return null;
  const shot = document.createElement("canvas");
  shot.width = width;
  shot.height = height;
  shot.getContext("2d").drawImage(state.frame, left, top, width, height, 0, 0, width, height);
  return shot.toDataURL("image/png").split(",")[1];
}

// Capture the selected region of a terminal grid by repainting it into an
// offscreen canvas at native cell size, so the note's image is crisp regardless
// of the viewer's zoom.
function terminalScreenshot(rect) {
  const term = state.terminal;
  const sx = (rect.x - state.ox) / state.scale;
  const sy = (rect.y - state.oy) / state.scale;
  const left = Math.max(0, Math.floor(sx / CELL_W));
  const top = Math.max(0, Math.floor(sy / CELL_H));
  const right = Math.min(term.cols, Math.ceil((sx + rect.width / state.scale) / CELL_W));
  const bottom = Math.min(term.rows, Math.ceil((sy + rect.height / state.scale) / CELL_H));
  const cols = right - left;
  const rows = bottom - top;
  if (cols <= 0 || rows <= 0) return null;
  const shot = document.createElement("canvas");
  shot.width = cols * CELL_W;
  shot.height = rows * CELL_H;
  const sctx = shot.getContext("2d");
  sctx.fillStyle = TERM_BG;
  sctx.fillRect(0, 0, shot.width, shot.height);
  sctx.font = CELL_FONT;
  sctx.textBaseline = "top";
  for (const cell of term.cells.values()) {
    if (cell.x < left || cell.x >= right || cell.y < top || cell.y >= bottom) continue;
    const x = (cell.x - left) * CELL_W;
    const y = (cell.y - top) * CELL_H;
    const [fr, fg, fb] = colorRgb(cell.fg);
    const [br, bg, bb] = colorRgb(cell.bg);
    const reversed = (cell.m & 0x40) !== 0;
    if (cell.bg !== "reset" || reversed) {
      sctx.fillStyle = reversed ? `rgb(${fr},${fg},${fb})` : `rgb(${br},${bg},${bb})`;
      sctx.fillRect(x, y, CELL_W, CELL_H);
    }
    if (cell.s === " ") continue;
    sctx.fillStyle = reversed ? `rgb(${br},${bg},${bb})` : `rgb(${fr},${fg},${fb})`;
    sctx.font = cell.m & 0x01 ? `bold ${CELL_FONT}` : CELL_FONT;
    sctx.fillText(cell.s, x, y);
  }
  return shot.toDataURL("image/png").split(",")[1];
}

async function sendComment() {
  const session = state.session;
  const comment = el("comment-text").value.trim();
  if (!comment || !session) return;
  try {
    await api(sessionPath(session) + "/feedback", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ comment, screenshot: selectionScreenshot() }),
    });
    // Only reset the annotation state if the viewer is still on the session
    // the note was addressed to; a switch may have happened mid-request.
    if (state.session === session) {
      el("comment-text").value = "";
      setAnnotating(false);
      state.feedbackSig = null;
    }
    toast("Note sent to the agent");
    loadFeedback();
  } catch (error) {
    toast(`Could not send note: ${error.message}`, "error");
  }
}

// --------------------------------------------------------------- feedback

async function loadFeedback() {
  // Pin the session for the whole flow: the fetch below outlives the click
  // that started it, and a response for a session the viewer has already
  // left must never reach the panel, the badge, or the screenshot URLs.
  const session = state.session;
  if (!session) return;
  let items = [];
  try {
    items = await api(sessionPath(session) + "/feedback?pending=true");
  } catch {
    return;
  }
  if (state.session !== session) return;
  const badge = el("feedback-count");
  badge.textContent = items.length;
  badge.hidden = items.length === 0;

  const signature = `${state.sessionOrigin}:${items.map((item) => item.id).join(",")}`;
  if (signature === state.feedbackSig) return;
  state.feedbackSig = signature;

  // Remembered so a later "session ended" report can say whether the human's
  // notes are still queued for the agent.
  state.feedbackPending = items.length;

  const list = el("feedback-list");
  list.replaceChildren();

  if (state.sessionOrigin !== "agent") {
    const hint = document.createElement("li");
    hint.className = "hint";
    hint.textContent = "No agent is attached to this session — notes stay unread.";
    list.append(hint);
  }

  if (!items.length) {
    const li = document.createElement("li");
    li.className = "placeholder";
    li.textContent = "No pending notes";
    list.append(li);
    return;
  }

  for (const item of items) {
    const li = document.createElement("li");
    const comment = document.createElement("span");
    comment.className = "comment";
    comment.textContent = item.comment;
    li.append(comment);

    if (item.screenshot) {
      const shot = document.createElement("img");
      shot.className = "shot";
      shot.alt = "The annotated region";
      shot.loading = "lazy";
      shot.src = sessionPath(session) + `/feedback/${item.id}/screenshot`;
      li.append(shot);
    }

    const meta = document.createElement("div");
    meta.className = "meta";
    meta.textContent = `#${item.id}`;

    const resolve = document.createElement("button");
    resolve.className = "ghost";
    resolve.textContent = "Resolve";
    resolve.onclick = async (event) => {
      event.stopPropagation();
      try {
        // Pinned to the session this row was rendered for, never to whichever
        // session the viewer shows by the time the button is clicked.
        await api(sessionPath(session) + `/feedback/${item.id}/ack`, { method: "POST" });
        state.feedbackSig = null;
        loadFeedback();
      } catch (error) {
        toast(`Could not resolve: ${error.message}`, "error");
      }
    };
    meta.append(resolve);

    li.append(meta);
    list.append(li);
  }
}

// --------------------------------------------------------------- sessions

function sessionItem(session) {
  const li = document.createElement("li");
  li.dataset.name = session.name;
  li.dataset.origin = session.origin;
  li.dataset.kind = session.kind;

  const row = document.createElement("span");
  row.className = "row";

  const name = document.createElement("span");
  name.className = "name";
  name.textContent = session.name;

  const attended = session.origin === "agent";
  const chip = document.createElement("span");
  chip.className = `chip ${attended ? "agent" : "manual"}`;
  chip.textContent = attended ? session.owner || "agent" : "no agent";
  chip.title = attended
    ? "An agent registered this session and will read its feedback"
    : "Created by hand; no agent will read feedback here";
  row.append(name, chip);

  const sub = document.createElement("span");
  sub.className = "sub";
  sub.textContent = session.kind === "browser"
    ? session.cdp_endpoint.replace(/^https?:\/\//, "")
    : `${session.kind} · ${session.path || "unknown path"}`;

  li.append(row, sub);
  li.onclick = () => connect(session.name);
  return li;
}

function groupLabel(text) {
  const li = document.createElement("li");
  li.className = "group";
  li.setAttribute("aria-hidden", "true");
  li.textContent = text;
  return li;
}

function markActive(name) {
  for (const li of el("sessions").children) {
    li.classList.toggle("active", li.dataset.name === name);
  }
}

async function loadSessions() {
  let sessions;
  try {
    sessions = await api("/v1/sessions");
  } catch {
    return;
  }
  const list = el("sessions");
  const signature = sessions
    .map((session) => `${session.name}:${session.kind}:${session.path || ""}:${session.origin}:${session.owner || ""}`)
    .join("\n");

  if (signature !== state.sessionsSig) {
    state.sessionsSig = signature;
    list.replaceChildren();

    if (!sessions.length) {
      const li = document.createElement("li");
      li.className = "placeholder";
      li.textContent = "No sessions yet";
      list.append(li);
    } else {
      const agents = sessions.filter((session) => session.origin === "agent");
      const manual = sessions.filter((session) => session.origin !== "agent");
      // Group labels only earn their keep when both kinds are present.
      const grouped = agents.length > 0 && manual.length > 0;
      if (agents.length) {
        if (grouped) list.append(groupLabel(`Agent sessions · ${agents.length}`));
        list.append(...agents.map(sessionItem));
      }
      if (manual.length) {
        if (grouped) list.append(groupLabel(`Manual · no agent · ${manual.length}`));
        list.append(...manual.map(sessionItem));
      }
    }
  }
  markActive(state.session);
}

function normalizeUrl(raw) {
  if (/^(data|about|blob):/.test(raw) || raw.includes("://")) return raw;
  return `https://${raw}`;
}

function clearReconnect() {
  if (state.reconnectTimer) {
    clearTimeout(state.reconnectTimer);
    state.reconnectTimer = null;
  }
}

function clearSession(name, ended = false) {
  if (state.session !== name) return;
  clearReconnect();
  if (state.ws) {
    state.ws.onclose = null;
    state.ws.close();
  }
  state.ws = null;
  state.session = null;
  state.sessionOrigin = null;
  state.sessionKind = null;
  state.terminal = null;
  state.frame?.close();
  state.frame = null;
  canvas.dataset.frameReady = "false";
  el("cdp").textContent = "not connected";
  el("attach").hidden = true;
  el("spinner").hidden = true;
  el("empty").hidden = false;
  el("page-title").textContent = "";
  const badge = el("feedback-count");
  badge.textContent = "0";
  badge.hidden = true;
  setControlsEnabled(false);
  setState("empty");
  if (ended) {
    // The session did not merely go idle: its backend ended, so say what
    // happened and where the human's notes went. The inbox is durable and
    // keyed by name, so the agent's next ensure/consume still receives them.
    setStatus(`ended · ${name}`, "off");
    const pending = state.feedbackPending;
    toast(
      pending > 0
        ? `session ${name} ended · ${pending} note${pending === 1 ? "" : "s"} stay queued for the agent`
        : `session ${name} ended`,
    );
  } else {
    setStatus("idle", "off");
  }
  markActive(null);
}

function scheduleReconnect(name) {
  clearReconnect();
  setStatus("reconnecting…", "warn");
  state.reconnectTimer = setTimeout(() => {
    if (state.session === name) void connect(name);
  }, 1500);
}

async function connect(name) {
  clearReconnect();
  if (state.ws) {
    state.ws.onclose = null;
    state.ws.close();
  }
  state.ws = null;
  state.decoder = { active: false, pending: null };
  state.session = name;
  state.sessionOrigin = null;
  state.sessionKind = null;
  state.terminal = null;
  state.frame?.close();
  state.frame = null;
  canvas.dataset.frameReady = "false";
  state.fit = true;
  state.feedbackSig = null;
  state.feedbackPending = 0;
  el("cdp").textContent = "connecting…";
  el("attach").hidden = true;
  el("empty").hidden = true;
  el("spinner").hidden = false;
  el("page-title").textContent = "";
  el("feedback-count").hidden = true;
  setAnnotating(false);
  setControlsEnabled(false);
  setState("connecting");
  setStatus(`connecting to ${name}…`, "warn");
  markActive(name);
  loadFeedback();

  let info;
  try {
    info = await api(sessionPath(name));
  } catch (error) {
    if (state.session !== name) return;
    if (error.status === 404) {
      // The row the user clicked belongs to a session that has since ended.
      clearSession(name, true);
    } else {
      scheduleReconnect(name);
    }
    return;
  }
  if (state.session !== name) return;

  state.sessionKind = info.kind;
  const terminal = isDesktopSession() || isCellSession();
  const endpoint = terminal
    ? `${info.kind} · ${info.path || "unknown path"}`
    : info.cdp_endpoint;
  el("cdp").textContent = endpoint;
  el("cdp").title = endpoint;
  el("attach").textContent = info.kind === "browser"
    ? `attach: bin/pw.sh -s=${name}`
    : `agent: lumen ensure ${name} --${info.kind} ${info.path || "<path>"}`;
  el("attach").hidden = false;
  state.sessionOrigin = info.origin;
  state.feedbackSig = null;
  loadFeedback();

  const proto = location.protocol === "https:" ? "wss" : "ws";
  const ws = new WebSocket(`${proto}://${location.host}${sessionPath(name)}/stream`);
  ws.binaryType = "arraybuffer";
  ws.onmessage = (event) => {
    if (state.ws === ws) onServerMessage(event);
  };
  ws.onopen = () => setControl("agent");
  ws.onerror = () => {};
  ws.onclose = () => {
    if (state.ws !== ws) return;
    state.ws = null;
    el("spinner").hidden = true;
    setControlsEnabled(false);
    setStatus("disconnected", "off");
    if (state.session === name) scheduleReconnect(name);
  };
  state.ws = ws;
}

// --------------------------------------------------------------- messages

function onServerMessage(event) {
  if (typeof event.data === "string") {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    if (message.type === "control") {
      setControl(message.owner);
      setControlsEnabled(true);
      setState("live");
      setStatus(`live · ${state.session}`, "on");
    } else if (message.type === "error") {
      toast(message.message, "error");
    }
    return;
  }
  onFrame(event.data);
}

function onFrame(buffer) {
  if (isCellSession()) {
    applyTerminalFrame(buffer);
    return;
  }
  const decoder = state.decoder;
  if (!decoder) return;
  if (decoder.active) {
    decoder.pending = buffer;
    return;
  }
  void decodeFrame(decoder, buffer);
}

// A ratatui frame is JSON describing the whole grid, not an encoded image.
function applyTerminalFrame(buffer) {
  let snapshot;
  try {
    snapshot = JSON.parse(new TextDecoder().decode(buffer));
  } catch {
    return;
  }
  applySnapshot(snapshot);
}

async function decodeFrame(decoder, buffer) {
  decoder.active = true;
  try {
    const bitmap = await createImageBitmap(new Blob([buffer], { type: "image/jpeg" }));
    if (state.decoder !== decoder) {
      bitmap.close();
      return;
    }
    if (state.frame) state.frame.close();
    state.frame = bitmap;
    state.frameW = bitmap.width;
    state.frameH = bitmap.height;
    canvas.dataset.frameReady = "true";
    canvas.dataset.frameWidth = String(bitmap.width);
    canvas.dataset.frameHeight = String(bitmap.height);
    if (!el("spinner").hidden) {
      el("spinner").hidden = true;
      el("empty").hidden = true;
      setControlsEnabled(true);
      setState("live");
      setStatus(`live · ${state.session}`, "on");
    }
    if (state.fit) fitView();
    else draw();
  } catch {
    // Ignore corrupt or superseded frames; a later frame can still recover.
  } finally {
    decoder.active = false;
    const pending = decoder.pending;
    decoder.pending = null;
    if (pending && state.decoder === decoder) void decodeFrame(decoder, pending);
  }
}

async function pollInfo() {
  if (!state.session) return;
  if (isCellSession() || isDesktopSession()) {
    el("url").value = "";
    el("url").title = "This session kind does not have browser navigation";
    const label = state.sessionKind === "qt"
      ? "Qt application"
      : state.sessionKind === "quickshell"
        ? "Quickshell desktop"
        : state.sessionKind === "ratatui"
          ? "Ratatui terminal"
          : "Terminal";
    el("page-title").textContent = label;
    document.title = `${label} · ${state.session} · Lumen`;
    return;
  }
  if (state.pollInFlight) {
    state.pollQueued = true;
    return;
  }
  state.pollInFlight = true;
  const session = state.session;
  try {
    const tabs = await api(sessionPath(session) + "/tabs");
    if (state.session !== session) return;
    const active = tabs.find((tab) => tab.active) || tabs[0];
    if (!active) return;
    if (document.activeElement !== el("url")) el("url").value = active.url || "";
    el("url").title = active.url || "";
    el("page-title").textContent = active.title || "";
    document.title = active.title ? `${active.title} · Lumen` : "Lumen";
  } catch (error) {
    if (error.status === 404) clearSession(session, true);
  } finally {
    state.pollInFlight = false;
    if (state.pollQueued) {
      state.pollQueued = false;
      setTimeout(pollInfo, 0);
    }
  }
}

// ---------------------------------------------------------------- actions

async function navigate() {
  const raw = el("url").value.trim();
  if (!raw || !state.session) return;
  try {
    await api(sessionPath(state.session) + "/navigate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ url: normalizeUrl(raw) }),
    });
    el("url").blur();
    setTimeout(pollInfo, 400);
  } catch (error) {
    toast(`Navigation blocked: ${error.message}`, "error");
  }
}

el("new-kind").onchange = () => {
  const kind = el("new-kind").value;
  el("new-path").hidden = kind === "browser";
  el("new-path").placeholder = kind === "quickshell"
    ? "shell.qml or directory"
    : kind === "qt"
      ? "Qt app command (absolute)"
      : "ratatui app binary (absolute)";
};

el("new-session").onsubmit = async (event) => {
  event.preventDefault();
  const name = el("new-name").value.trim();
  if (!name) return;
  const kind = el("new-kind").value;
  const path = el("new-path").value.trim();
  if (kind !== "browser" && !path) {
    const label = kind === "quickshell" ? "Quickshell" : kind === "qt" ? "Qt" : "Ratatui";
    toast(`${label} sessions need a path`, "error");
    el("new-path").focus();
    return;
  }
  try {
    await api("/v1/sessions", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name, kind, ...(kind !== "browser" ? { path } : {}) }),
    });
    el("new-name").value = "";
    el("new-path").value = "";
    await loadSessions();
    connect(name);
  } catch (error) {
    toast(`Could not create session: ${error.message}`, "error");
  }
};

el("url").addEventListener("keydown", (event) => {
  if (event.key === "Enter") navigate();
});

el("control").onclick = () =>
  send({ type: "control", action: state.control === "human" ? "release" : "claim" });

el("zoom-label").onclick = actualSize;
el("zoom-label").addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    zoomCenter(event.deltaY < 0 ? 1.1 : 1 / 1.1);
  },
  { passive: false },
);
el("fit").onclick = fitView;
el("fullscreen").onclick = () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else document.documentElement.requestFullscreen();
};

el("comment").onclick = () => setAnnotating(!state.annotating);
el("comment-send").onclick = sendComment;
el("comment-cancel").onclick = () => setAnnotating(false);
el("comment-text").addEventListener("keydown", (event) => {
  if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) sendComment();
});

document.addEventListener("keydown", (event) => {
  if (event.target.tagName === "INPUT" || event.target.tagName === "TEXTAREA") return;
  if (
    state.control === "human" &&
    isCellSession() &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey
  ) {
    const key = terminalKey(event);
    if (key) {
      send({ type: "key", code: key.code, mods: 0 });
      event.preventDefault();
      return;
    }
  }
  if (
    state.control === "human" &&
    event.key.length === 1 &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey
  ) {
    send({ type: "text", text: event.key });
    event.preventDefault();
    return;
  }
  if (event.key === "Escape") {
    if (state.annotating) setAnnotating(false);
    else if (state.control === "human") send({ type: "control", action: "release" });
  } else if (event.key === "+" || event.key === "=") {
    zoomCenter(1.2);
  } else if (event.key === "-" || event.key === "_") {
    zoomCenter(1 / 1.2);
  }
});

// Translate a browser key event into the service's key encoding. Printable
// characters are sent as text instead; this covers the named keys a ratatui app
// actually reads.
function terminalKey(event) {
  const named = {
    Enter: "enter",
    Backspace: "backspace",
    Tab: "tab",
    ArrowUp: "up",
    ArrowDown: "down",
    ArrowLeft: "left",
    ArrowRight: "right",
    Home: "home",
    End: "end",
    PageUp: "page_up",
    PageDown: "page_down",
    Delete: "delete",
    Insert: "insert",
    " ": { char: " " },
  };
  if (event.key === "Escape") return { code: "esc" };
  const f = /^F(\d{1,2})$/.exec(event.key);
  if (f) return { code: { f: Number(f[1]) } };
  const entry = named[event.key];
  if (!entry) return null;
  return { code: typeof entry === "string" ? entry : entry };
}

canvas.addEventListener("contextmenu", (event) => event.preventDefault());

// Pointer coordinates for input. Browser and desktop sessions use page pixels;
// a terminal session reports cells, since that is the app's coordinate space.
function terminalPointer(event) {
  if (!state.terminal) return toPage(event.clientX, event.clientY);
  const { x, y } = toPage(event.clientX, event.clientY);
  const term = state.terminal;
  return {
    x: Math.max(0, Math.min(term.cols - 1, Math.floor(x / CELL_W))),
    y: Math.max(0, Math.min(term.rows - 1, Math.floor(y / CELL_H))),
  };
}

canvas.addEventListener("pointerdown", (event) => {
  canvas.setPointerCapture(event.pointerId);
  canvas.focus();
  if (state.control === "human") {
    const { x, y } = terminalPointer(event);
    state.pointerButton = event.button === 2 ? "right" : event.button === 1 ? "middle" : "left";
    send({
      type: "mouse",
      action: "down",
      x,
      y,
      button: state.pointerButton,
    });
  } else if (event.button === 0) {
    // Not driving the page: drag the frame itself, at any zoom, the way a PDF
    // viewer pans. Taking the view out of fit mode keeps an incoming frame from
    // recentering it. Annotation runs on the overlay above this canvas, so its
    // selection drag never reaches here.
    state.panning = true;
    state.fit = false;
    state.last = { x: event.clientX, y: event.clientY };
    document.body.classList.add("panning");
  }
});

canvas.addEventListener("pointermove", (event) => {
  if (state.panning) {
    state.ox += event.clientX - state.last.x;
    state.oy += event.clientY - state.last.y;
    state.last = { x: event.clientX, y: event.clientY };
    draw();
  } else if (state.control === "human") {
    const { x, y } = terminalPointer(event);
    send({ type: "mouse", action: "move", x, y });
  }
});

canvas.addEventListener("pointerup", (event) => {
  canvas.releasePointerCapture(event.pointerId);
  if (state.control === "human") {
    const { x, y } = terminalPointer(event);
    send({ type: "mouse", action: "up", x, y, button: state.pointerButton || "left" });
  }
  state.pointerButton = null;
  state.panning = false;
  document.body.classList.remove("panning");
});

canvas.addEventListener("pointercancel", (event) => {
  canvas.releasePointerCapture(event.pointerId);
  if (state.control === "human" && state.pointerButton) {
    const { x, y } = terminalPointer(event);
    send({ type: "mouse", action: "up", x, y, button: state.pointerButton });
  }
  state.pointerButton = null;
  state.panning = false;
  document.body.classList.remove("panning");
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    if (state.control === "human" && !event.ctrlKey && !event.metaKey) {
      const { x, y } = terminalPointer(event);
      send({ type: "wheel", x, y, dx: event.deltaX, dy: event.deltaY });
      return;
    }
    const rect = canvas.getBoundingClientRect();
    zoomAt(event.clientX - rect.left, event.clientY - rect.top, event.deltaY < 0 ? 1.1 : 1 / 1.1);
  },
  { passive: false },
);

overlay.addEventListener("pointerdown", (event) => {
  if (!state.annotating) return;
  overlay.setPointerCapture(event.pointerId);
  state.drawing = true;
  state.drawStart = overlayPos(event);
  state.drawEnd = state.drawStart;
  el("composer").hidden = true;
  drawOverlay();
});

overlay.addEventListener("pointermove", (event) => {
  if (!state.annotating || !state.drawing) return;
  state.drawEnd = overlayPos(event);
  drawOverlay();
});

// Freeze the rectangle at release and open the composer for a usable region.
// Guarded by `drawing` so a pointer that keeps moving after release can no
// longer drag the selection or corrupt the region the agent reads.
function finishSelection(point) {
  if (!state.annotating || !state.drawing) return;
  state.drawing = false;
  if (point) state.drawEnd = point;
  drawOverlay();
  const region = selectionRect();
  if (region && region.width > 8 && region.height > 8) {
    el("composer").hidden = false;
    el("comment-text").focus();
  } else {
    state.drawStart = null;
    state.drawEnd = null;
    drawOverlay();
  }
}

overlay.addEventListener("pointerup", (event) => {
  if (overlay.hasPointerCapture(event.pointerId)) overlay.releasePointerCapture(event.pointerId);
  finishSelection(overlayPos(event));
});

overlay.addEventListener("pointercancel", (event) => {
  if (overlay.hasPointerCapture(event.pointerId)) overlay.releasePointerCapture(event.pointerId);
  finishSelection(null);
});

window.addEventListener("resize", () => {
  resizeCanvas();
  if (state.fit) fitView();
});

// ------------------------------------------------------------------- boot

resizeCanvas();
setControlsEnabled(false);
loadSessions();
setInterval(loadSessions, 5000);
setInterval(() => {
  if (state.session) {
    loadFeedback();
    pollInfo();
  }
}, 3000);
