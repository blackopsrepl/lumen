// What Qt's xdg-shell is missing for Lumen's clients.
//
// QWaylandXdgShell implements xdg_wm_base version 1 and nothing around it. Two
// gaps decide whether a real application's popups appear at all:
//
//  - Repositioning. Version 1 has no xdg_popup.reposition, so a client that
//    moves an open popup must destroy and re-create it. Quickshell repositions
//    a panel's popups whenever the panel lays out, and on version 1 ends up
//    with the popup closed. Lumen advertises version 3 and answers reposition
//    itself: it tracks every positioner, works out the new geometry and sends
//    repositioned + configure, which is what the client then waits for.
//
//  - Layer-parented popups. A popup opened from a layer surface (a panel's
//    menu, tooltip or drop-down) is created with no xdg parent and attached to
//    its layer surface by the next request. Qt rejects the parentless popup
//    with a protocol error that kills the client. The popup is given itself as
//    a parent instead, and LayerShell attaches the real one.
//
// Both work through libwayland's protocol logger, which sees every request
// immediately before it is dispatched — the only place a request Qt handles
// can be observed or adjusted without Qt's private headers.

#pragma once

#include <QHash>
#include <QList>
#include <QObject>
#include <QPair>
#include <QPoint>
#include <QRect>
#include <QSize>

#include <cstdint>

#include <wayland-server-core.h>

class QWaylandCompositor;
class QWaylandXdgSurface;

class XdgShellExtras : public QObject {
    Q_OBJECT
  public:
    /// Install on `compositor`. Must run after the compositor is created, so
    /// Qt's xdg-shell global exists, and before any client connects.
    explicit XdgShellExtras(QWaylandCompositor* compositor, QObject* parent = nullptr);
    ~XdgShellExtras() override;

    /// The popup created without a parent whose xdg_popup is `popup`, claimed
    /// by its layer surface. Null when there is none.
    QWaylandXdgSurface* takeOrphan(struct ::wl_resource* popup);

    /// The geometry a popup was last placed at, relative to its parent's
    /// window geometry, when Lumen placed it. Qt's own idea of it is not
    /// updated by a reposition.
    bool placement(QWaylandXdgSurface* popup, QRect* geometry) const;

    /// Positioner state, as the client set it.
    struct Positioner {
        QSize size;
        QRect anchorRect;
        uint32_t anchor = 0;
        uint32_t gravity = 0;
        QPoint offset;
    };
    /// The unconstrained geometry `positioner` asks for, relative to the
    /// parent's window geometry.
    static QRect place(const Positioner& positioner);

  private:
    static void logRequest(void* data, enum wl_protocol_logger_type direction,
                           const struct wl_protocol_logger_message* message);
    void onRequest(const struct wl_protocol_logger_message* message);
    void reposition(struct ::wl_resource* popup, struct ::wl_resource* positioner, uint32_t token);

    using Key = QPair<struct ::wl_client*, uint32_t>;
    static Key keyOf(struct ::wl_resource* resource);
    /// Forget `key` everywhere when `resource` is destroyed, so a later
    /// object reusing the same id is never matched to a freed resource.
    void watch(struct ::wl_resource* resource, Key key);
    void forget(Key key);
    struct Watch;
    static void watchFired(struct wl_listener* listener, void* data);

    struct wl_display* m_display = nullptr;
    struct wl_protocol_logger* m_logger = nullptr;
    struct wl_global* m_raised = nullptr;
    bool m_creating = false;
    /// Positioners by (client, id).
    QHash<Key, Positioner> m_positioners;
    /// Each popup's xdg_surface resource, by the popup's (client, id).
    QHash<Key, struct ::wl_resource*> m_popupSurface;
    /// Parentless popups waiting for their layer surface, by (client, id).
    QHash<Key, struct ::wl_resource*> m_orphans;
    /// Geometry Lumen gave each repositioned popup, by its xdg_surface.
    QHash<QWaylandXdgSurface*, QRect> m_placed;
    QList<Watch*> m_watches;
};
