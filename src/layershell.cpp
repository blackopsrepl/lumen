#include "layershell.h"

#include "xdgshellextras.h"

#include <QtWaylandCompositor/QWaylandCompositor>
#include <QtWaylandCompositor/QWaylandSurface>
#include <QtWaylandCompositor/QWaylandXdgSurface>

#include <cstring>

namespace {

/// The highest protocol version implemented here. Version 5 only adds
/// set_exclusive_edge, which decides nothing on an output with no other
/// clients to make room for.
constexpr int kLayerShellVersion = 4;

/// The surface role, so a surface cannot be both a layer surface and a toplevel.
QWaylandSurfaceRole s_layerRole("zwlr_layer_surface_v1");

constexpr uint32_t kAnchorTop = 1;
constexpr uint32_t kAnchorBottom = 2;
constexpr uint32_t kAnchorLeft = 4;
constexpr uint32_t kAnchorRight = 8;

/// One axis of the layout rule: anchored to both edges or to neither means
/// centred in the space between the margins, otherwise flush to the anchored
/// edge plus its margin.
int place(bool start, bool end, int extent, int size, int marginStart, int marginEnd) {
    if (start && !end) {
        return marginStart;
    }
    if (end && !start) {
        return extent - size - marginEnd;
    }
    return marginStart + (extent - marginStart - marginEnd - size) / 2;
}

} // namespace

LayerShell::LayerShell(QWaylandCompositor* compositor, const QSize& outputSize)
    : QWaylandCompositorExtensionTemplate<LayerShell>(compositor), m_compositor(compositor),
      m_outputSize(outputSize) {}

LayerShell::~LayerShell() {
    if (m_logger) {
        wl_protocol_logger_destroy(m_logger);
    }
}

void LayerShell::initialize() {
    QWaylandCompositorExtensionTemplate::initialize();
    init(m_compositor->display(), kLayerShellVersion);
    // The layer-shell handshake runs on wl_surface.commit, and Qt has no signal
    // for a commit that carries no buffer — which is exactly the one that must
    // be answered with the first configure. Without an answer the client waits
    // forever and the panel never draws. libwayland's protocol logger sees
    // every request, so the commit is caught there.
    m_logger =
        wl_display_add_protocol_logger(m_compositor->display(), &LayerShell::logRequest, this);
}

void LayerShell::logRequest(void* data, enum wl_protocol_logger_type direction,
                            const struct wl_protocol_logger_message* message) {
    if (direction != WL_PROTOCOL_LOGGER_REQUEST) {
        return;
    }
    if (std::strcmp(wl_resource_get_class(message->resource), "wl_surface") != 0 ||
        std::strcmp(message->message->name, "commit") != 0) {
        return;
    }
    auto* shell = static_cast<LayerShell*>(data);
    if (LayerSurface* layer = shell->m_bySurface.value(message->resource)) {
        // Answered within the commit's own dispatch, as other compositors do:
        // the configure depends only on layer-surface state already received,
        // and a client sees it before anything it sends after the commit.
        layer->onCommit();
    }
}

void LayerShell::zwlr_layer_shell_v1_get_layer_surface(Resource* resource, uint32_t id,
                                                       struct ::wl_resource* surfaceResource,
                                                       struct ::wl_resource*, uint32_t layer,
                                                       const QString& nameSpace) {
    QWaylandSurface* surface = QWaylandSurface::fromResource(surfaceResource);
    if (!surface) {
        return;
    }
    if (layer > layer_overlay) {
        wl_resource_post_error(resource->handle, error_invalid_layer, "invalid layer %u", layer);
        return;
    }
    if (!surface->setRole(&s_layerRole, resource->handle, error_role)) {
        return;
    }
    auto* layerSurface = new LayerSurface(this, surface, layer, nameSpace, resource->client(), id,
                                          wl_resource_get_version(resource->handle));
    emit layerSurfaceCreated(layerSurface);
}

void LayerShell::zwlr_layer_shell_v1_destroy(Resource* resource) {
    wl_resource_destroy(resource->handle);
}

LayerSurface::LayerSurface(LayerShell* shell, QWaylandSurface* surface, uint32_t layer,
                           const QString& nameSpace, struct ::wl_client* client, uint32_t id,
                           int version)
    : QObject(shell), QtWaylandServer::zwlr_layer_surface_v1(client, id, version), m_shell(shell),
      m_surface(surface), m_nameSpace(nameSpace), m_layer(layer) {
    m_surfaceResource = surface->resource();
    shell->m_bySurface.insert(m_surfaceResource, this);
    connect(surface, &QWaylandSurface::destinationSizeChanged, this,
            &LayerSurface::positionChanged);
    connect(surface, &QWaylandSurface::surfaceDestroyed, this, [this]() {
        m_shell->m_bySurface.remove(m_surfaceResource);
        m_surfaceResource = nullptr;
        m_surface = nullptr;
    });
}

LayerSurface::~LayerSurface() {
    m_shell->m_bySurface.remove(m_surfaceResource);
}

QSize LayerSurface::configuredSize() const {
    const QSize output = m_shell->outputSize();
    int width = m_requested.width();
    int height = m_requested.height();
    // Zero means "as large as you will make me", which is only meaningful
    // along an axis the surface is stretched across by its anchors.
    if (width == 0 && (m_anchor & kAnchorLeft) && (m_anchor & kAnchorRight)) {
        width = output.width() - m_margins.left() - m_margins.right();
    }
    if (height == 0 && (m_anchor & kAnchorTop) && (m_anchor & kAnchorBottom)) {
        height = output.height() - m_margins.top() - m_margins.bottom();
    }
    return QSize(qMax(0, width), qMax(0, height));
}

QPoint LayerSurface::position() const {
    const QSize output = m_shell->outputSize();
    QSize size = m_surface ? m_surface->destinationSize() : QSize();
    if (!size.isValid() || size.isEmpty()) {
        size = configuredSize();
    }
    return QPoint(place(m_anchor & kAnchorLeft, m_anchor & kAnchorRight, output.width(),
                        size.width(), m_margins.left(), m_margins.right()),
                  place(m_anchor & kAnchorTop, m_anchor & kAnchorBottom, output.height(),
                        size.height(), m_margins.top(), m_margins.bottom()));
}

void LayerSurface::onCommit() {
    if (!m_surface) {
        return;
    }
    if (m_configured && !m_dirty) {
        return;
    }
    const QSize size = configuredSize();
    m_dirty = false;
    // A client re-sends its whole state as it lays itself out; only a change
    // that moves the size is worth a configure. Each one makes the client lay
    // out again, and repositions every popup open on the panel.
    if (m_configured && size == m_sentSize) {
        return;
    }
    m_sentSize = size;
    m_serial = m_shell->compositor()->nextSerial();
    send_configure(m_serial, size.width(), size.height());
    m_configured = true;
    emit positionChanged();
}

void LayerSurface::zwlr_layer_surface_v1_set_size(Resource*, uint32_t width, uint32_t height) {
    m_requested = QSize(int(width), int(height));
    m_dirty = true;
}

void LayerSurface::zwlr_layer_surface_v1_set_anchor(Resource* resource, uint32_t anchor) {
    if (anchor > (kAnchorTop | kAnchorBottom | kAnchorLeft | kAnchorRight)) {
        wl_resource_post_error(resource->handle, error_invalid_anchor, "invalid anchor %u", anchor);
        return;
    }
    m_anchor = anchor;
    m_dirty = true;
}

void LayerSurface::zwlr_layer_surface_v1_set_exclusive_zone(Resource*, int32_t) {
    // Nothing else on Lumen's output needs to make room.
}

void LayerSurface::zwlr_layer_surface_v1_set_margin(Resource*, int32_t top, int32_t right,
                                                    int32_t bottom, int32_t left) {
    m_margins = QMargins(left, top, right, bottom);
    m_dirty = true;
}

void LayerSurface::zwlr_layer_surface_v1_set_keyboard_interactivity(Resource*,
                                                                    uint32_t interactivity) {
    m_keyboard = interactivity;
}

void LayerSurface::zwlr_layer_surface_v1_get_popup(Resource*, struct ::wl_resource* popup) {
    // The popup was created parentless a moment ago (see XdgShellExtras); this
    // is where it learns it hangs from this surface.
    QWaylandXdgSurface* surface =
        m_shell->m_extras ? m_shell->m_extras->takeOrphan(popup) : nullptr;
    if (surface) {
        emit m_shell->popupParented(surface, this);
    }
}

void LayerSurface::zwlr_layer_surface_v1_ack_configure(Resource*, uint32_t) {}

void LayerSurface::zwlr_layer_surface_v1_destroy(Resource* resource) {
    wl_resource_destroy(resource->handle);
}

void LayerSurface::zwlr_layer_surface_v1_set_layer(Resource*, uint32_t layer) {
    if (layer <= QtWaylandServer::zwlr_layer_shell_v1::layer_overlay) {
        m_layer = layer;
    }
}

void LayerSurface::zwlr_layer_surface_v1_destroy_resource(Resource*) {
    deleteLater();
}
