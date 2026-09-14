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
  highlight: null,
  feedbackSig: null,
  sessionsSig: null,
  sessionOrigin: null,
  reconnectTimer: null,
  intentionalClose: false,
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
    throw new Error(detail);
  }
  if (response.status === 204) return null;
  return response.json();
}

const sessionPath = (name) => `/v1/sessions/${encodeURIComponent(name)}`;

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
  for (const id of ["go", "url", "zoom-in", "zoom-out", "zoom-label", "fit", "fullscreen", "comment"]) {
    el(id).disabled = !enabled;
  }
  el("control").disabled = !enabled;
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
  if (!state.frame) return;
  const width = state.frameW * state.scale;
  const height = state.frameH * state.scale;
  ctx.save();
  ctx.shadowColor = "rgba(0, 0, 0, 0.55)";
  ctx.shadowBlur = 28;
  ctx.shadowOffsetY = 6;
  ctx.imageSmoothingEnabled = state.scale < 1;
  ctx.drawImage(state.frame, state.ox, state.oy, width, height);
  ctx.restore();
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
  if (!state.frame) return;
  const { w, h } = stageSize();
  state.scale = Math.min(w / state.frameW, h / state.frameH, 1);
  state.fit = true;
  center();
  draw();
  zoomLabel();
}

function actualSize() {
  if (!state.frame) return;
  state.scale = 1;
  state.fit = false;
  center();
  draw();
  zoomLabel();
}

function zoomAt(px, py, factor) {
  if (!state.frame) return;
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
  if (state.highlight) {
    const { x, y, width, height } = state.highlight;
    octx.fillStyle = "rgba(245, 184, 61, 0.12)";
    octx.strokeStyle = "rgba(245, 184, 61, 0.9)";
    octx.lineWidth = 1.5;
    octx.fillRect(x, y, width, height);
    octx.strokeRect(x, y, width, height);
  }
  if (!state.drawStart || !state.drawEnd) return;
  const x = Math.min(state.drawStart.x, state.drawEnd.x);
  const y = Math.min(state.drawStart.y, state.drawEnd.y);
  const width = Math.abs(state.drawEnd.x - state.drawStart.x);
  const height = Math.abs(state.drawEnd.y - state.drawStart.y);
  octx.fillStyle = "rgba(94, 234, 212, 0.15)";
  octx.strokeStyle = "#5eead4";
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
    el("composer").hidden = true;
    drawOverlay();
  }
}

function overlayPos(event) {
  const rect = overlay.getBoundingClientRect();
  return { x: event.clientX - rect.left, y: event.clientY - rect.top };
}

function currentRegion() {
  if (!state.drawStart || !state.drawEnd) return null;
  const toPageLocal = (point) => ({
    x: (point.x - state.ox) / state.scale,
    y: (point.y - state.oy) / state.scale,
  });
  const a = toPageLocal(state.drawStart);
  const b = toPageLocal(state.drawEnd);
  return {
    x: Math.min(a.x, b.x),
    y: Math.min(a.y, b.y),
    width: Math.abs(a.x - b.x),
    height: Math.abs(a.y - b.y),
    scale: state.scale,
  };
}

async function sendComment() {
  const comment = el("comment-text").value.trim();
  if (!comment || !state.session) return;
  try {
    await api(sessionPath(state.session) + "/feedback", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ comment, region: currentRegion() }),
    });
    el("comment-text").value = "";
    setAnnotating(false);
    state.feedbackSig = null;
    toast("Note sent to the agent");
    loadFeedback();
  } catch (error) {
    toast(`Could not send note: ${error.message}`, "error");
  }
}

function highlightRegion(region) {
  if (!region) {
    state.highlight = null;
    drawOverlay();
    return;
  }
  state.highlight = {
    x: region.x * region.scale + state.ox,
    y: region.y * region.scale + state.oy,
    width: region.width * region.scale,
    height: region.height * region.scale,
  };
  drawOverlay();
}

// --------------------------------------------------------------- feedback

async function loadFeedback() {
  if (!state.session) return;
  let items = [];
  try {
    items = await api(sessionPath(state.session) + "/feedback?pending=true");
  } catch {
    return;
  }
  el("feedback-count").textContent = items.length;

  const signature = `${state.sessionOrigin}:${items.map((item) => item.id).join(",")}`;
  if (signature === state.feedbackSig) return;
  state.feedbackSig = signature;

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

    const meta = document.createElement("div");
    meta.className = "meta";
    meta.textContent = `#${item.id}`;

    const resolve = document.createElement("button");
    resolve.className = "ghost";
    resolve.textContent = "Resolve";
    resolve.onclick = async (event) => {
      event.stopPropagation();
      try {
        await api(sessionPath(state.session) + `/feedback/${item.id}/ack`, { method: "POST" });
        state.feedbackSig = null;
        state.highlight = null;
        drawOverlay();
        loadFeedback();
      } catch (error) {
        toast(`Could not resolve: ${error.message}`, "error");
      }
    };
    meta.append(resolve);

    li.append(comment, meta);
    li.onmouseenter = () => highlightRegion(item.region);
    li.onmouseleave = () => highlightRegion(null);
    list.append(li);
  }
}

// --------------------------------------------------------------- sessions

function sessionItem(session) {
  const li = document.createElement("li");
  li.dataset.name = session.name;
  li.dataset.origin = session.origin;

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
  sub.textContent = session.cdp_endpoint.replace(/^https?:\/\//, "");

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
    .map((session) => `${session.name}:${session.origin}:${session.owner || ""}`)
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
      if (agents.length) {
        list.append(groupLabel(`Agent sessions · ${agents.length}`));
        list.append(...agents.map(sessionItem));
      }
      if (manual.length) {
        list.append(groupLabel(`Manual · no agent · ${manual.length}`));
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

function scheduleReconnect(name) {
  clearReconnect();
  setStatus("reconnecting…", "warn");
  state.reconnectTimer = setTimeout(() => {
    if (state.session === name) connect(name);
  }, 1500);
}

function connect(name) {
  clearReconnect();
  if (state.ws) {
    state.intentionalClose = true;
    state.ws.onclose = null;
    state.ws.close();
  }
  state.session = name;
  state.frame?.close();
  state.frame = null;
  state.fit = true;
  state.highlight = null;
  state.feedbackSig = null;
  el("cdp").textContent = "connecting…";
  el("attach").hidden = true;
  el("empty").hidden = true;
  el("spinner").hidden = false;
  setAnnotating(false);
  setControlsEnabled(false);
  setState("connecting");
  setStatus(`connecting to ${name}…`, "warn");
  markActive(name);
  loadFeedback();

  api(sessionPath(name))
    .then((info) => {
      el("cdp").textContent = info.cdp_endpoint;
      el("cdp").title = info.cdp_endpoint;
      el("attach").textContent = `attach: bin/pw.sh -s=${name}`;
      el("attach").hidden = false;
      state.sessionOrigin = info.origin;
      state.feedbackSig = null;
      loadFeedback();
    })
    .catch(() => {});

  const proto = location.protocol === "https:" ? "wss" : "ws";
  const ws = new WebSocket(`${proto}://${location.host}${sessionPath(name)}/stream`);
  ws.binaryType = "arraybuffer";
  ws.onmessage = onServerMessage;
  ws.onopen = () => setControl("agent");
  ws.onerror = () => {};
  ws.onclose = () => {
    el("spinner").hidden = true;
    setControlsEnabled(false);
    setStatus(state.intentionalClose ? "idle" : "disconnected", "off");
    state.intentionalClose = false;
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
  createImageBitmap(new Blob([buffer], { type: "image/jpeg" }))
    .then((bitmap) => {
      if (state.frame) state.frame.close();
      state.frame = bitmap;
      state.frameW = bitmap.width;
      state.frameH = bitmap.height;
      if (!el("spinner").hidden) {
        el("spinner").hidden = true;
        el("empty").hidden = true;
        setControlsEnabled(true);
        setState("live");
        setStatus(`live · ${state.session}`, "on");
      }
      if (state.fit) fitView();
      else draw();
      pollInfo();
    })
    .catch(() => {});
}

async function pollInfo() {
  if (!state.session) return;
  try {
    const tabs = await api(sessionPath(state.session) + "/tabs");
    const active = tabs.find((tab) => tab.active) || tabs[0];
    if (!active) return;
    if (document.activeElement !== el("url")) el("url").value = active.url || "";
    el("url").title = active.url || "";
    document.title = active.title ? `${active.title} · Lumen` : "Lumen";
  } catch {
    /* ignore transient tab errors */
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

el("new-session").onsubmit = async (event) => {
  event.preventDefault();
  const name = el("new-name").value.trim();
  if (!name) return;
  try {
    await api("/v1/sessions", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ name }),
    });
    el("new-name").value = "";
    await loadSessions();
    connect(name);
  } catch (error) {
    toast(`Could not create session: ${error.message}`, "error");
  }
};

el("go").onclick = navigate;
el("url").addEventListener("keydown", (event) => {
  if (event.key === "Enter") navigate();
});

el("control").onclick = () =>
  send({ type: "control", action: state.control === "human" ? "release" : "claim" });

el("zoom-in").onclick = () => zoomCenter(1.2);
el("zoom-out").onclick = () => zoomCenter(1 / 1.2);
el("zoom-label").onclick = actualSize;
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
  if (event.key === "Escape") {
    if (state.annotating) setAnnotating(false);
    else if (state.control === "human") send({ type: "control", action: "release" });
  } else if (event.key === "+" || event.key === "=") {
    zoomCenter(1.2);
  } else if (event.key === "-" || event.key === "_") {
    zoomCenter(1 / 1.2);
  }
});

canvas.addEventListener("contextmenu", (event) => event.preventDefault());

canvas.addEventListener("pointerdown", (event) => {
  canvas.setPointerCapture(event.pointerId);
  if (state.control === "human") {
    const { x, y } = toPage(event.clientX, event.clientY);
    send({
      type: "mouse",
      action: "down",
      x,
      y,
      button: event.button === 2 ? "right" : event.button === 1 ? "middle" : "left",
    });
  } else if (state.scale > 1) {
    state.panning = true;
    state.last = { x: event.clientX, y: event.clientY };
  }
});

canvas.addEventListener("pointermove", (event) => {
  if (state.panning) {
    state.ox += event.clientX - state.last.x;
    state.oy += event.clientY - state.last.y;
    state.last = { x: event.clientX, y: event.clientY };
    draw();
  } else if (state.control === "human") {
    const { x, y } = toPage(event.clientX, event.clientY);
    send({ type: "mouse", action: "move", x, y });
  }
});

canvas.addEventListener("pointerup", (event) => {
  canvas.releasePointerCapture(event.pointerId);
  if (state.control === "human") {
    const { x, y } = toPage(event.clientX, event.clientY);
    send({ type: "mouse", action: "up", x, y, button: "left" });
  }
  state.panning = false;
});

canvas.addEventListener(
  "wheel",
  (event) => {
    event.preventDefault();
    if (state.control === "human" && !event.ctrlKey && !event.metaKey) {
      const { x, y } = toPage(event.clientX, event.clientY);
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
  state.drawStart = overlayPos(event);
  state.drawEnd = state.drawStart;
  drawOverlay();
});

overlay.addEventListener("pointermove", (event) => {
  if (!state.annotating || !state.drawStart) return;
  state.drawEnd = overlayPos(event);
  drawOverlay();
});

overlay.addEventListener("pointerup", (event) => {
  if (!state.annotating || !state.drawStart) return;
  state.drawEnd = overlayPos(event);
  drawOverlay();
  const region = currentRegion();
  if (region && region.width > 8 && region.height > 8) {
    el("composer").hidden = false;
    el("comment-text").focus();
  }
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
