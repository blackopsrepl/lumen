# Playwright browser stack (containerized, architecture B)

A browser server for agents, split so that a human can **watch and annotate**
what an agent is doing — the Codex-style view — without VNC.

- **Container** (`browser-broker`): one **isolated Chromium per agent**, each
  with its own process, profile and random loopback CDP port.
- **Host**: the agent's `playwright-cli` attaches to its Chromium over CDP and
  drives it. `playwright-cli show` is the **live dashboard**: a screencast grid
  of every agent session, click to zoom in and take over, `--annotate` to draw
  a box and leave a comment that is returned to the caller.

There is no MCP server in this design. Agents drive the browser with
`playwright-cli` (installable skill in `skills/playwright-browser`).

```
        host (agent + human)                          container
 ┌───────────────────────────────┐        ┌──────────────────────────────┐
 │  agent runs:                  │        │  browser-broker (node)       │
 │  playwright-cli -s=alice …    │        │   ├─ Chromium  alice  :<p1>  │
 │        │  CDP attach           │        │   ├─ Chromium  bob    :<p2>  │
 │        └───────────────────────┼───────▶│   └─ …                       │
 │  human runs:                  │  CDP   │  profiles: /data/agents/<n>  │
 │  playwright-cli show  (live)  │        │  HTTP: 127.0.0.1:8090        │
 └───────────────────────────────┘        └──────────────────────────────┘
```

## Why this shape
- Playwright's injected mouse events do not move the real OS cursor, so VNC
  can never show a moving cursor. The dashboard is a **CDP screencast** with
  action/target overlays — the thing people actually mean by "watch the agent".
- A persistent browser profile is single-owner, so a single shared MCP server
  cannot host concurrent agents. Here isolation is per-agent: each agent gets
  its own Chromium, and the dashboard shows them all in one grid.

## Contents

```
Containerfile              browser image (base: mcr.microsoft.com/playwright)
browser-broker.js          per-agent Chromium broker (no deps)
entrypoint.sh              mode dispatcher copied into the image
healthcheck.sh             container healthcheck (broker /healthz)
compose.yaml               production Compose service (host network)
Makefile                   make help / make bootstrap
bin/bootstrap.sh           new-machine setup (install-host, build, up, services, skill, smoke)
bin/install-host.sh        install @playwright/cli + its browser on the host
bin/install-systemd.sh     generate + enable the user units (real paths)
bin/install-skill.sh       install the opencode skill (real path)
bin/build.sh               build the image
bin/up.sh / down.sh        start / stop the broker
bin/restart.sh             restart
bin/status.sh              broker status + registered agents
bin/logs.sh                follow broker logs
bin/shell.sh               shell inside the broker container
bin/session.sh <agent>     provision + attach an agent's browser (explicit)
bin/pw.sh                  playwright-cli wrapper (shared workspace, self-heal)
bin/dashboard.sh           live dashboard, bound to localhost
bin/feedback.sh <agent>    read a session's human-feedback inbox
bin/feedback-watch.sh      route dashboard annotations -> session inboxes
bin/smoke-test.sh          end-to-end verification
skills/playwright-browser/ agent skill for driving the browser
systemd/*.service          browser + dashboard + feedback user services
.env.example               configuration
```

## Requirements

- Rootless Podman 4+ (tested 5.4) with `podman-compose` 1.5+.
- Node.js/npm on the host (for `@playwright/cli`).
- `curl`, `python3`, `zip`/`unzip` for the helper scripts and the smoke test.
- A graphical session on the host for the dashboard/annotation windows.
- ~3 GB disk for the image.

## New machine (one command)

Everything is reproducible from this checkout:

```bash
git clone <this-repo> && cd playwright
make bootstrap        # == bin/bootstrap.sh
```

`make bootstrap` does, idempotently:

1. `bin/install-host.sh` — installs `@playwright/cli` into `~/.local` and its
   matching Chromium (needed for the dashboard/annotation windows).
2. `bin/build.sh` — builds the `playwright-browser` image.
3. `bin/up.sh` — starts the broker container.
4. `bin/install-systemd.sh` — generates the three user units with **this
   checkout's real paths** and enables them (plus linger).
5. `bin/install-skill.sh` — installs the `playwright-browser` skill for opencode
   with the real path baked in.
6. `bin/smoke-test.sh` — proves broker, per-agent isolation, cross-split
   driving, self-heal, and the localhost dashboard.

Then open http://localhost:8899 and drive a browser:

```bash
bin/pw.sh -s=alice goto https://example.com
bin/pw.sh -s=alice snapshot
```

Everything is path-relative: no `/home/<user>` or `~/tools/...` is hardcoded in
the units or the skill — `install-systemd.sh`/`install-skill.sh` substitute the
actual location. Run `make help` for the full target list.

## Multiple agents

Every agent uses a distinct `-s=<name>`. On first use, `bin/pw.sh` provisions
that agent's own Chromium in the container and attaches the CLI to it; if the
broker later restarts, the next command re-provisions and re-attaches
automatically. Agents never manage sessions by hand (`bin/session.sh <name>` is
only an explicit convenience).

`playwright-cli show` (the dashboard) lists every session; click one to watch or
control it. `MAX_AGENTS` (default 8) caps concurrent browsers.

## The dashboard and human-in-the-loop

The dashboard is **for the human**. It runs as a systemd user service bound to
`127.0.0.1:${DASHBOARD_PORT}` (default `8899`) and shows a screencast grid of
every agent browser. You can:

- **Watch** — each agent action is labeled with a callout and the target element
  is highlighted, with an animated mouse pointer, so you see the cursor move
  (enabled per session via `video-show-actions --cursor pointer`; viewer-only —
  it does not appear in the agent's accessibility snapshots).
- **Steer** — click a session, then **Enable interactive mode** to take over
  mouse/keyboard (Escape releases). You are driving the agent's *own* browser.
- **Annotate** — draw boxes and type comments, then export. The dashboard
  downloads an `annotations-*.zip` to your browser's download directory.

### Non-blocking feedback to the agent

The agent must never hang waiting for you. Exports are routed automatically:

```
~/Downloads/annotations-*.zip  --(playwright-feedback.service)-->  .workspace/feedback/<session>/<ts>.zip
```

The service parses the session name from the export's `feedback.md`
(`## screenshot N: <session> / <title> @ <url>`) and files it under that
session's inbox. The agent checks its inbox between steps with:

```bash
bin/feedback.sh <session>            # prints pending notes + image paths, or nothing
bin/feedback.sh <session> --consume  # archive what it read
```

This is asynchronous: you annotate whenever you like; the agent picks it up at
its next check and adjusts. It only blocks when *you* ask it to wait — the
explicit `bin/pw.sh -s=<agent> show --annotate` path, which opens a local
annotation window and returns the annotation to the caller.

`playwright-feedback.service` requires no extra browser config: it watches the
default download directory (`DOWNLOADS_DIR`, default `~/Downloads`).

## Configuration

Copy `.env.example` to `.env`. Key values:

| Variable | Default | Meaning |
| --- | --- | --- |
| `PLAYWRIGHT_IMAGE_VERSION` | `1.63.0` | Base image tag |
| `BROKER_HOST` / `BROKER_PORT` | `127.0.0.1` / `8090` | Broker bind (loopback) |
| `MAX_AGENTS` | `8` | Max concurrent agent browsers |
| `AGENT_HEADLESS` | `1` | Chromium headless (`0` needs a display) |
| `AGENT_NO_SANDBOX` | `1` | Chromium `--no-sandbox` (required rootless) |
| `AGENT_DATA_DIR` | `/data/agents` | Per-agent profile root |
| `AGENT_ACTION_ANNOTATIONS` | `1` | Live action callouts + moving cursor |
| `AGENT_ACTION_ANNOTATION_MS` | `1200` | How long each callout stays |
| `AGENT_EXTRA_ARGS` | – | Extra Chromium flags, word-split |
| `DASHBOARD_HOST` / `DASHBOARD_PORT` | `127.0.0.1` / `8899` | Dashboard bind |
| `DOWNLOADS_DIR` | `~/Downloads` | Where the feedback watcher looks |
| `PW_FEEDBACK_DIR` | `./.workspace/feedback` | Per-session feedback inboxes |
| `PW_WORKSPACE` | `./.workspace` | Shared CLI workspace (daemon agreement) |
| `NPM_PREFIX` | `~/.local` | Where `install-host.sh` installs the CLI |

The container uses `network_mode: host` so Chromium's loopback CDP ports are
reachable by the host CLI. Only loopback is used: the broker on
`127.0.0.1:8090`, each Chromium's DevTools on `127.0.0.1:<random>`.

## Reaching host services from the browser

The browser is in the container; `localhost` there is not your host. Use
`http://host.containers.internal:<port>` for host dev servers.

## Persistence

Each agent's profile lives at `/data/agents/<name>` inside the `browser-data`
volume, so cookies/logins survive restarts. `bin/down.sh` keeps it; remove the
volume with `podman compose down -v` to reset all agent profiles.

## systemd (rootless user services)

```bash
make install-systemd      # == bin/install-systemd.sh
```

It generates the three units from `systemd/*.service` templates, substituting
this checkout's real `PROJECT_DIR`, `HOME` and `podman-compose` path, writes them
to `~/.config/systemd/user/`, reloads, enables them, and turns on linger. Re-run
it after moving the checkout.

- `playwright-browser.service` runs `podman-compose up -d browser-broker` on
  boot and `stop` on shutdown; crash recovery comes from the container's
  `restart: unless-stopped`.
- `playwright-dashboard.service` keeps the human dashboard up on
  `127.0.0.1:${DASHBOARD_PORT}`; its `ExecStartPre`/`ExecStop` clear the CLI's
  dashboard singleton so systemd always sees a foreground process.
- `playwright-feedback.service` routes dashboard annotation exports from
  `DOWNLOADS_DIR` into each session's feedback inbox.

## Verification

```bash
bin/smoke-test.sh
```

It proves: broker healthy; two agents get **distinct** CDP endpoints
(isolation); the host CLI drives a real page across the split and reads it
back; a lost session state file self-heals; the dashboard serves on
`${DASHBOARD_PORT}` locked to loopback. The annotation round-trip is interactive
(human draws; the command returns the annotated image + snapshot + notes) and is
verified manually.

## Upgrading

```bash
# in .env
PLAYWRIGHT_IMAGE_VERSION=1.63.0
bin/build.sh && bin/restart.sh
# host CLI
CLI_VERSION=latest bin/install-host.sh
```

## Troubleshooting

- **`playwright-cli: command not found`** — run `bin/install-host.sh`; ensure
  `~/.local/bin` is on `PATH`.
- **Broker unreachable** — `bin/status.sh`, then `bin/logs.sh`; restart with
  `bin/restart.sh`.
- **Session seems stale / broker was restarted** — just run your next
  `bin/pw.sh -s=<name> …`; it re-provisions and re-attaches automatically.
- **`show --annotate` fails with "Annotation client exited with code 1"** — the
  host CLI needs its own browser and a graphical session. Run
  `playwright-cli install-browser chromium` and launch from the desktop session.
- **`localhost` inside the page is wrong** — use
  `host.containers.internal`.
- **Dashboard unreachable** — `systemctl --user status playwright-dashboard`;
  it should bind `127.0.0.1` (verify with `ss -ltn | grep "${DASHBOARD_PORT}"`).
- **Dashboard or broker won't bind** — another service may own the port; check
  `ss -ltn` and adjust `DASHBOARD_PORT`/`BROKER_PORT` in `.env`.

## Relation to the MCP design

The earlier `@playwright/mcp` Streamable HTTP server is gone. That server has
no live-view surface and a persistent profile is single-owner, which is why it
could not satisfy "watch the agent" or "many agents". If an MCP-only client is
ever required, `@playwright/mcp` can run with `--cdp-endpoint` pointing at any
agent's Chromium and coexist with this broker.
