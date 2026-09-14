const el = (id) => document.getElementById(id);

const canvas = el("screen");
const stage = el("stage");
const ctx = canvas.getContext("2d");
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
};

function send(message) {
  if (state.ws && state.ws.readyState === WebSocket.OPEN) {
    state.ws.send(JSON.stringify(message));
  }
}

function stageSize() {
  return { w: stage.clientWidth, h: stage.clientHeight };
}

function resizeCanvas() {
  const { w, h } = stageSize();
  canvas.width = Math.max(1, Math.round(w * dpr));
  canvas.height = Math.max(1, Math.round(h * dpr));
  draw();
}

function draw() {
  const { w, h } = stageSize();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, w, h);
  if (!state.frame) return;
  ctx.imageSmoothingEnabled = state.scale < 1;
  ctx.drawImage(
    state.frame,
    state.ox,
    state.oy,
    state.frameW * state.scale,
    state.frameH * state.scale,
  );
}

function center() {
  const { w, h } = stageSize();
  state.ox = (w - state.frameW * state.scale) / 2;
  state.oy = (h - state.frameH * state.scale) / 2;
}

function fitView() {
  if (!state.frame) return;
  const { w, h } = stageSize();
  state.scale = Math.min(w / state.frameW, h / state.frameH);
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

function zoomLabel() {
  el("zoom-label").textContent = `${Math.round(state.scale * 100)}%`;
}

function toPage(clientX, clientY) {
  const rect = canvas.getBoundingClientRect();
  return {
    x: (clientX - rect.left - state.ox) / state.scale,
    y: (clientY - rect.top - state.oy) / state.scale,
  };
}

function setControl(owner) {
  state.control = owner;
  const controlling = owner === "human";
  document.body.classList.toggle("controlling", controlling);
  const button = el("control");
  button.textContent = controlling ? "Release control" : "Take control";
  button.classList.toggle("human", controlling);
}

function onFrame(buffer) {
  createImageBitmap(new Blob([buffer], { type: "image/jpeg" })).then((bitmap) => {
    if (state.frame) state.frame.close();
    state.frame = bitmap;
    state.frameW = bitmap.width;
    state.frameH = bitmap.height;
    if (state.fit) fitView();
    else draw();
  });
}

function onServerMessage(event) {
  if (typeof event.data === "string") {
    let message;
    try {
      message = JSON.parse(event.data);
    } catch {
      return;
    }
    if (message.type === "control") setControl(message.owner);
    return;
  }
  onFrame(event.data);
}

function connect(name) {
  if (state.ws) {
    state.ws.onclose = null;
    state.ws.close();
  }
  state.session = name;
  state.frame?.close();
  state.frame = null;
  state.fit = true;
  el("empty").hidden = true;
  el("cdp").textContent = "";
  markActive(name);

  fetch(`/v1/sessions/${encodeURIComponent(name)}`)
    .then((r) => (r.ok ? r.json() : null))
    .then((info) => {
      if (info) el("cdp").textContent = info.cdp_endpoint;
    })
    .catch(() => {});

  const proto = location.protocol === "https:" ? "wss" : "ws";
  const ws = new WebSocket(
    `${proto}://${location.host}/v1/sessions/${encodeURIComponent(name)}/stream`,
  );
  ws.binaryType = "arraybuffer";
  ws.onmessage = onServerMessage;
  ws.onopen = () => {
    el("conn").classList.add("on");
    el("conn").title = `connected to ${name}`;
    setControl("agent");
  };
  ws.onclose = () => {
    el("conn").classList.remove("on");
    el("conn").title = "disconnected";
  };
  state.ws = ws;
}

function markActive(name) {
  for (const li of el("sessions").children) {
    li.classList.toggle("active", li.dataset.name === name);
  }
}

function sessionItem(session) {
  const li = document.createElement("li");
  li.textContent = session.name;
  li.dataset.name = session.name;
  li.title = session.cdp_endpoint;
  li.onclick = () => connect(session.name);
  return li;
}

async function loadSessions() {
  const sessions = await fetch("/v1/sessions").then((r) => r.json());
  const list = el("sessions");
  const desired = sessions.map((session) => session.name).join("\n");
  const current = [...list.children].map((li) => li.dataset.name).join("\n");

  if (desired !== current) {
    list.replaceChildren(...sessions.map(sessionItem));
  }
  for (const li of list.children) {
    const session = sessions.find((s) => s.name === li.dataset.name);
    if (session) li.title = session.cdp_endpoint;
  }
  markActive(state.session);
}

el("new-session").onsubmit = async (event) => {
  event.preventDefault();
  const name = el("new-name").value.trim();
  if (!name) return;
  const response = await fetch("/v1/sessions", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ name }),
  });
  if (!response.ok) {
    alert((await response.json()).error || "could not create session");
    return;
  }
  el("new-name").value = "";
  await loadSessions();
  connect(name);
};

el("go").onclick = () => {
  const url = el("url").value.trim();
  if (!url || !state.session) return;
  fetch(`/v1/sessions/${encodeURIComponent(state.session)}/navigate`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ url }),
  });
};

el("url").addEventListener("keydown", (event) => {
  if (event.key === "Enter") el("go").click();
});

el("control").onclick = () => {
  send({ type: "control", action: state.control === "human" ? "release" : "claim" });
};

el("zoom-in").onclick = () => {
  const { w, h } = stageSize();
  zoomAt(w / 2, h / 2, 1.2);
};
el("zoom-out").onclick = () => {
  const { w, h } = stageSize();
  zoomAt(w / 2, h / 2, 1 / 1.2);
};
el("fit").onclick = fitView;
el("one").onclick = actualSize;
el("fullscreen").onclick = () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else document.documentElement.requestFullscreen();
};

canvas.addEventListener("pointerdown", (event) => {
  canvas.setPointerCapture(event.pointerId);
  if (state.control === "human") {
    const { x, y } = toPage(event.clientX, event.clientY);
    send({ type: "mouse", action: "down", x, y, button: "left" });
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
    if (state.control === "human" && !event.ctrlKey) {
      const { x, y } = toPage(event.clientX, event.clientY);
      send({ type: "wheel", x, y, dx: event.deltaX, dy: event.deltaY });
      return;
    }
    const rect = canvas.getBoundingClientRect();
    zoomAt(event.clientX - rect.left, event.clientY - rect.top, event.deltaY < 0 ? 1.1 : 1 / 1.1);
  },
  { passive: false },
);

window.addEventListener("resize", () => {
  resizeCanvas();
  if (state.fit) fitView();
});

resizeCanvas();
loadSessions();
setInterval(loadSessions, 5000);
