---
name: lumen
description: Use when you need to drive a Qt desktop application as an agent — read its controls, click them, type into it — or to leave and read human feedback on a session, or to run a GUI app in a hosted session on a Linux desktop. Covers the lumen-cli commands (ensure, status, click, type, accessibility, feedback).
---

# Lumen

Lumen hosts Qt applications for agents and gives the human a live view of the
same session. It **is** the compositor: every session is a nested Wayland client
of Lumen's own daemon, so its pixels and its input belong to Lumen directly.
There is no screen capture, no input injection, and no network surface — you
reach it over a unix socket in the user's runtime directory.

Two processes:

- `lumen-daemon` — owns the compositor and every session. Runs with no window
  and is a systemd user service, so sessions keep running when no viewer is open.
- `lumen` — the viewer. Renders the active session and forwards human input.
- `lumen-cli` — what you drive.

Every session is one Qt application plus its own private D-Bus and AT-SPI
registry. Only Qt applications are hosted: there is no browser, no terminal and
no Quickshell path any more.

## The commands

    lumen-cli status                      # every session: name, state, title, owner
    lumen-cli ensure <name> "<command>" --owner <you>
    lumen-cli stop <name>
    lumen-cli accessibility <name>        # the structured control tree
    lumen-cli click <name> <x> <y>        # a point in the session's own coordinates
    lumen-cli type  <name> "<text>"
    lumen-cli feedback <name>             # pending human notes
    lumen-cli feedback <name> --consume   # read and acknowledge them
    lumen-cli ack <name> --id <id>        # acknowledge one note

The command is an absolute program plus arguments, quoted as **one** argument,
because the CLI takes exactly one positional for it:

    lumen-cli ensure gitnaga "/opt/gitnaga/bin/gitnaga" --owner desktop-agent

## Working with a Qt application

    lumen-cli ensure <your-session-name> "/absolute/app --flag" --owner <your-agent>
    lumen-cli accessibility <your-session-name>

The tree is JSON. Each node has `ref`, `role`, `name`, `description`, `states`,
optional `bounds` (`x`, `y`, `width`, `height`) and `children`, plus a `stats`
object (`applications`, `nodes`, `named`, `interior`, `max_depth`) so you can
tell a populated tree from an empty one without guessing.

Click by coordinate, in the session's own pixel space:

    lumen-cli click <your-session-name> 240 160
    lumen-cli type  <your-session-name> "hello world"

Click first to focus a field, then `type` into it. **Re-read the tree after
acting** — that is the verification loop. A session's window title reflects what
the client itself calls the window, so `status` is a cheap check that an action
landed.

Reading the boundary: the tree is empty while no application is publishing
(still starting, or already exited — retry). An application that paints its own
controls and sets no `Accessible.name` publishes structural containers only,
and there is nothing to address; screenshots and pointer input still apply. Bare
`Rectangle`s in QML never appear unless they set `Accessible.name`.

The application must be a normal Qt program — Qt Widgets, or QML loaded through
`QQmlApplicationEngine` or `QQuickView`.

## Human feedback

The human watches your session live in the viewer and can take control or
annotate a region with a comment. Lumen crops those pixels out of the frame the
moment they send the note, so it keeps showing what they meant even after the
session redraws.

Feedback is non-blocking: check your inbox between steps and adjust.

    lumen-cli feedback <your-session-name>            # print pending notes
    lumen-cli feedback <your-session-name> --consume  # print and acknowledge them

A note carries `id`, `comment`, `author`, `created_at` and a `screenshot`
boolean. The annotated PNG is fetched separately as base64:

    lumen-cli feedback <name> --consume     # then note the id
    # the image for note <id>:
    #   {"cmd":"note-image","name":"<name>","id":<id>}  → {"image":"<base64 png>"}

`--consume` acknowledges exactly the notes it returned, so a second call with
`--consume` returns nothing. Do not consume a note you have not acted on: use
`feedback` without `--consume` to read, and `ack --id` to acknowledge one note
after you have dealt with it.

Never block waiting for a human unless you are explicitly asked to.

Session names are addressed by name, and notes are keyed by session name: a
session that ends with unacknowledged notes leaves them queued, and the next
`ensure` of the same name hands them to whoever attaches.

## Driving Lumen with Lumen

The viewer is itself a Qt application, so you can host it as a session — a good
end-to-end check that hosting, streaming and input all work:

    lumen-cli ensure lumen-self "$(command -v lumen)" --owner <you>

## Finish

    lumen-cli stop <your-session-name>

This stops the process, reclaims its private profile directory and discards
anything the session held. Read what you still need first.

## Notes

- Sessions are isolated per agent; do not drive another agent's session name.
- The viewer is for the human, not for you. Do not narrate at them or paste
  screenshots at them; look at the frame only when *you* need to verify
  something.
- If `lumen-cli` is not on your PATH it is at `~/.local/bin/lumen-cli`.
- If the daemon is down there is no socket: `systemctl --user status
  lumen.service`, or the human can start it from the viewer's Settings.
