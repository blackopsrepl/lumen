import QtQuick
import QtQuick.Window
import QtWayland.Compositor
import QtWayland.Compositor.XdgShell

// Phase 0 spike: a minimal Qt Wayland compositor that hosts one session
// application as a nested Wayland client, renders its surface into the scene
// graph, and forwards pointer input to it.
//
// Notes on the API, learned the hard way:
//  * WaylandSeat is NOT instantiable from QML — the compositor owns a
//    defaultSeat, and pointer input reaches a client through the
//    WaylandQuickItem's own input handling (inputEventsEnabled).
//  * The hosted client keeps its own size; the item is laid out here, so the
//    click target must come from the toplevel's windowGeometry, not from the
//    item's geometry.
Window {
    id: win
    width: 900
    height: 620
    visible: true
    color: "#101418"
    title: "Lumen spike"

    // Flipped by the compositor when a client toplevel maps.
    property bool surfaceReady: false
    property string toplevelTitle: ""
    property int surfaceW: 0
    property int surfaceH: 0

    WaylandCompositor {
        id: comp
        socketName: "lumen-spike"

        WaylandOutput {
            compositor: comp
            sizeFollowsWindow: true
            window: win
        }

        XdgShell {
            onToplevelCreated: (toplevel, xdgSurface) => {
                view.surface = xdgSurface.surface
                win.toplevelTitle = toplevel.title
                toplevel.titleChanged.connect(function () {
                    win.toplevelTitle = toplevel.title
                })
                // The client has not sized itself at creation time; the
                // geometry arrives on a later commit.
                xdgSurface.windowGeometryChanged.connect(function () {
                    win.surfaceW = xdgSurface.windowGeometry.width
                    win.surfaceH = xdgSurface.windowGeometry.height
                })
                win.surfaceW = xdgSurface.windowGeometry.width
                win.surfaceH = xdgSurface.windowGeometry.height
                win.surfaceReady = true
            }
        }
    }

    // The session surface, drawn at its own size and driven by real input.
    WaylandQuickItem {
        id: view
        objectName: "view"
        x: 0
        y: 0
        width: win.surfaceW > 0 ? win.surfaceW : win.width
        height: win.surfaceH > 0 ? win.surfaceH : win.height
        focusOnClick: true
        inputEventsEnabled: true
        onSurfaceDestroyed: win.surfaceReady = false
    }
}
