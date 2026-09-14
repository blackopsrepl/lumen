---
name: lumen
description: Use when you need to drive a real web browser — navigate, click, type, fill forms, take screenshots, or scrape pages — or to read human feedback left on the shared viewer. Front-loads the concrete tools: lumen, pw.sh, playwright-cli, snapshot, browser automation.
---

# Lumen (browser service)

Lumen is a single service that gives each agent an isolated Chromium and gives
the human a live, controllable view of it. Your browser runs in the container,
which shares the host network, so a dev server on the host is reachable at
`http://127.0.0.1:<port>` from inside the page.

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

Useful extras: `go-back`, `reload`, `type`, `select`, `upload`, `tab-list`,
`console`, `requests`, `eval`, `pdf`, `state-save`/`state-load`.

## 2. Human feedback

The human watches your browser at <http://localhost:8899> and can take control
or annotate a region with a comment. Feedback is non-blocking: check your inbox
between steps and adjust.

    lumen feedback <your-agent-name>            # print pending notes
    lumen feedback <your-agent-name> --consume  # print and acknowledge them

(The `lumen` client runs inside the service container; use
`podman exec lumen lumen feedback <name>` if `lumen` is not on your PATH.)

Never block waiting for a human unless you are explicitly asked to.

Running `pw.sh` registers your session as **agent-owned**, so the human sees it
under “Agent sessions” and knows its notes will be read. A browser the human
creates from the viewer has no agent and its feedback goes unread — use
`--owner` (or `AGENT_NAME`) to label a session when your handle is not enough.

## 3. Finish

    $PW/pw.sh -s=<your-agent-name> close

## Notes

- Sessions are isolated per agent; do not drive another agent's `-s=` name.
- The viewer is for the human, not for you. It is always served by the service.
- If `pw.sh` reports the CLI is missing: `$PW/install-host.sh`.
- If the service is down: `$PW/status.sh`, then `$PW/up.sh`.
