---
name: lumen
description: Use when you need to drive a real web browser — navigate, click, type, fill forms, take screenshots, or scrape pages — or to work on a Qt/desktop application through its accessibility tree, run a terminal session, or read human feedback left on the shared viewer. Front-loads concrete tooling (lumen, pw.sh, playwright-cli, accessibility, snapshot, browser automation).
---

# Lumen (session service)

Lumen is one service that gives every agent an isolated session and gives the
human a live, controllable view of the same session. Sessions run in the
selected container runtime, which shares the host network, so a dev server on
the host is reachable at `http://127.0.0.1:<port>` from inside a page.

There are five session kinds. Pick the one that matches the surface:

| kind | what it runs | how an agent reads it |
| --- | --- | --- |
| `browser` | Chromium | DOM / accessibility snapshot over CDP |
| `qt` | any Qt GUI application | structured accessibility tree |
| `quickshell` | a Quickshell QML shell | screenshots only |
| `ratatui` | an app linking `lumen-ratatui` | styled cell grid as text |
| `terminal` | any program in a PTY | parsed cell grid as text |

Set a short handle once:

    PW=__PW_BIN__

## 1. Drive the browser

Pick a unique session name and pass it as `-s=<name>` to every command.
`pw.sh` asks Lumen to ensure your browser and attaches `playwright-cli` over
CDP automatically.

    $PW/pw.sh -s=<your-agent-name> goto https://example.com
    $PW/pw.sh -s=<your-agent-name> snapshot
    $PW/pw.sh -s=<your-agent-name> click e12
    $PW/pw.sh -s=<your-agent-name> fill e7 "hello"
    $PW/pw.sh -s=<your-agent-name> press Enter
    $PW/pw.sh -s=<your-agent-name> screenshot

`snapshot` prints an accessibility tree with refs (`e1`, `e2`, …) that `click` /
`fill` / `hover` / `check` accept. Take one snapshot, then act on the refs.

Every file a command produces (screenshots, pdfs, snapshots) is reported on a
line like `[lumen] artifact <session> /absolute/path.png`. Read exactly that
path — never search the filesystem for it. Artifacts belong to the session:
they live under that session's own directory and are removed when the session
closes, so read what you need while the session is open.

Useful extras: `go-back`, `reload`, `type`, `select`, `upload`, `tab-list`,
`console`, `requests`, `eval`, `pdf`, `state-save`/`state-load`.

## 2. Work on a Qt application

A Qt session runs an absolute executable (plus arguments) on its own headless
Wayland output. The application publishes its controls as a structured
accessibility tree, so you address a control by role, name, and bounds instead
of guessing pixels from a screenshot:

    lumen ensure <your-session-name> --qt "/absolute/app --flag" --owner <your-agent>
    lumen accessibility <your-session-name>
    lumen click <your-session-name> "<ref>"
    lumen type <your-session-name> "text"

`lumen accessibility` prints JSON. Each node has `ref`, `role`, `name`,
`description`, `states`, optional `bounds` (`x`, `y`, `width`, `height`), and
`children`. Find the control you need by name and role, then act on its `ref`:

    lumen click <your-session-name> ":1.5|/org/a11y/atspi/accessible/42"
    lumen type  <your-session-name> "hello world"

Click first to focus a text field, then `type` into it. Re-read the tree after
acting to confirm the state changed — that is the verification loop.

Reading the boundary: the body is the registry root node plus a `stats`
object (`applications`, `nodes`, `named`, `interior`, `max_depth`). The
endpoint answers **409** while no application is publishing (still starting,
or already exited — retry), and **409** when the application published nothing
addressable (`stats.named == 0` and `stats.interior == 0`: a canvas that draws
its own controls, so only screenshots and pointer input apply). `stats.named ==
0` with `stats.interior > 0` means it published real objects under no names;
the tree is served with a `x-lumen-tree-warning` header (`lumen accessibility`
prints it on stderr) — address those controls by `ref` and `bounds`. Window
titles do not count as names. Bare `Rectangle`s in QML never appear in the
tree unless they set `Accessible.name`.

The same operations over HTTP, if you are not using the CLI:

    GET  /v1/sessions/<name>/accessibility
    POST /v1/sessions/<name>/accessibility/click   {"ref": "…"}
    POST /v1/sessions/<name>/accessibility/type    {"text": "…"}

The application must be a normal Qt program — Qt Widgets, or QML run through
`QQmlApplicationEngine`/`QQuickView`. Quickshell shells do **not** publish an
accessibility tree (use screenshots instead).

In a containerized deployment the binary must be under `LUMEN_PROJECTS_ROOT`,
the read-only host root Compose mounts at the same absolute path. Point that
root at the host directory containing the application so the path you pass stays
valid inside the service.

## 3. Run a Quickshell surface

Quickshell sessions run one headless Sway compositor and one Quickshell process
per session. The path must be an absolute path to `shell.qml` or its containing
directory, and it must be readable by the Lumen process:

    lumen ensure <your-session-name> --quickshell /absolute/path/to/shell.qml --owner <your-agent>

In a containerized deployment the path must be under `LUMEN_PROJECTS_ROOT`,
which Compose mounts read-only at the same absolute path inside the container.
Set that variable in `.env` to the common host directory containing the QML
file and its imports or assets before calling `ensure`.

The viewer provides the live Wayland surface and native mouse, wheel, and text
input. Quickshell sessions do not expose browser navigation, tabs, CDP, or an
accessibility tree.

## 4. Run a terminal

**Terminal sessions run any program, unmodified**, in a real pseudoterminal.
Lumen parses the output into the same structured grid every other session kind
produces, so you can read the screen as text with no screenshot and no vision:

    curl -s -X POST http://127.0.0.1:8899/v1/sessions \
      -H 'content-type: application/json' \
      -d '{"name":"<your-session-name>","kind":"terminal","path":"/usr/bin/htop","origin":"agent","owner":"<your-agent>"}'
    curl -s http://127.0.0.1:8899/v1/sessions/<your-session-name>/screen

**Ratatui sessions run a Lumen-native app** — a binary that links the
`lumen-ratatui` crate. There is no PTY and no emulator, which makes it the
cheaper option when you control the app's source:

    lumen ensure <your-session-name> --ratatui "/absolute/app" --owner <your-agent>

Both kinds render to cells, not pixels; read `GET /v1/sessions/<name>/screen`
rather than `/screenshot`.

## 5. Human feedback

The human watches your session live at <http://localhost:8899> — they see the
same surface you act on, in real time. Do not narrate for them or paste
screenshots at them; take a screenshot only when *you* need to verify
something. They can take control or annotate a region with a comment; Lumen
captures that region as a PNG when they send it, so a note keeps showing what
they meant even after the surface changes. Feedback is non-blocking: check your
inbox between steps and adjust.

    lumen feedback <your-session-name>            # print pending notes
    lumen feedback <your-session-name> --consume  # print and acknowledge them

When a note has a screenshot, the command saves it to an absolute path under
`$LUMEN_FEEDBACK_DIR` (default the system temp dir) and prints that path plus
the serving endpoint. Read the image, then act on the note.

(The `lumen` client runs inside the service container; use
`$PW/ctr.sh exec lumen lumen <command>` if `lumen` is not on your PATH.)

Never block waiting for a human unless you are explicitly asked to.

Starting a session through `pw.sh` or `lumen ensure` registers it as
**agent-owned**, so the human sees it under "Agent sessions" and knows its
notes will be read. A session the human creates from the viewer has no agent
and its feedback goes unread — use `--owner` (or `AGENT_NAME`) to label a
session when your handle is not enough.

## 6. Finish

    $PW/pw.sh -s=<your-agent-name> close     # browser
    lumen stop <your-session-name>           # any kind, stops and purges it

Ending a session stops its process, reclaims its profile, and discards the
session's artifacts. Read anything you still need before closing; detaching
alone would leave it running.

## Notes

- Sessions are isolated per agent; do not drive another agent's session name.
- If a browser navigation fails with `net::ERR_BLOCKED_BY_CLIENT`, Lumen's
  navigation host policy (`[policy]` in `config/lumen.toml`) blocked that host.
  Ask the human to allow it if the page should be reachable. The policy applies
  to browser navigation only.
- The viewer is for the human, not for you. It is always served by the service.
- If `pw.sh` reports the CLI is missing: `$PW/install-host.sh`.
- If the service is down: `$PW/status.sh`, then `$PW/up.sh`.
