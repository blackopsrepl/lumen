import QtQuick

// The view stage: renders the daemon's frame for the active session, with the
// zoom and pan the viewer has always had.
//
// The daemon owns the compositor and renders the session's surface; this item
// displays the frame it receives and forwards human clicks. There is no surface
// item here, no compositor — that is the point of the daemon/viewer split.
//
// Zoom is view-only: it scales what the human sees and never changes what the
// session believes its size is. The frame is drawn at `scale` about a `pan`
// offset, and both the pointer mapping and the note region map back through the
// same transform, so a click always lands where the human aimed whatever the
// zoom.
Item {
    id: stage
    required property var frame
    /// URL of the current frame, served by the daemon's image provider.
    required property string frameUrl
    required property string activeName
    required property bool humanControlling

    signal pointerDown(real x, real y)
    signal pointerUp(real x, real y)
    signal pointerMoved(real x, real y)
    signal keyTyped(string text)

    // --- zoom state ---------------------------------------------------------

    /// 1.0 is actual size. The old viewer clamped to 0.1–4.
    property real scale: 1.0
    /// Top-left of the drawn frame, in stage coordinates.
    property real panX: 0
    property real panY: 0
    /// True while the frame is being fitted to the stage; any manual zoom or
    /// pan clears it, so an incoming frame does not yank the view back.
    property bool fitted: true

    readonly property real minScale: 0.1
    readonly property real maxScale: 4.0
    /// The percentage the old viewer showed bottom-right of the frame.
    readonly property int zoomPercent: Math.round(scale * 100)

    // The frame's pixel size, read from the loaded image.
    //
    // Not from the `frame` QImage: in QML a QImage's width and height are
    // *methods*, not properties, so `frame.width` is a function object — truthy,
    // and arithmetic on it yields NaN. That silently collapsed the drawn image
    // to nothing. `sourceSize` is the real thing.
    readonly property real frameW: frameImage.sourceSize.width
    readonly property real frameH: frameImage.sourceSize.height

    // --- view transform -----------------------------------------------------

    /// Fit the whole frame in the stage, scaling up as well as down.
    ///
    /// This matches the `PreserveAspectFit` the canvas used before zoom existed:
    /// a frame smaller than the stage fills it rather than sitting in the middle
    /// behind a black border, and a frame larger than the stage is shown whole
    /// rather than opening cropped. The only difference is that the fit is now an
    /// explicit scale, which the zoom builds on.
    function fit() {
        const vw = viewport.width
        const vh = viewport.height
        if (frameW <= 0 || frameH <= 0 || vw <= 0 || vh <= 0) return
        scale = Math.min(vw / frameW, vh / frameH)
        fitted = true
        center()
    }

    /// Back to 1:1, centered. Bound to the percentage readout, as it was.
    function actualSize() {
        if (frameW <= 0) return
        scale = 1
        fitted = false
        center()
    }

    function center() {
        panX = (viewport.width - frameW * scale) / 2
        panY = (viewport.height - frameH * scale) / 2
    }

    /// Zoom about a point in stage coordinates, so the pixel under the cursor
    /// stays under the cursor — the behaviour a document viewer has and the
    /// reason `zoomAt` takes a point rather than just a factor.
    function zoomAt(px, py, factor) {
        if (frameW <= 0) return
        const next = Math.min(maxScale, Math.max(minScale, scale * factor))
        if (next === scale) return
        panX = px - (px - panX) * (next / scale)
        panY = py - (py - panY) * (next / scale)
        scale = next
        fitted = false
    }

    function zoomCenter(factor) {
        zoomAt(viewport.width / 2, viewport.height / 2, factor)
    }

    // --- mapping ------------------------------------------------------------

    /// Stage coordinates to frame pixels, through the live zoom and pan. Both
    /// the pointer path and the note region use this, so they cannot disagree.
    function mapToFrame(x, y) {
        if (frameW <= 0 || scale <= 0) return { x: 0, y: 0 }
        return {
            x: (x - panX) / scale,
            y: (y - panY) / scale
        }
    }

    /// The frame's drawn rectangle in stage coordinates, for the annotation
    /// overlay to clip to.
    function frameRect() {
        return { x: panX, y: panY, width: frameW * scale, height: frameH * scale }
    }

    /// Re-fit when the first frame arrives or the drawing area is resized, but
    /// only while the view is still fitted — a manual zoom must survive both.
    ///
    /// The listeners are on the viewport, not on this root item: the viewport is
    /// the inset area the frame is actually fitted into, so its size is the one
    /// that matters.
    ///
    /// `frameW` is bound to `sourceSize`, which QML only updates *after* the
    /// image has decoded — so the fit that ran while the frame was still loading
    /// saw a size of zero and did nothing, and the fit that the size change
    /// triggered then ran too early to be the last word. Both are handled by
    /// fitting on the change *and* whenever a new source has settled.
    onFrameWChanged: fitIfFitted()
    onFrameHChanged: fitIfFitted()

    Component.onCompleted: {
        viewport.widthChanged.connect(fitIfFitted)
        viewport.heightChanged.connect(fitIfFitted)
    }

    function fitIfFitted() {
        if (!fitted) return
        if (frameW <= 0 || frameH <= 0) return
        fit()
    }

    // --- the frame ----------------------------------------------------------

    Item {
        id: viewport
        anchors.fill: parent
        // A small inset, so a fitted frame does not touch the well's border.
        // The old canvas got this for free from PreserveAspectFit leaving a
        // margin; now that the fit is an explicit scale, it has to be asked for.
        anchors.margins: 8
        clip: true

        Image {
            id: frameImage
            // Positioned and scaled by the transform rather than by
            // fillMode: an explicit rectangle is what makes zoom and pan exact.
            x: stage.panX
            y: stage.panY
            width: stage.frameW * stage.scale
            height: stage.frameH * stage.scale
            source: stage.frameUrl
            smooth: true
            cache: false
            // Synchronous on purpose: the provider returns an in-memory QImage,
            // so loading is immediate — and an asynchronous load would be
            // cancelled by the next frame's URL before it completed, leaving
            // the view blank.
            asynchronous: false

            // QML sets `sourceSize` only once the image has decoded, and the fit
            // has to happen after that — a fit that runs while the frame is still
            // loading sees size 0 and leaves the view at 1:1. Re-fit here, on the
            // status change, so the first frame lands fitted.
            onStatusChanged: if (status === Image.Ready) stage.fitIfFitted()
        }
    }

    // --- input --------------------------------------------------------------

    // Forward clicks to the daemon, mapped through the zoom. This is the
    // human's input path into the session, and the agent's travels the same
    // route through the daemon's socket, so the two cannot diverge.
    MouseArea {
        id: pointer
        anchors.fill: parent
        enabled: stage.humanControlling
        acceptedButtons: Qt.LeftButton
        // The wheel is handled below: while controlling a session it belongs to
        // the session, and only otherwise does it zoom.
        onWheel: (wheel) => wheel.accepted = false

        property bool panning: false
        property real lastX: 0
        property real lastY: 0

        onPressed: (mouse) => {
            const p = stage.mapToFrame(mouse.x, mouse.y)
            stage.pointerDown(p.x, p.y)
        }
        onReleased: (mouse) => {
            const p = stage.mapToFrame(mouse.x, mouse.y)
            stage.pointerUp(p.x, p.y)
        }
        onPositionChanged: (mouse) => {
            const p = stage.mapToFrame(mouse.x, mouse.y)
            stage.pointerMoved(p.x, p.y)
        }
    }

    // Panning and zooming the view, for the human looking at a session they are
    // not driving. Dragging moves the frame at any zoom, the way a document
    // reader pans; the wheel zooms about the pointer. Both are deliberately
    // inert while the human has control, because then the drag and the wheel
    // belong to the session.
    MouseArea {
        id: viewportInput
        anchors.fill: parent
        enabled: !stage.humanControlling
        acceptedButtons: Qt.LeftButton
        hoverEnabled: true

        property real lastX: 0
        property real lastY: 0

        onPressed: (mouse) => {
            lastX = mouse.x
            lastY = mouse.y
            cursorShape = Qt.ClosedHandCursor
        }
        onReleased: cursorShape = Qt.OpenHandCursor
        onPositionChanged: (mouse) => {
            if (!pressed) return
            stage.panX += mouse.x - lastX
            stage.panY += mouse.y - lastY
            lastX = mouse.x
            lastY = mouse.y
            stage.fitted = false
        }
        onWheel: (wheel) => {
            stage.zoomAt(wheel.x, wheel.y, wheel.angleDelta.y < 0 ? 1 / 1.1 : 1.1)
            wheel.accepted = true
        }
    }

    /// Deliver a key press to the session. Text keys go to the daemon as text,
    /// so the client's own keyboard handling decides what they mean.
    function sendKey(event) {
        if (event.text.length > 0) {
            stage.keyTyped(event.text)
            event.accepted = true
        } else if (event.key === Qt.Key_Return || event.key === Qt.Key_Enter) {
            stage.keyTyped("\n")
            event.accepted = true
        } else if (event.key === Qt.Key_Backspace) {
            stage.keyTyped("\b")
            event.accepted = true
        }
    }
}
