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

#include "layershell.h"
#include "xdgshellextras.h"

#include <QtWaylandCompositor/QWaylandClient>
#include <QtWaylandCompositor/QWaylandCompositor>
#include <QtWaylandCompositor/QWaylandOutput>
#include <QtWaylandCompositor/QWaylandSeat>
#include <QtWaylandCompositor/QWaylandSurface>
#include <QtWaylandCompositor/QWaylandView>
#include <QtWaylandCompositor/QWaylandXdgShell>
#include <QtWaylandCompositor/QWaylandXdgSurface>
#include <QtWaylandCompositor/QWaylandXdgToplevel>

#include <QFile>
#include <QImage>
#include <QKeyEvent>
#include <QPainter>
#include <QPointF>
#include <QRect>
#include <QRegion>

#include <algorithm>

namespace {

/// The compositor's headless output size.
///
/// An output is required even with no window: a client is told the size it may
/// occupy, and pointer focus is expressed in output coordinates, so an output
/// with no geometry leaves input with nowhere to land.
constexpr int kOutputWidth = 1920;
constexpr int kOutputHeight = 1080;
constexpr int kOutputRefresh = 60000;

/// Paint order bands. Background and bottom layers sit under windows, top and
/// overlay layers over them, and popups over everything they could open from.
constexpr int kBandWindow = 2;
constexpr int kBandPopup = 5;
int layerBand(uint32_t layer) {
    return layer <= 1 ? int(layer) : int(layer) + 1;
}

/// A session's surface, as Lumen renders it.
///
/// This is a QWaylandView, not a scene-graph item: `advance()` moves to the
/// client's newest buffer and `currentBuffer()` yields it as a QImage. That is
/// what lets the daemon composite with no window and no Quick scene.
class SessionView : public QWaylandView {
    Q_OBJECT
  public:
    explicit SessionView(QObject* parent = nullptr) : QWaylandView(parent) {}

    /// The client's latest frame, or a null image when it has not drawn yet.
    ///
    /// Stores the buffer from the surface's bufferCommitted signal and returns
    /// its image. For SHM buffers (which software-rendered clients use) this
    /// needs no GL context and no render thread.
    QImage frame() const {
        if (m_buffer.isNull() || !m_buffer.hasBuffer()) {
            return QImage();
        }
        return m_buffer.image();
    }

    /// Whether the surface currently shows anything. A window that is hidden
    /// commits a null buffer, which unmaps it without destroying it.
    bool mapped() const { return !m_buffer.isNull() && m_buffer.hasBuffer(); }

    QSize size() const { return surface() ? surface()->destinationSize() : QSize(); }

  protected:
    /// Qt calls this when the client commits a new buffer. Storing it here is
    /// what makes frame() work: the buffer is the client's latest pixels, and
    /// reading an SHM buffer needs no GL context and no render thread.
    void bufferCommitted(const QWaylandBufferRef& buffer, const QRegion&) override {
        m_buffer = buffer;
    }

  private:
    QWaylandBufferRef m_buffer;
};

SessionView* asSessionView(QWaylandView* view) {
    return static_cast<SessionView*>(view);
}

/// The session a process was started for, read from its environment.
///
/// The environment survives fork, exec and daemonizing, which the process id
/// does not: a launcher script, an application that re-executes itself, or one
/// that detaches from its parent all connect from a pid nobody registered.
QString sessionFromEnvironment(qint64 pid) {
    QFile environ(QStringLiteral("/proc/%1/environ").arg(pid));
    if (!environ.open(QIODevice::ReadOnly)) {
        return QString();
    }
    const QByteArray prefix = QByteArray(LumenCompositor::kSessionEnvVar) + '=';
    for (const QByteArray& entry : environ.readAll().split('\0')) {
        if (entry.startsWith(prefix)) {
            return QString::fromUtf8(entry.mid(prefix.size()));
        }
    }
    return QString();
}

} // namespace

LumenCompositor::LumenCompositor(QObject* parent) : QObject(parent) {
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
    connect(
        m_shell, &QWaylandXdgShell::toplevelCreated, this,
        [this](QWaylandXdgToplevel* toplevel, QWaylandXdgSurface*) { adoptToplevel(toplevel); });
    connect(m_shell, &QWaylandXdgShell::popupCreated, this,
            [this](QWaylandXdgPopup* popup, QWaylandXdgSurface*) { adoptPopup(popup); });

    // Panels, bars and overlays — Quickshell's PanelWindow among them — are
    // layer surfaces, a protocol Qt does not ship. Without it such a client
    // finds nothing to map its window with and the session draws nothing.
    m_layerShell = new LayerShell(m_compositor, QSize(kOutputWidth, kOutputHeight));
    connect(m_layerShell, &LayerShell::layerSurfaceCreated, this,
            [this](LayerSurface* layer) { adoptLayer(layer); });
    connect(m_layerShell, &LayerShell::popupParented, this,
            [this](QWaylandXdgSurface* popup, LayerSurface* parent) {
                m_layerParent.insert(popup->surface(), parent);
            });
}

LumenCompositor::~LumenCompositor() {
    for (const QList<Entry*>& entries : std::as_const(m_entries)) {
        qDeleteAll(entries);
    }
}

bool LumenCompositor::start(const QString& socketName) {
    m_compositor->setSocketName(socketName.toUtf8());
    // create() is what actually begins listening; it must run after the output
    // and shell exist, or the first client to connect sees no output.
    m_compositor->create();
    if (!m_compositor->isCreated()) {
        return false;
    }
    // Popup repositioning and layer-parented popups, which Qt's xdg-shell
    // lacks; installed before any client can connect.
    m_xdgExtras = new XdgShellExtras(m_compositor, this);
    m_layerShell->setXdgShellExtras(m_xdgExtras);
    return true;
}

QWaylandSeat* LumenCompositor::seat() const {
    return m_compositor ? m_compositor->defaultSeat() : nullptr;
}

QWaylandView* LumenCompositor::viewFor(const QString& session) const {
    // The topmost surface on screen; before anything has drawn, the newest.
    const QList<Entry*> entries = stacked(session);
    if (!entries.isEmpty()) {
        return entries.last()->view;
    }
    const QList<Entry*> all = m_entries.value(session);
    return all.isEmpty() ? nullptr : all.last()->view;
}

int LumenCompositor::surfaceCount(const QString& session) const {
    return m_entries.value(session).size();
}

void LumenCompositor::expectProcess(const QString& session, qint64 pid) {
    m_sessionByPid.insert(pid, session);
}

QString LumenCompositor::sessionForPid(const QString& pid) const {
    bool ok = false;
    const qint64 numeric = pid.toLongLong(&ok);
    if (!ok) {
        return QString();
    }
    return m_sessionByPid.value(numeric);
}

QString LumenCompositor::sessionForClient(QWaylandClient* client) {
    const qint64 pid = client ? client->processId() : 0;
    if (pid <= 0) {
        return QString();
    }
    QString session = m_sessionByPid.value(pid);
    if (session.isEmpty()) {
        session = sessionFromEnvironment(pid);
        if (!session.isEmpty()) {
            m_sessionByPid.insert(pid, session);
        }
    }
    if (session.isEmpty()) {
        qWarning("lumen: a client (pid %lld) connected that belongs to no session", pid);
    }
    return session;
}

QString LumenCompositor::sessionForSurface(QWaylandSurface* surface) const {
    for (auto it = m_entries.constBegin(); it != m_entries.constEnd(); ++it) {
        for (const Entry* entry : it.value()) {
            if (entry->surface == surface) {
                return it.key();
            }
        }
    }
    return QString();
}

LumenCompositor::Entry* LumenCompositor::entryFor(QWaylandSurface* surface) const {
    for (const QList<Entry*>& entries : m_entries) {
        for (Entry* entry : entries) {
            if (entry->surface == surface) {
                return entry;
            }
        }
    }
    return nullptr;
}

LumenCompositor::Entry* LumenCompositor::track(QWaylandSurface* surface, const QString& session,
                                               int band) {
    auto* view = new SessionView(this);
    view->setSurface(surface);
    view->setOutput(m_output);
    view->setPrimary();

    auto* entry = new Entry;
    entry->view = view;
    entry->surface = surface;
    entry->band = band;
    entry->serial = ++m_serial;
    m_entries[session].append(entry);

    // The entry ends with its surface — or with its role, when the client
    // keeps the surface and gives it a new role (Qt re-uses a menu's surface
    // for the next menu); the role objects are gone then, and an entry left
    // pointing at them would be read after they are freed.
    const quint64 serial = entry->serial;
    connect(view, &QWaylandView::surfaceDestroyed, this,
            [this, session, serial]() { untrack(session, serial); });
    // Tell the client it may draw. Without frame callbacks a client draws once
    // and then waits forever, so the compositor sends them on every commit.
    connect(surface, &QWaylandSurface::damaged, this, [this]() {
        if (m_output) {
            m_output->frameStarted();
            m_output->sendFrameCallbacks();
        }
    });
    // A window shown or hidden changes what the session's title should say.
    connect(surface, &QWaylandSurface::hasContentChanged, this,
            [this, session]() { announceTitle(session); });
    connect(surface, &QWaylandSurface::redraw, this,
            [this, session]() { emit frameReady(session); });
    return entry;
}

void LumenCompositor::untrack(const QString& session, quint64 serial) {
    QList<Entry*>& entries = m_entries[session];
    const auto it = std::find_if(entries.begin(), entries.end(),
                                 [serial](const Entry* e) { return e->serial == serial; });
    if (it == entries.end()) {
        if (entries.isEmpty()) {
            m_entries.remove(session);
        }
        return;
    }
    Entry* entry = *it;
    entries.erase(it);
    if (m_focus.value(session) == entry->surface) {
        m_focus.remove(session);
    }
    m_layerParent.remove(entry->surface);
    entry->view->deleteLater();
    delete entry;
    if (entries.isEmpty()) {
        // Only the last surface leaving ends the session's picture; a dialog
        // closing must not take its main window with it.
        m_entries.remove(session);
        m_focus.remove(session);
        emit surfaceGone(session);
    } else {
        announceTitle(session);
    }
}

void LumenCompositor::adoptToplevel(QWaylandXdgToplevel* toplevel) {
    QWaylandSurface* surface = toplevel->xdgSurface()->surface();
    // A surface belongs to a session, and sessions are addressed by name. The
    // client's process is the only handle the compositor has on it, so it is
    // mapped back to the session that started that process.
    const QString session = sessionForClient(surface->client());
    Entry* entry = track(surface, session, kBandWindow);
    entry->toplevel = toplevel;
    entry->xdg = toplevel->xdgSurface();
    const quint64 serial = entry->serial;
    connect(toplevel->xdgSurface(), &QObject::destroyed, this,
            [this, session, serial]() { untrack(session, serial); });

    // The toplevel waits for the compositor's first configure before it draws.
    // Acknowledge it at once so the client starts committing frames. The
    // two-argument overload takes QList<State>, which an empty brace list
    // cannot disambiguate, so it is spelled out explicitly.
    //
    // ActivatedState matters for input: a client that is never told it is
    // active does not consider itself focused, so its own items refuse
    // keyboard focus and delivered keys are dropped without a trace.
    toplevel->sendConfigure(QSize(kOutputWidth, kOutputHeight),
                            QList<QWaylandXdgToplevel::State>()
                                << QWaylandXdgToplevel::ActivatedState);

    connect(toplevel, &QWaylandXdgToplevel::titleChanged, this,
            [this, session]() { announceTitle(session); });
    // The newest window takes the keyboard, as a dialog would on a desktop.
    m_focus.insert(session, surface);

    emit surfaceReady(session);
    announceTitle(session);
}

void LumenCompositor::adoptPopup(QWaylandXdgPopup* popup) {
    QWaylandSurface* surface = popup->xdgSurface()->surface();
    QWaylandXdgSurface* parent = popup->parentXdgSurface();
    // A popup belongs to whatever opened it; its own client is the fallback.
    QString session = parent ? sessionForSurface(parent->surface()) : QString();
    if (session.isEmpty()) {
        session = sessionForClient(surface->client());
    }
    Entry* entry = track(surface, session, kBandPopup);
    entry->popup = popup;
    entry->xdg = popup->xdgSurface();
    entry->parentXdg = parent;
    const quint64 serial = entry->serial;
    connect(popup->xdgSurface(), &QObject::destroyed, this,
            [this, session, serial]() { untrack(session, serial); });
    m_focus.insert(session, surface);
    emit surfaceReady(session);
}

void LumenCompositor::adoptLayer(LayerSurface* layer) {
    QWaylandSurface* surface = layer->surface();
    const QString session = sessionForClient(surface->client());
    Entry* entry = track(surface, session, layerBand(layer->layer()));
    entry->layer = layer;
    const quint64 serial = entry->serial;
    connect(layer, &QObject::destroyed, this,
            [this, session, serial]() { untrack(session, serial); });
    if (layer->wantsKeyboard() || !m_focus.contains(session)) {
        m_focus.insert(session, surface);
    }
    emit surfaceReady(session);
    announceTitle(session);
}

QPoint LumenCompositor::positionOf(const Entry* entry) const {
    if (entry->layer) {
        return entry->layer->position();
    }
    if (entry->popup && entry->xdg) {
        // A popup's geometry is relative to its parent's window geometry, and
        // its own buffer may carry a margin outside its window geometry.
        // A popup Lumen repositioned is where Lumen put it; Qt's configured
        // geometry still holds the first placement.
        QRect placed = entry->popup->configuredGeometry();
        if (m_xdgExtras) {
            m_xdgExtras->placement(entry->xdg, &placed);
        }
        const QPoint offset = placed.topLeft() - entry->xdg->windowGeometry().topLeft();
        // Opened from a panel: the parent is a layer surface, and the popup's
        // xdg parent is itself (see XdgShellExtras).
        if (LayerSurface* layer = m_layerParent.value(entry->surface)) {
            return layer->position() + offset;
        }
        QWaylandXdgSurface* parent = entry->parentXdg;
        if (!parent || parent == entry->xdg) {
            return offset;
        }
        const Entry* parentEntry = entryFor(parent->surface());
        const QPoint parentOrigin = parentEntry ? positionOf(parentEntry) : QPoint();
        return parentOrigin + parent->windowGeometry().topLeft() + offset;
    }
    // Windows sit at the output's origin, so an ordinary application's frame
    // and click coordinates are its own window's.
    return QPoint();
}

QList<LumenCompositor::Entry*> LumenCompositor::stacked(const QString& session) const {
    QList<Entry*> entries;
    for (Entry* entry : m_entries.value(session)) {
        if (asSessionView(entry->view)->mapped()) {
            entries.append(entry);
        }
    }
    std::stable_sort(entries.begin(), entries.end(), [](const Entry* a, const Entry* b) {
        return a->band != b->band ? a->band < b->band : a->serial < b->serial;
    });
    return entries;
}

LumenCompositor::Entry* LumenCompositor::hit(const QString& session, const QPointF& point,
                                             QPointF* local) const {
    const QList<Entry*> entries = stacked(session);
    for (auto it = entries.crbegin(); it != entries.crend(); ++it) {
        const QPoint origin = positionOf(*it);
        const QRect rect(origin, asSessionView((*it)->view)->size());
        if (rect.contains(point.toPoint())) {
            if (local) {
                *local = point - QPointF(origin);
            }
            return *it;
        }
    }
    return nullptr;
}

void LumenCompositor::announceTitle(const QString& session) {
    // The topmost window names the session: a dialog in front is what the
    // application is showing. A session of panels only is named by namespace.
    QString title;
    QString layerName;
    for (const Entry* entry : stacked(session)) {
        if (entry->toplevel) {
            title = entry->toplevel->title();
        } else if (entry->layer && layerName.isEmpty()) {
            layerName = entry->layer->nameSpace();
        }
    }
    if (title.isEmpty()) {
        // Before the first buffer nothing is stacked yet; the newest toplevel
        // still has a title worth reporting.
        for (const Entry* entry : m_entries.value(session)) {
            if (entry->toplevel) {
                title = entry->toplevel->title();
            }
        }
    }
    emit titleChanged(session, title.isEmpty() ? layerName : title);
}

QImage LumenCompositor::frame(const QString& session) {
    const QList<Entry*> entries = stacked(session);
    if (entries.isEmpty()) {
        return QImage();
    }
    // One window at the origin is the common case, and its buffer is already
    // the frame: no copy, no painting.
    if (entries.size() == 1 && positionOf(entries.first()).isNull()) {
        return asSessionView(entries.first()->view)->frame();
    }
    QRect extent;
    for (const Entry* entry : entries) {
        extent |= QRect(positionOf(entry), asSessionView(entry->view)->size());
    }
    // The frame always starts at the output's origin, so a coordinate means the
    // same thing however many surfaces the session has at the moment.
    extent = QRect(QPoint(0, 0), extent.bottomRight())
                 .intersected(QRect(0, 0, kOutputWidth, kOutputHeight));
    if (extent.isEmpty()) {
        return QImage();
    }
    QImage canvas(extent.size(), QImage::Format_RGB32);
    canvas.fill(Qt::black);
    QPainter painter(&canvas);
    for (const Entry* entry : entries) {
        const auto* view = asSessionView(entry->view);
        painter.drawImage(QRect(positionOf(entry), view->size()), view->frame());
    }
    painter.end();
    return canvas;
}

bool LumenCompositor::click(const QString& session, const QPointF& point) {
    QWaylandSeat* seat = this->seat();
    if (!seat) {
        return false;
    }
    QPointF local;
    Entry* target = hit(session, point, &local);
    if (!target && stacked(session).isEmpty() && !m_entries.value(session).isEmpty()) {
        // Nothing has drawn yet, so there is nothing to hit-test against. The
        // newest surface is the one about to appear; deliver to it as is.
        target = m_entries.value(session).last();
        local = point;
    }
    // A click outside an open popup dismisses it, as on any desktop: the
    // client holds a grab and is waiting to be told the popup is done.
    if (!target || !target->popup) {
        for (Entry* entry : m_entries.value(session)) {
            if (entry->popup && entry->xdg) {
                entry->popup->sendPopupDone();
            }
        }
    }
    if (!target) {
        return false;
    }
    // Exactly the sequence Qt's own compositor uses: give the client keyboard
    // focus, tell it where the pointer is, then press and release. A press is
    // only delivered to the seat's current mouse focus, so the move must come
    // first — and it is expressed in the surface's own coordinates.
    QWaylandSurface* surface = target->surface;
    m_focus.insert(session, surface);
    seat->setKeyboardFocus(surface);
    seat->sendMouseMoveEvent(target->view, local);
    seat->sendMousePressEvent(Qt::LeftButton);
    seat->sendMouseReleaseEvent(Qt::LeftButton);
    return true;
}

bool LumenCompositor::type(const QString& session, const QString& text) {
    QWaylandSeat* seat = this->seat();
    QWaylandSurface* surface = m_focus.value(session);
    if (!surface || !entryFor(surface)) {
        QWaylandView* view = viewFor(session);
        surface = view ? view->surface() : nullptr;
    }
    if (!surface || !seat) {
        return false;
    }
    seat->setKeyboardFocus(surface);
    // A full key event carries the text itself, so the client does not have to
    // map a scancode through its keymap to get the character. Plain unicode key
    // events depend on that translation, and a client whose keymap has not been
    // delivered drops them silently.
    for (const QChar& ch : text) {
        const int key = ch == QLatin1Char('\n')   ? Qt::Key_Return
                        : ch == QLatin1Char('\b') ? Qt::Key_Backspace
                                                  : int(ch.toUpper().unicode());
        QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier,
                        ch == QLatin1Char('\n') || ch == QLatin1Char('\b') ? QString()
                                                                           : QString(ch));
        seat->sendFullKeyEvent(&press);
        QKeyEvent release(QEvent::KeyRelease, key, Qt::NoModifier, QString());
        seat->sendFullKeyEvent(&release);
    }
    return true;
}

#include "compositor.moc"
