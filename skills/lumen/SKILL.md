---
name: lumen
description: Use when you need to drive a real web browser — navigate, click, type, fill forms, take screenshots, or scrape pages — or to read human feedback left on the shared viewer. Front-loads concrete tooling (lumen, pw.sh, playwright-cli, snapshot, browser automation).
---

# Lumen (session service)

Lumen is a single service that gives each agent an isolated Chromium or
Quickshell desktop session and gives the human a live, controllable view of it.
Browser sessions run in the container, which shares the host network, so a dev
server on the host is reachable at `http://127.0.0.1:<port>` from inside the
page.

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

For a Quickshell surface, register the absolute QML path instead of attaching
Playwright:

    lumen ensure <your-session-name> --quickshell /absolute/path/to/shell.qml --owner <your-agent>

The viewer provides the live Wayland surface and native mouse, wheel, and text
input. Quickshell sessions do not expose browser navigation, tabs, or CDP.

Every file a command produces (screenshots, pdfs, snapshots) is reported on a
line like `[lumen] artifact <session> /absolute/path.png`. Read exactly that
path — never search the filesystem for it. Artifacts belong to the session:
they live under that session's own directory and are removed when the session
closes, so read what you need while the session is open.

Useful extras: `go-back`, `reload`, `type`, `select`, `upload`, `tab-list`,
`console`, `requests`, `eval`, `pdf`, `state-save`/`state-load`.

## 2. Human feedback

The human watches your browser live at <http://localhost:8899> — they see the
same tab you act on, in real time. Do not narrate for them or paste
screenshots at them; take a screenshot only when *you* need to verify
something. They can take control or annotate a region with a comment.
Feedback is non-blocking: check your inbox between steps and adjust.

    lumen feedback <your-agent-name>            # print pending notes
    lumen feedback <your-agent-name> --consume  # print and acknowledge them

(The `lumen` client runs inside the service container; use
`podman exec lumen lumen feedback <name>` (or `docker exec …`) if `lumen` is not on your PATH.)

Never block waiting for a human unless you are explicitly asked to.

Running `pw.sh` registers your session as **agent-owned**, so the human sees it
under “Agent sessions” and knows its notes will be read. A browser the human
creates from the viewer has no agent and its feedback goes unread — use
`--owner` (or `AGENT_NAME`) to label a session when your handle is not enough.

## 3. Finish

    $PW/pw.sh -s=<your-agent-name> close

`close` ends the session: it stops the browser, reclaims its profile, and
discards the session's artifacts. Read anything you still need before closing;
detaching alone would leave the browser running.

## Notes

- Sessions are isolated per agent; do not drive another agent's `-s=` name.
- If a navigation fails with `net::ERR_BLOCKED_BY_CLIENT`, Lumen's navigation
  host policy (`[policy]` in `config/lumen.toml`) blocked that host. Ask the
  human to allow it if the page should be reachable.
- The viewer is for the human, not for you. It is always served by the service.
- If `pw.sh` reports the CLI is missing: `$PW/install-host.sh`.
- If the service is down: `$PW/status.sh`, then `$PW/up.sh`.
