# AGENTS.md

Native Qt application (C++ + QML, CMake) plus a resident daemon. The daemon owns
a Wayland compositor and every session is a nested Wayland client of it. There is
no HTTP surface, no container and no browser. `lumen-cli` is the agent-facing
binary; `bash` appears only in `bin/install.sh` and the CI workflows.

## Verification gates

```bash
cmake -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j$(nproc)
cd build && QT_QPA_PLATFORM=offscreen QT_QPA_PLATFORMTHEME= ctest --output-on-failure
```

`make build` and `make test` do the same locally. The pre-commit hook runs
clang-format, the build and the suite on any change to source, QML or CMake.

- Single test: `ctest -R <name>` in `build/`, or run the test binary directly.
- The tests must set `QT_QPA_PLATFORM=offscreen` and an **empty**
  `QT_QPA_PLATFORMTHEME`. Without the empty theme the platform theme plugin
  reaches for a display and every test aborts at startup.
- Tests that host a client need the `qt_fixture` target built.

## Architecture invariants

- **Lumen is the compositor.** A session's surface and its input are owned
  directly. Nothing captures the screen and nothing injects input — the two
  things a normal Wayland client cannot do, and the reason this is a compositor
  at all.
- **The compositor owns no window.** `QWaylandOutput` is created with a null
  window and an explicit mode. Nothing in `compositor.*` may depend on a window:
  the daemon must run with no viewer attached.
- **One input path.** `LumenCompositor::click`/`type` is the only place input is
  delivered; the agent socket and the viewer's human input both call it, so the
  two cannot diverge.
- **`QWaylandView`, not `QWaylandQuickItem`.** A view is a plain `QObject`;
  frames come from overriding the real virtual `QWaylandView::bufferCommitted`,
  not from a scene-graph item. A Quick item would need a live Quick scene, which
  a windowless daemon does not have.
- **A client only draws when given frame callbacks**, and only accepts input once
  told it is activated. `adoptToplevel` sends the first xdg configure (with
  `ActivatedState`), sets the view primary, and drives
  `frameStarted()`/`sendFrameCallbacks()` on damage.
- **Key events go as full `QKeyEvent`s**, which carry the text themselves. Plain
  unicode key events depend on the client mapping a scancode through its keymap
  and are dropped silently when that has not been delivered.
- **A client must take focus itself.** `forceActiveFocus()` in the client is
  required before it receives keys; a compositor cannot do that for it.
- A Window has **no** `Keys` attached property — only Items do. Use `Shortcut`
  on a Window.

## Config and deployment

- `Config::load` reads `$XDG_CONFIG_HOME/lumen/lumen.ini`.
- Sockets live in the runtime directory: `lumen-agent.sock`, `lumen-stream.sock`,
  and the compositor socket `lumen`.
- The daemon **publishes** `LUMEN_AGENT_SOCKET` and `LUMEN_STREAM_SOCKET`. A
  client inside a session has its own private `XDG_RUNTIME_DIR` and cannot derive
  those paths, so it must honour the published ones.
- A session's profile directory must be mode `0700`. Qt rejects a looser
  `XDG_RUNTIME_DIR` and silently resolves every runtime path somewhere shared.
- A Qt session starts a private `dbus-daemon` and `at-spi2-registryd` **eagerly**.
  Do not replace that with lazy activation: when the host runs systemd the bus
  launcher delegates to systemd, which cannot reach a private bus, and the tree
  goes empty.
- `packaging/lumen.service` is a systemd **user** unit; `bin/install.sh` installs
  the binaries and the unit and restarts the daemon. It is stopped by
  `systemctl --user`, never by logout.
- `/build/` and `/build-qt/` are gitignored. Committing a CMake tree also feeds
  `CMakeCache.txt` to the secret scanner, which reads package version strings as
  keys.

## QML invariants

- **Theme is a C++ type handed to QML as a context property**, not a QML `pragma
  Singleton`. A QML singleton that fails to resolve leaves every colour
  `undefined` and the window renders in the default palette with no error at all.
- Register every context property the QML names. A missing one fails the whole
  component load and no window appears.
- **A `QImage` cannot be assigned to `Image.source`.** Frames are served by a
  `QQuickImageProvider` at a URL whose token changes per frame; an unchanging URL
  is served from cache and the view freezes, and `asynchronous: true` cancels each
  load before the next frame arrives.
- A `Dialog` does not lay out its `contentItem`; use a `ColumnLayout`, whose
  implicit size it does pick up. `Column.implicitHeight` is read-only.

## Conventions

- Conventional commit subjects with scope: `fix(compositor): …`,
  `feat(viewer): …`, `test(input): …`.
- The repository default branch is `master`.
- `init-pre-commit` covers Python, Go and Rust, not C++. `.pre-commit-config.yaml`
  here is hand-written and owns the C++ gates — do not regenerate it with that
  tool.
- Do not push or publish unless asked. Remotes: `origin` is the local Forgejo
  (`http://vigilance:3002/blackopsrepl/lumen.git`), `blackopsrepl` is GitHub.
  `.github/workflows/ci.yml` declares `workflow_call`, so `release.yml` reuses it
  and a pushed `v*` tag publishes a GitHub Release with generated notes.

Workspace: re-run `git status` and `git branch` rather than trusting a snapshot.
