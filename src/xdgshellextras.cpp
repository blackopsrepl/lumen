#include "xdgshellextras.h"

#include <QtWaylandCompositor/QWaylandCompositor>
#include <QtWaylandCompositor/QWaylandXdgPopup>
#include <QtWaylandCompositor/QWaylandXdgSurface>

#include <cstring>

namespace QtWaylandServer {
// Qt's generated xdg_wm_base server class, as far as its exported bind
// function: the class itself lives in a private header this build does not
// use. The signature matches the exported symbol exactly.
class xdg_wm_base {
  public:
    static void bind_func(struct ::wl_client* client, void* data, uint32_t version, uint32_t id);
};
} // namespace QtWaylandServer

namespace {

/// The version advertised in place of Qt's 1: the first with reposition.
constexpr uint32_t kXdgShellVersion = 3;

/// xdg_popup.repositioned, the event a client waits for after reposition.
constexpr uint32_t kPopupRepositioned = 2;

void bindRaised(struct wl_client* client, void* data, uint32_t version, uint32_t id) {
    // Qt's own global carries the shell object binding needs; the raised
    // global hands every client straight to it, so the objects created are
    // Qt's exactly as before.
    auto* qtGlobal = static_cast<struct wl_global*>(data);
    QtWaylandServer::xdg_wm_base::bind_func(client, wl_global_get_user_data(qtGlobal), version, id);
}

bool isRequest(const struct wl_protocol_logger_message* message, const char* interface,
               const char* request) {
    return std::strcmp(wl_resource_get_class(message->resource), interface) == 0 &&
           std::strcmp(message->message->name, request) == 0;
}

} // namespace

struct XdgShellExtras::Watch {
    struct wl_listener listener;
    XdgShellExtras* extras;
    Key key;
};

XdgShellExtras::XdgShellExtras(QWaylandCompositor* compositor, QObject* parent)
    : QObject(parent), m_display(compositor->display()) {
    m_logger = wl_display_add_protocol_logger(m_display, &XdgShellExtras::logRequest, this);
    // Qt's global is hidden and a raised one offered in its place. The filter
    // meets Qt's global the first time a client asks for the registry, and
    // creates the raised one then: it needs Qt's global to bind through.
    wl_display_set_global_filter(
        m_display,
        [](const struct wl_client*, const struct wl_global* global, void* data) -> bool {
            auto* self = static_cast<XdgShellExtras*>(data);
            if (global == self->m_raised || self->m_creating) {
                return true;
            }
            if (std::strcmp(wl_global_get_interface(global)->name, "xdg_wm_base") != 0) {
                return true;
            }
            if (!self->m_raised) {
                // Creating a global consults this filter for it before its
                // pointer is known here; `m_creating` lets it through.
                self->m_creating = true;
                self->m_raised = wl_global_create(
                    self->m_display, wl_global_get_interface(global), kXdgShellVersion,
                    const_cast<struct wl_global*>(global), bindRaised);
                self->m_creating = false;
            }
            return false;
        },
        this);
}

XdgShellExtras::~XdgShellExtras() {
    wl_display_set_global_filter(m_display, nullptr, nullptr);
    if (m_logger) {
        wl_protocol_logger_destroy(m_logger);
    }
    for (Watch* watch : std::as_const(m_watches)) {
        wl_list_remove(&watch->listener.link);
        delete watch;
    }
}

XdgShellExtras::Key XdgShellExtras::keyOf(struct ::wl_resource* resource) {
    return qMakePair(wl_resource_get_client(resource), wl_resource_get_id(resource));
}

void XdgShellExtras::watch(struct ::wl_resource* resource, Key key) {
    auto* watch = new Watch;
    watch->extras = this;
    watch->key = key;
    watch->listener.notify = &XdgShellExtras::watchFired;
    wl_resource_add_destroy_listener(resource, &watch->listener);
    m_watches.append(watch);
}

void XdgShellExtras::watchFired(struct wl_listener* listener, void*) {
    Watch* watch = wl_container_of(listener, watch, listener);
    watch->extras->forget(watch->key);
    watch->extras->m_watches.removeAll(watch);
    wl_list_remove(&watch->listener.link);
    delete watch;
}

void XdgShellExtras::forget(Key key) {
    m_positioners.remove(key);
    m_popupSurface.remove(key);
    m_orphans.remove(key);
}

void XdgShellExtras::logRequest(void* data, enum wl_protocol_logger_type direction,
                                const struct wl_protocol_logger_message* message) {
    if (direction == WL_PROTOCOL_LOGGER_REQUEST) {
        static_cast<XdgShellExtras*>(data)->onRequest(message);
    }
}

void XdgShellExtras::onRequest(const struct wl_protocol_logger_message* message) {
    const char* interface = wl_resource_get_class(message->resource);
    const union wl_argument* args = message->arguments;

    if (std::strcmp(interface, "xdg_positioner") == 0) {
        const Key key = keyOf(message->resource);
        if (!m_positioners.contains(key)) {
            watch(message->resource, key);
        }
        Positioner& positioner = m_positioners[key];
        const char* request = message->message->name;
        if (std::strcmp(request, "set_size") == 0) {
            positioner.size = QSize(args[0].i, args[1].i);
        } else if (std::strcmp(request, "set_anchor_rect") == 0) {
            positioner.anchorRect = QRect(args[0].i, args[1].i, args[2].i, args[3].i);
        } else if (std::strcmp(request, "set_anchor") == 0) {
            positioner.anchor = args[0].u;
        } else if (std::strcmp(request, "set_gravity") == 0) {
            positioner.gravity = args[0].u;
        } else if (std::strcmp(request, "set_offset") == 0) {
            positioner.offset = QPoint(args[0].i, args[1].i);
        }
        return;
    }

    if (isRequest(message, "xdg_surface", "get_popup")) {
        const Key popup = qMakePair(wl_resource_get_client(message->resource), args[0].n);
        m_popupSurface.insert(popup, message->resource);
        watch(message->resource, popup);
        if (args[1].o == nullptr) {
            // A popup opened from a layer surface, named to it by the next
            // request. Qt would kill the client over the null parent, so the
            // popup is made its own parent. The logger is handed the very
            // arguments the request is about to be dispatched with —
            // libwayland logs a request immediately before invoking it — so
            // this rewrite is what Qt sees.
            auto* writable = const_cast<union wl_argument*>(args);
            writable[1].o = reinterpret_cast<struct wl_object*>(message->resource);
            m_orphans.insert(popup, message->resource);
        }
        return;
    }

    if (isRequest(message, "xdg_popup", "reposition") && args[0].o) {
        reposition(message->resource, reinterpret_cast<struct ::wl_resource*>(args[0].o),
                   args[1].u);
    }
}

void XdgShellExtras::reposition(struct ::wl_resource* popupResource,
                                struct ::wl_resource* positionerResource, uint32_t token) {
    struct ::wl_resource* surfaceResource = m_popupSurface.value(keyOf(popupResource));
    QWaylandXdgSurface* surface =
        surfaceResource ? QWaylandXdgSurface::fromResource(surfaceResource) : nullptr;
    QWaylandXdgPopup* popup = surface ? surface->popup() : nullptr;
    const auto positioner = m_positioners.constFind(keyOf(positionerResource));
    if (!popup || positioner == m_positioners.constEnd()) {
        return;
    }
    const QRect geometry = place(*positioner);
    if (!m_placed.contains(surface)) {
        connect(surface, &QObject::destroyed, this,
                [this, surface]() { m_placed.remove(surface); });
    }
    m_placed.insert(surface, geometry);
    // The protocol's order: repositioned, then the configure it belongs to.
    wl_resource_post_event(popupResource, kPopupRepositioned, token);
    popup->sendConfigure(geometry);
}

QWaylandXdgSurface* XdgShellExtras::takeOrphan(struct ::wl_resource* popup) {
    struct ::wl_resource* surface = m_orphans.take(keyOf(popup));
    return surface ? QWaylandXdgSurface::fromResource(surface) : nullptr;
}

bool XdgShellExtras::placement(QWaylandXdgSurface* popup, QRect* geometry) const {
    const auto it = m_placed.constFind(popup);
    if (it == m_placed.constEnd()) {
        return false;
    }
    *geometry = *it;
    return true;
}

QRect XdgShellExtras::place(const Positioner& positioner) {
    // xdg_positioner's anchor and gravity enums share one layout:
    // none, top, bottom, left, right, top_left, bottom_left, top_right,
    // bottom_right.
    const auto isLeft = [](uint32_t v) { return v == 3 || v == 5 || v == 6; };
    const auto isRight = [](uint32_t v) { return v == 4 || v == 7 || v == 8; };
    const auto isTop = [](uint32_t v) { return v == 1 || v == 5 || v == 7; };
    const auto isBottom = [](uint32_t v) { return v == 2 || v == 6 || v == 8; };

    const QRect& rect = positioner.anchorRect;
    const int ax = isLeft(positioner.anchor)    ? rect.x()
                   : isRight(positioner.anchor) ? rect.x() + rect.width()
                                                : rect.x() + rect.width() / 2;
    const int ay = isTop(positioner.anchor)      ? rect.y()
                   : isBottom(positioner.anchor) ? rect.y() + rect.height()
                                                 : rect.y() + rect.height() / 2;
    const QSize size = positioner.size;
    // Gravity is the direction the popup extends from the anchor point.
    const int x = isLeft(positioner.gravity)    ? ax - size.width()
                  : isRight(positioner.gravity) ? ax
                                                : ax - size.width() / 2;
    const int y = isTop(positioner.gravity)      ? ay - size.height()
                  : isBottom(positioner.gravity) ? ay
                                                 : ay - size.height() / 2;
    return QRect(QPoint(x, y) + positioner.offset, size);
}
