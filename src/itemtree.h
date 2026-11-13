// The daemon's side of `lumen-cli tree`: ask every probe in a session for its
// item tree and place each window in the session's frame.
//
// The probe runs inside the application (see src/probe/lumenprobe.cpp) and
// knows item geometry only relative to its own window; where that window sits
// is the compositor's knowledge. A window is matched to a drawn surface by
// process id and size (the buffer is the window plus any client-side
// decoration margins), and every rectangle in its tree is then translated into
// frame coordinates — the ones `click` takes.

#pragma once

#include "compositor.h"

#include <QJsonObject>
#include <QList>
#include <QString>

namespace ItemTree {

/// Query every probe listening in `probeDir`. Each reply is one process's
/// `{pid, windows, hidden}`. Unanswered sockets (an exited process) are skipped.
QList<QJsonObject> query(const QString& probeDir, bool all, int timeoutMs = 3000);

/// Place each process's windows on `placements` and translate their
/// rectangles into frame coordinates. A window with no drawn surface keeps
/// window coordinates and is marked `"mapped": false`.
QJsonObject place(const QList<QJsonObject>& processes,
                  const QList<LumenCompositor::Placement>& placements);

/// Translate an accessibility tree's bounds from window to frame coordinates.
///
/// AT-SPI reports bounds relative to each window — a Wayland client does not
/// know where it sits — so a panel's control would otherwise be clicked at
/// the wrong place. Each top-level frame is matched by size to a window the
/// probes placed (`placed` is `place()`'s result) and its subtree moved by
/// that window's origin. Returns how many windows were translated.
int translateAccessibility(QJsonObject* tree, const QJsonObject& placed);

} // namespace ItemTree
