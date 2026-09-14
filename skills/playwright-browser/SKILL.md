---
name: playwright-browser
description: Use when you need to drive a real web browser — navigate, click, type, fill forms, take screenshots, or scrape pages. Front-loads the concrete tools: playwright-cli, pw.sh, snapshot, browser automation.
---

# Playwright browser (containerized)

A browser broker runs in a Podman container; each agent gets its own isolated
Chromium. The host `playwright-cli` attaches to it over CDP. All commands below
are absolute so they work from any working directory.

Set a short handle once for readability:

    PW=__PW_BIN__

## 1. Pick a session name

Choose a unique handle for yourself and pass it as `-s=<name>` to every command.
There is no setup step: `pw.sh` provisions your isolated Chromium on first use
and attaches the CLI. If the broker has restarted, the next command
re-provisions and re-attaches automatically.

    $PW/pw.sh -s=<your-agent-name> goto https://example.com

## 2. Drive it

Always pass your session name with `-s=`. The browser is already open —
navigate with `goto` (`open` is accepted but is normalized to `goto`, since
`open` would try to launch a new local browser):

    $PW/pw.sh -s=<your-agent-name> goto https://example.com
    $PW/pw.sh -s=<your-agent-name> snapshot
    $PW/pw.sh -s=<your-agent-name> click e12
    $PW/pw.sh -s=<your-agent-name> fill e7 "hello"
    $PW/pw.sh -s=<your-agent-name> press Enter
    $PW/pw.sh -s=<your-agent-name> screenshot
    $PW/pw.sh -s=<your-agent-name> find "Sign in"

`snapshot` prints an accessibility tree with element refs (`e1`, `e2`, …) that
`click` / `fill` / `hover` / `check` accept. Prefer one `snapshot`, then act on
the refs, rather than re-snapshotting after every step.

Useful extras: `goto`, `go-back`, `reload`, `type`, `select`, `upload`,
`tab-list`, `console`, `requests`, `eval`, `pdf`, `state-save`/`state-load`
(cookies + storage), `tracing-start`/`tracing-stop`.

## 3. Finish

    $PW/pw.sh -s=<your-agent-name> close

## Notes

- The browser runs in the container, so `localhost` inside it is not this host.
  To reach a host dev server use `http://host.containers.internal:<port>`.
- Sessions are isolated per agent; do not drive another agent's `-s=` name.
- The dashboard is for the human, not for you. It stays up as a service on
  `http://localhost:8899`; you never start it.
- **Human feedback arrives asynchronously.** Between steps, check your inbox:

      $PW/feedback.sh <your-agent-name>

  If something is pending it prints the human's note and the paths to the
  annotated screenshot + region snapshot; read them and adjust, then archive:

      $PW/feedback.sh <your-agent-name> --consume

  This is non-blocking — it returns immediately when there is nothing. Never
  block waiting for a human unless you are explicitly told to.
- Only when you are *asked* to wait for a human, use
  `$PW/pw.sh -s=<name> show --annotate`; it blocks until they submit and returns
  the annotation to you.
- If `pw.sh` reports the CLI is missing: `$PW/install-host.sh`.
- If the broker is down: `$PW/status.sh`, then `$PW/up.sh`.
