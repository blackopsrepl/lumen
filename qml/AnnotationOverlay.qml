import QtQuick
import QtQuick.Controls

// The annotation overlay: drag a rectangle on the surface, type a note, send it.
// The rectangle is cropped out of the *live* surface at send time, so the note
// carries the pixels the human meant rather than coordinates that a redraw could
// invalidate.
Item {
    id: overlay
    required property bool annotating
    required property var sessionCanvas

    property point drawStart
    property point drawEnd
    property bool drawing: false
    readonly property bool hasSelection: drawing || (drawStart.x !== drawEnd.x && drawStart.y !== drawEnd.y)

    signal sendRequested(var rect)

    function rect() {
        if (!hasSelection) return null
        return {
            x: Math.min(drawStart.x, drawEnd.x),
            y: Math.min(drawStart.y, drawEnd.y),
            width: Math.abs(drawEnd.x - drawStart.x),
            height: Math.abs(drawEnd.y - drawStart.y)
        }
    }

    function clear() {
        drawStart = Qt.point(0, 0)
        drawEnd = Qt.point(0, 0)
        drawing = false
        canvas.requestPaint()
    }

    visible: annotating
    // Only intercept the pointer while a region is actually being drawn;
    // otherwise this layer would swallow every click meant for the session.
    enabled: annotating

    Canvas {
        id: canvas
        anchors.fill: parent
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            if (!overlay.hasSelection) return
            const r = overlay.rect()
            // The in-progress selection uses the human brass, never the agent
            // accent: it is the one rectangle the human is drawing right now.
            ctx.fillStyle = "rgba(216, 160, 78, 0.14)"
            ctx.strokeStyle = "#d8a04e"
            ctx.lineWidth = 1.5
            ctx.fillRect(r.x, r.y, r.width, r.height)
            ctx.strokeRect(r.x, r.y, r.width, r.height)
        }
    }

    MouseArea {
        anchors.fill: parent
        onPressed: (mouse) => {
            overlay.drawStart = Qt.point(mouse.x, mouse.y)
            overlay.drawEnd = overlay.drawStart
            overlay.drawing = true
            canvas.requestPaint()
        }
        onPositionChanged: (mouse) => {
            if (!overlay.drawing) return
            overlay.drawEnd = Qt.point(mouse.x, mouse.y)
            canvas.requestPaint()
        }
        onReleased: (mouse) => {
            overlay.drawing = false
            canvas.requestPaint()
            const r = overlay.rect()
            // A selection has to be a real region, not a stray click.
            if (r && r.width > 8 && r.height > 8) {
                overlay.sendRequested(r)
            } else {
                overlay.clear()
            }
        }
    }
}
