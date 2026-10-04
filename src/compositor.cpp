// Lumen's compositor.
//
// Lumen *is* the compositor. Every session is a nested Wayland client whose
// surface Lumen owns, renders and drives. That is not a stylistic choice: on
// Wayland a client may not read another client's pixels nor inject input into
// it, so owning the surface is the only way to show and drive a session.
//
// The shape follows Qt's own compositor examples (minimal-cpp): a
// QWaylandCompositor subclass, a QWaylandView subclass per surface, input
// delivered through the seat against the view. It deliberately uses no Quick:
// a QWaylandView is a plain QObject that advances to the client's buffer and
// hands back an image, so the daemon needs no scene and no window.
//
// Nothing here depends on a window existing, because a session must keep
// running while no viewer is attached.

#include "compositor.h"

#include <QImage>
#include <QPointF>
#include <QRegion>

#include <QtWaylandCompositor/QWaylandCompositor>
#include <QtWaylandCompositor/QWaylandOutput>
#include <QtWaylandCompositor/QWaylandSeat>
#include <QtWaylandCompositor/QWaylandSurface>
#include <QtWaylandCompositor/QWaylandView>
#include <QtWaylandCompositor/QWaylandXdgShell>
#include <QtWaylandCompositor/QWaylandXdgSurface>
#include <QtWaylandCompositor/QWaylandXdgToplevel>

namespace {

/// The compositor's headless output size.
///
/// An output is required even with no window: a client is told the size it may
/// occupy, and pointer focus is expressed in output coordinates, so an output
/// with no geometry leaves input with nowhere to land.
constexpr int kOutputWidth = 1920;
constexpr int kOutputHeight = 1080;
constexpr int kOutputRefresh = 60000;

/// A session's surface, as Lumen renders it.
///
/// This is a QWaylandView, not a scene-graph item: `advance()` moves to the
/// client's newest buffer and `currentBuffer()` yields it as a QImage. That is
/// what lets the daemon composite with no window and no Quick scene.
class SessionView : public QWaylandView {
    Q_OBJECT
public:
    explicit SessionView(QObject *parent = nullptr) : QWaylandView(parent) {}

    /// The client's latest frame, or a null image when it has not drawn yet.
    ///
    /// Stores the buffer from the surface's bufferCommitted signal and returns
    /// its image. For SHM buffers (which software-rendered clients use) this
    /// needs no GL context and no render thread.
    QImage frame() {
        if (m_buffer.isNull()) {
            return QImage();
        }
        return m_buffer.image();
    }

    QSize size() const {
        return surface() ? surface()->destinationSize() : QSize();
    }

protected:
    /// Qt calls this when the client commits a new buffer. Storing it here is
    /// what makes frame() work: the buffer is the client's latest pixels, and
    /// reading an SHM buffer needs no GL context and no render thread.
    void bufferCommitted(const QWaylandBufferRef &buffer, const QRegion &) override {
        m_buffer = buffer;
    }

private:
    QWaylandBufferRef m_buffer;
};

} // namespace

LumenCompositor::LumenCompositor(QObject *parent) : QObject(parent) {
    m_compositor = new QWaylandCompositor(this);

    // The output is created without a window. QWaylandOutput accepts a null
    // window as long as it is given a mode, which is what makes the compositor
    // usable with no viewer attached.
    m_output = new QWaylandOutput(m_compositor, nullptr);
    const QWaylandOutputMode mode(QSize(kOutputWidth, kOutputHeight), kOutputRefresh);
    m_output->addMode(mode, true);
    m_output->setCurrentMode(mode);
    m_output->setManufacturer(QStringLiteral("Lumen"));
    m_output->setModel(QStringLiteral("Lumen session surface"));

    m_shell = new QWaylandXdgShell(m_compositor);
    connect(m_shell, &QWaylandXdgShell::toplevelCreated, this,
            [this](QWaylandXdgToplevel *toplevel, QWaylandXdgSurface *) {
                adoptToplevel(toplevel);
            });
}

LumenCompositor::~LumenCompositor() = default;

bool LumenCompositor::start(const QString &socketName) {
    m_compositor->setSocketName(socketName.toUtf8());
    // create() is what actually begins listening; it must run after the output
    // and shell exist, or the first client to connect sees no output.
    m_compositor->create();
    return m_compositor->isCreated();
}

QWaylandSeat *LumenCompositor::seat() const {
    return m_compositor ? m_compositor->defaultSeat() : nullptr;
}

QWaylandView *LumenCompositor::viewFor(const QString &session) const {
    return m_viewBySession.value(session);
}

void LumenCompositor::expectProcess(const QString &session, qint64 pid) {
    m_sessionByPid.insert(pid, session);
}

QString LumenCompositor::sessionForPid(const QString &pid) const {
    bool ok = false;
    const qint64 numeric = pid.toLongLong(&ok);
    if (!ok) {
        return QString();
    }
    return m_sessionByPid.value(numeric);
}

void LumenCompositor::adoptToplevel(QWaylandXdgToplevel *toplevel) {
    QWaylandSurface *surface = toplevel->xdgSurface()->surface();
    const qint64 pid = surface->client() ? surface->client()->processId() : 0;
    // A surface belongs to a session, and sessions are addressed by name. The
    // client's process id is the only handle the compositor has on it, so it is
    // mapped back to the session that started that process.
    const QString session = m_sessionByPid.value(pid);

    auto *view = new SessionView(this);
    view->setSurface(surface);
    view->setOutput(m_output);
    view->setPrimary();
    m_viewBySession.insert(session, view);
    m_sessionByView.insert(view, session);
    // The toplevel waits for the compositor's first configure before it draws.
    // Acknowledge it at once so the client starts committing frames. The
    // two-argument overload takes QList<State>, which an empty brace list
    // cannot disambiguate, so it is spelled out explicitly.
    toplevel->sendConfigure(QSize(kOutputWidth, kOutputHeight),
                            QList<QWaylandXdgToplevel::State>());

    connect(toplevel, &QWaylandXdgToplevel::titleChanged, this, [this, session, toplevel]() {
        emit titleChanged(session, toplevel->title());
    });
    connect(view, &QWaylandView::surfaceDestroyed, this, [this, view]() {
        const QString gone = m_sessionByView.take(view);
        m_viewBySession.remove(gone);
        view->deleteLater();
        emit surfaceGone(gone);
    });
    // Tell the client it may draw. Without frame callbacks a client draws once
    // and then waits forever, so the compositor sends them on every commit.
    connect(surface, &QWaylandSurface::damaged, this, [this]() {
        if (m_output) {
            m_output->frameStarted();
            m_output->sendFrameCallbacks();
        }
    });
    // A client draws only when told to; without this it never commits its first
    // buffer and never becomes interactive.
    connect(surface, &QWaylandSurface::redraw, this, [this, session]() {
        emit frameReady(session);
    });

    emit surfaceReady(session);
    emit titleChanged(session, toplevel->title());
}

QImage LumenCompositor::frame(const QString &session) {
    auto *view = qobject_cast<SessionView *>(m_viewBySession.value(session));
    return view ? view->frame() : QImage();
}

bool LumenCompositor::click(const QString &session, const QPointF &point) {
    auto *view = m_viewBySession.value(session);
    QWaylandSeat *seat = this->seat();
    if (!view || !view->surface() || !seat) {
        qWarning("lumen-click: session='%s' view=%p surface=%p seat=%p", qPrintable(session),
                 (void *)view, view ? (void *)view->surface() : nullptr, (void *)seat);
        return false;
    }
    qWarning("lumen-click: session='%s' view=%p surface=%p size=%dx%d seat=%p at %.0f,%.0f",
             qPrintable(session), (void *)view, (void *)view->surface(),
             view->surface()->destinationSize().width(),
             view->surface()->destinationSize().height(), (void *)seat, point.x(), point.y());
    // Exactly the sequence Qt's own compositor uses: give the client keyboard
    // focus, tell it where the pointer is, then press and release. A press is
    // only delivered to the seat's current mouse focus, so the move must come
    // first — and it is expressed in the surface's own coordinates.
    seat->setKeyboardFocus(view->surface());
    seat->sendMouseMoveEvent(view, point);
    seat->sendMousePressEvent(Qt::LeftButton);
    seat->sendMouseReleaseEvent(Qt::LeftButton);
    return true;
}

bool LumenCompositor::type(const QString &session, const QString &text) {
    auto *view = m_viewBySession.value(session);
    QWaylandSeat *seat = this->seat();
    if (!view || !view->surface() || !seat) {
        return false;
    }
    seat->setKeyboardFocus(view->surface());
    for (const QChar &ch : text) {
        if (ch == QLatin1Char('\n')) {
            seat->sendKeyPressEvent(Qt::Key_Return);
            seat->sendKeyReleaseEvent(Qt::Key_Return);
        } else {
            const uint code = ch.unicode();
            seat->sendUnicodeKeyPressEvent(code);
            seat->sendUnicodeKeyReleaseEvent(code);
        }
    }
    return true;
}

#include "compositor.moc"
