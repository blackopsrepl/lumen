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

} // namespace ItemTree
