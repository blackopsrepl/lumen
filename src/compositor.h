// Lumen's compositor.
//
// Lumen *is* the compositor. Every session is a nested Wayland client whose
// surface Lumen owns, renders and drives. That is not a stylistic choice: on
// Wayland a client may not read another client's pixels nor inject input into
// it, so owning the surface is the only way to show and drive a session.
//
// Nothing here uses Quick. A session's surfaces are held as QWaylandViews,
// which are plain QObjects: each advances to its client's latest buffer and
// hands back a QImage. Input goes through the seat against those same views. A
// QQuickItem-based surface would require a live Quick scene, which is exactly
// what a windowless daemon does not have.
//
// A session is every surface its application maps, not just the first one: a
// dialog, a second window and a menu popup all belong to it.
// They are composited into one frame in the output's coordinates, and a click is
// delivered to whichever surface is on top at that point — so what the human
// sees, what the agent's coordinates mean and where input lands are one model.
//
// The compositor owns no window and runs for the life of the daemon. A session
// must keep running with no viewer attached, so nothing here may depend on a
// window existing.

#pragma once

#include <QtWaylandCompositor/QWaylandXdgPopup>
#include <QtWaylandCompositor/QWaylandXdgSurface>
#include <QtWaylandCompositor/QWaylandXdgToplevel>

#include <QHash>
#include <QList>
#include <QObject>
#include <QPoint>
#include <QPointF>
#include <QPointer>
#include <QString>

class QImage;
class QWaylandClient;
class QWaylandCompositor;
class QWaylandOutput;
class QWaylandSeat;
class QWaylandSurface;
class QWaylandView;
class QWaylandXdgShell;

class LumenCompositor : public QObject {
    Q_OBJECT

  public:
    explicit LumenCompositor(QObject* parent = nullptr);
    ~LumenCompositor() override;

    /// Begin listening on `socketName`. Returns false if the socket is taken.
    bool start(const QString& socketName);

    QWaylandCompositor* compositor() const { return m_compositor; }
    QWaylandOutput* output() const { return m_output; }
    QWaylandSeat* seat() const;

    /// The topmost view of `session` on screen — or, before anything has
    /// drawn, its newest — or nullptr when it has no surface at all.
    QWaylandView* viewFor(const QString& session) const;

    /// How many surfaces `session` currently has (windows and popups).
    int surfaceCount(const QString& session) const;

    /// The session a client process id belongs to, when known.
    QString sessionForPid(const QString& pid) const;

    /// Declare which session owns a client process, before it connects.
    void expectProcess(const QString& session, qint64 pid);

    /// The session's composited frame, or a null image when nothing has drawn.
    ///
    /// The frame is in output coordinates from the top-left corner, extending
    /// as far as the session has drawn: an ordinary application's window sits
    /// at the origin, so its frame is exactly its window.
    QImage frame(const QString& session);

    /// Deliver a click at a point in the session's frame coordinates.
    ///
    /// This is the single input path: the daemon's viewer calls it for a human
    /// click and the agent socket calls it for an agent click, so the two can
    /// never diverge.
    bool click(const QString& session, const QPointF& point);

    /// Deliver text to the session's focused element.
    bool type(const QString& session, const QString& text);

    /// The environment variable a session's processes carry their session name
    /// in. Surfaces are attributed by it when the connecting process is not the
    /// one the session started — a launcher script, a forked or daemonized child.
    static constexpr const char* kSessionEnvVar = "LUMEN_SESSION";

  signals:
    void surfaceReady(const QString& session);
    void surfaceGone(const QString& session);
    void titleChanged(const QString& session, const QString& title);
    /// A session drew a new frame.
    void frameReady(const QString& session);

  private:
    /// One mapped role on a session: a toplevel or a popup.
    /// The role objects are guarded: a client may destroy its role before the
    /// surface underneath it, and the entry lives until the surface goes.
    struct Entry {
        QWaylandView* view = nullptr;
        /// Kept apart from the view: the view forgets its surface as the
        /// surface is destroyed, which is exactly when the entry needs it.
        QWaylandSurface* surface = nullptr;
        QPointer<QWaylandXdgToplevel> toplevel;
        QPointer<QWaylandXdgPopup> popup;
        /// The role's xdg_surface, and for a popup the one it opened from.
        QPointer<QWaylandXdgSurface> xdg;
        QPointer<QWaylandXdgSurface> parentXdg;
        /// Paint order band: windows, then popups.
        int band = 0;
        /// Creation order, which orders surfaces within a band.
        quint64 serial = 0;
    };

    void adoptToplevel(QWaylandXdgToplevel* toplevel);
    void adoptPopup(QWaylandXdgPopup* popup);
    /// Shared bookkeeping for every role: view, frame callbacks, teardown.
    Entry* track(QWaylandSurface* surface, const QString& session, int band);
    /// Drop an entry, by serial: it may already be gone.
    void untrack(const QString& session, quint64 serial);
    QString sessionForClient(QWaylandClient* client);
    QString sessionForSurface(QWaylandSurface* surface) const;
    Entry* entryFor(QWaylandSurface* surface) const;
    /// Where `entry`'s surface sits, in output coordinates.
    QPoint positionOf(const Entry* entry) const;
    /// The session's drawn entries in paint order, bottom first.
    QList<Entry*> stacked(const QString& session) const;
    /// The topmost drawn entry containing `point`, and the point in its surface.
    Entry* hit(const QString& session, const QPointF& point, QPointF* local) const;
    /// Report the title of the session's topmost window.
    void announceTitle(const QString& session);

    QWaylandCompositor* m_compositor = nullptr;
    QWaylandOutput* m_output = nullptr;
    QWaylandXdgShell* m_shell = nullptr;
    /// Session name by client process id.
    QHash<qint64, QString> m_sessionByPid;
    /// Every session's surfaces.
    QHash<QString, QList<Entry*>> m_entries;
    /// The surface keyboard input goes to, per session: the last one clicked.
    QHash<QString, QWaylandSurface*> m_focus;
    quint64 m_serial = 0;
};
