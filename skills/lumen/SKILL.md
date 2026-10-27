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

Three binaries:

- `lumen-daemon` — owns the compositor and every session. Runs with no window
  and is a systemd user service, so sessions keep running when no viewer is open.
- `lumen` — the viewer. Renders the active session and forwards human input.
- `lumen-cli` — what you drive.

Every session is one Qt application plus its own private D-Bus and AT-SPI
registry. Any Qt application is hosted — Qt Widgets, QML, and **Quickshell**
configurations, panels included. There is no browser and no terminal.

## The commands

    lumen-cli status                      # every session: name, state, title, owner
    lumen-cli ensure <name> "<command>" --owner <you>
    lumen-cli stop <name>
    lumen-cli accessibility <name>        # the structured control tree
    lumen-cli click <name> <x> <y>        # a point in the session's frame
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

Click by coordinate, in the session's frame:

    lumen-cli click <your-session-name> 240 160
    lumen-cli type  <your-session-name> "hello world"

The frame is everything the application has on screen, composited the way the
human sees it: windows at the top-left corner, dialogs and menus over them,
Quickshell panels where their anchors put them on a 1920x1080 output. A click
goes to whatever is on top at that point, so a menu you just opened takes the
next click, and a click outside it closes it.

`click` and `type` **fail with exit 1 and a reason** when they cannot land: no
such session, nothing on screen yet (still starting, exited — the message names
the state; the application's own output is in its `session.log`), or nothing
drawn at that point (the message gives the frame size). Treat a non-zero exit
as "the human saw nothing happen", never as done.

Click first to focus a field, then `type` into it. **Re-read the tree after
acting** — that is the verification loop. A session's window title reflects what
the client itself calls the window, so `status` is a cheap check that an action
landed.

Reading the boundary: the tree is empty while no application is publishing
(still starting, or already exited — retry). An application that paints its own
controls and sets no `Accessible.name` publishes structural containers only, and
there is nothing here to address. **The CLI has no way to read pixels** — there
is no screenshot command — so for such an application, drive it by coordinate
and confirm through whatever it does publish (`status` title, a file it writes,
its own output), or ask the human what they see.

The application must be a Qt program — Qt Widgets, QML loaded through
`QQmlApplicationEngine` or `QQuickView`, or a Quickshell configuration. A
launcher script or an application that forks or daemonizes is fine: its windows
are attributed to the session through the environment.

## Quickshell

    lumen-cli ensure <your-session-name> "/usr/bin/quickshell -p /absolute/shell.qml" --owner <you>

`FloatingWindow`, `PanelWindow` (a layer surface) and `PopupWindow` all show and
take input. The session's `status` title is the window title, or the layer
namespace (`quickshell`) for a panel-only config. A panel whose `visible` is
bound to state that starts false — opened by IPC or a keybind — maps nothing:
`click` then says there is nothing on screen, which is correct. Quickshell's IPC
socket lives in the session's own runtime directory, not yours.

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
- The viewer is for watching: it opens fitted to the window, and the human can
  zoom (`+`/`-`, wheel, `Ctrl+0` actual size, `Ctrl+9` fit) and pan by dragging.
  Zoom is view-only — it never changes what the session believes its size is, so
  a click lands where the human aimed whatever the zoom. None of this affects
  the coordinates `click` takes, which are always in the session's own pixels.
- A human's note may carry a cropped image of what they were looking at. Fetch
  it with the `note-image` request after reading the note; it is not inlined.
