import QtQuick

// The view stage: renders the daemon's frame for the active session.
//
// The daemon owns the compositor and renders the session's surface. This item
// just displays the frame it receives and forwards human clicks. There is no
// surface item here, no placement logic, and no compositor — which is the whole
// point of the daemon/viewer split.
Item {
    id: stage
    required property var frame
    /// URL of the current frame, served by the daemon's image provider.
    required property string frameUrl
    required property string activeName
    required property bool humanControlling

    signal pointerDown(real x, real y)
    signal pointerUp(real x, real y)

    // The frame, scaled to fit while preserving aspect ratio.
    Image {
        id: frameImage
        anchors.fill: parent
        source: stage.frameUrl
        fillMode: Image.PreserveAspectFit
        smooth: true
        cache: false
        // Synchronous on purpose: the provider returns an in-memory QImage, so
        // loading is immediate — and an asynchronous load would be cancelled by
        // the next frame's URL before it ever completed, leaving the view blank.
        asynchronous: false
    }

    // Forward clicks to the daemon, mapped to surface coordinates. This is the
    // human's input path into the session, and the agent's travels the same
    // route through the daemon's socket, so the two cannot diverge.
    MouseArea {
        id: pointer
        anchors.fill: parent
        enabled: stage.humanControlling
        acceptedButtons: Qt.LeftButton

        onPressed: (mouse) => {
            const surface = mapToSurface(mouse.x, mouse.y)
            stage.pointerDown(surface.x, surface.y)
        }
        onReleased: (mouse) => {
            const surface = mapToSurface(mouse.x, mouse.y)
            stage.pointerUp(surface.x, surface.y)
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

    signal pointerMoved(real x, real y)
    signal keyTyped(string text)

    // Canvas coordinates to surface coordinates.
    function mapToSurface(x, y) {
        const img = frameImage
        if (!img.sourceSize.width || !img.sourceSize.height) return { x: 0, y: 0 }
        const scale = Math.min(img.width / img.sourceSize.width, img.height / img.sourceSize.height)
        const offsetX = (img.width - img.sourceSize.width * scale) / 2
        const offsetY = (img.height - img.sourceSize.height * scale) / 2
        return {
            x: (x - offsetX) / scale,
            y: (y - offsetY) / scale
        }
    }
}
