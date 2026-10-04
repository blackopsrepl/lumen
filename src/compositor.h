// Lumen's compositor.
//
// Lumen *is* the compositor. Every session is a nested Wayland client whose
// surface Lumen owns, renders and drives. That is not a stylistic choice: on
// Wayland a client may not read another client's pixels nor inject input into
// it, so owning the surface is the only way to show and drive a session.
//
// Nothing here uses Quick. A session's surface is held as a QWaylandView, which
// is a plain QObject: it advances to the client's latest buffer and hands back a
// QImage. Input goes through the seat against that same view. A QQuickItem-based
// surface would require a live Quick scene, which is exactly what a windowless
// daemon does not have.
//
// The compositor owns no window and runs for the life of the daemon. A session
// must keep running with no viewer attached, so nothing here may depend on a
// window existing.

#pragma once

#include <QHash>
#include <QObject>
#include <QString>

class QWaylandCompositor;
class QWaylandOutput;
class QWaylandSeat;
class QWaylandView;
class QWaylandXdgShell;
class QWaylandXdgToplevel;

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

    /// The view rendering `session`, or nullptr.
    QWaylandView* viewFor(const QString& session) const;

    /// The session a client process id belongs to, when known.
    QString sessionForPid(const QString& pid) const;

    /// Declare which session owns a client process, before it connects.
    void expectProcess(const QString& session, qint64 pid);

    /// The session's latest frame, or a null image when it has not drawn yet.
    QImage frame(const QString& session);

    /// Deliver a click at a point in the session's surface coordinates.
    ///
    /// This is the single input path: the daemon's viewer calls it for a human
    /// click and the agent socket calls it for an agent click, so the two can
    /// never diverge.
    bool click(const QString& session, const QPointF& point);

    /// Deliver text to the session's focused element.
    bool type(const QString& session, const QString& text);

  signals:
    void surfaceReady(const QString& session);
    void surfaceGone(const QString& session);
    void titleChanged(const QString& session, const QString& title);
    /// A session drew a new frame.
    void frameReady(const QString& session);

  private:
    void adoptToplevel(QWaylandXdgToplevel* toplevel);

    QWaylandCompositor* m_compositor = nullptr;
    QWaylandOutput* m_output = nullptr;
    QWaylandXdgShell* m_shell = nullptr;
    /// Session name by client process id.
    QHash<qint64, QString> m_sessionByPid;
    /// View by session name.
    QHash<QString, QWaylandView*> m_viewBySession;
    QHash<QWaylandView*, QString> m_sessionByView;
};
