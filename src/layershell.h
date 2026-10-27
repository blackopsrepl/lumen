// wlr-layer-shell, the server side.
//
// Qt's compositor module ships xdg-shell and nothing else, so a client that
// asks for a layer surface — a Quickshell PanelWindow, a bar, an overlay —
// finds no zwlr_layer_shell_v1 global, gives up on the window, and the session
// runs with nothing to show. Quickshell is a Qt application; what it needs on
// top of Qt is this one protocol.
//
// The implementation is deliberately small. Lumen has one headless output and
// no other shell clients to arrange around, so layers and exclusive zones only
// decide stacking; a surface's position is its anchors and margins on that
// output, and its size is what it asked for, stretched along any axis it is
// anchored to on both sides.

#pragma once

#include "qwayland-server-wlr-layer-shell-unstable-v1.h"

#include <QtWaylandCompositor/QWaylandCompositorExtension>

#include <QHash>
#include <QMargins>
#include <QObject>
#include <QPoint>
#include <QSize>
#include <QString>

class QWaylandSurface;
class QWaylandXdgSurface;
class LayerSurface;
class XdgShellExtras;

class LayerShell : public QWaylandCompositorExtensionTemplate<LayerShell>,
                   public QtWaylandServer::zwlr_layer_shell_v1 {
    Q_OBJECT
  public:
    /// `outputSize` is the area layer surfaces are anchored within.
    LayerShell(QWaylandCompositor* compositor, const QSize& outputSize);
    ~LayerShell() override;

    void initialize() override;

    /// Where parentless popups are held until a layer surface claims them.
    void setXdgShellExtras(XdgShellExtras* extras) { m_extras = extras; }

    QSize outputSize() const { return m_outputSize; }
    QWaylandCompositor* compositor() const { return m_compositor; }

  signals:
    /// A client gave a surface the layer role. It is not mapped yet: it has no
    /// buffer until it acknowledges its first configure.
    void layerSurfaceCreated(LayerSurface* layerSurface);
    /// A popup was attached to a layer surface — a menu or a drop-down opened
    /// from a panel. `popup` is the popup's xdg_surface.
    void popupParented(QWaylandXdgSurface* popup, LayerSurface* parent);

  protected:
    void zwlr_layer_shell_v1_get_layer_surface(Resource* resource, uint32_t id,
                                               struct ::wl_resource* surface,
                                               struct ::wl_resource* output, uint32_t layer,
                                               const QString& nameSpace) override;
    void zwlr_layer_shell_v1_destroy(Resource* resource) override;

  private:
    friend class LayerSurface;
    /// Sees every request on the display, to catch wl_surface.commit.
    static void logRequest(void* data, enum wl_protocol_logger_type direction,
                           const struct wl_protocol_logger_message* message);

    QWaylandCompositor* m_compositor = nullptr;
    QSize m_outputSize;
    struct wl_protocol_logger* m_logger = nullptr;
    /// Layer surfaces by their wl_surface resource, for the commit hook.
    QHash<struct ::wl_resource*, LayerSurface*> m_bySurface;
    XdgShellExtras* m_extras = nullptr;
};

class LayerSurface : public QObject, public QtWaylandServer::zwlr_layer_surface_v1 {
    Q_OBJECT
  public:
    LayerSurface(LayerShell* shell, QWaylandSurface* surface, uint32_t layer,
                 const QString& nameSpace, struct ::wl_client* client, uint32_t id, int version);
    ~LayerSurface() override;

    QWaylandSurface* surface() const { return m_surface; }
    QString nameSpace() const { return m_nameSpace; }
    /// background = 0, bottom = 1, top = 2, overlay = 3.
    uint32_t layer() const { return m_layer; }
    bool wantsKeyboard() const { return m_keyboard != 0; }

    /// Where the surface sits on the output, from its anchors and margins and
    /// the size it actually committed.
    QPoint position() const;

  signals:
    void positionChanged();

  protected:
    void zwlr_layer_surface_v1_set_size(Resource* resource, uint32_t width,
                                        uint32_t height) override;
    void zwlr_layer_surface_v1_set_anchor(Resource* resource, uint32_t anchor) override;
    void zwlr_layer_surface_v1_set_exclusive_zone(Resource* resource, int32_t zone) override;
    void zwlr_layer_surface_v1_set_margin(Resource* resource, int32_t top, int32_t right,
                                          int32_t bottom, int32_t left) override;
    void zwlr_layer_surface_v1_set_keyboard_interactivity(Resource* resource,
                                                          uint32_t interactivity) override;
    void zwlr_layer_surface_v1_get_popup(Resource* resource, struct ::wl_resource* popup) override;
    void zwlr_layer_surface_v1_ack_configure(Resource* resource, uint32_t serial) override;
    void zwlr_layer_surface_v1_destroy(Resource* resource) override;
    void zwlr_layer_surface_v1_set_layer(Resource* resource, uint32_t layer) override;
    void zwlr_layer_surface_v1_destroy_resource(Resource* resource) override;

  private:
    friend class LayerShell;
    /// Answer a commit. The first one carries no buffer and must be told a
    /// size before the client may draw; later ones are re-configured only when
    /// the client changed something that decides the size.
    void onCommit();
    QSize configuredSize() const;

    LayerShell* m_shell = nullptr;
    QWaylandSurface* m_surface = nullptr;
    struct ::wl_resource* m_surfaceResource = nullptr;
    QString m_nameSpace;
    uint32_t m_layer = 0;
    uint32_t m_anchor = 0;
    uint32_t m_keyboard = 0;
    QSize m_requested;
    QMargins m_margins;
    QSize m_sentSize;
    uint32_t m_serial = 0;
    bool m_configured = false;
    bool m_dirty = true;
};
