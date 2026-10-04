import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Lumen viewer: a thin client for lumen-daemon.
//
// It owns no sessions and no compositor. It renders frames the daemon sends and
// forwards human input. Closing this window ends nothing but the view.
ApplicationWindow {
    id: window
    visible: true
    width: 1280
    height: 820
    title: "Lumen"
    color: Theme.bg

    property string activeName: ""
    property bool humanControlling: false
    property bool annotating: false
    /// The region the human drew, held until the note text arrives.
    property var pendingRect: null
    property var activeSession: {
        for (const s of daemon.sessions) {
            if (s.name === window.activeName) return s
        }
        return null
    }

    // An agent driving a session shows it, so what is being driven is visible.
    Connections {
        target: daemon
        function onSessionsChanged() {
            // If the active session disappeared, clear it.
            if (window.activeName !== "" && !window.activeSession) {
                window.activeName = ""
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // --- sidebar ---
        Rectangle {
            Layout.preferredWidth: 280
            Layout.fillHeight: true
            color: Theme.panel

            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                SessionSidebar {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    sessions: daemon.sessions
                    activeName: window.activeName
                    onSessionActivated: (name) => window.connectSession(name)
                }

                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.lineSoft }

                // --- new session ---
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.margins: 12
                    spacing: 6
                    TextField {
                        id: newName
                        Layout.fillWidth: true
                        placeholderText: "new session"
                        color: Theme.text
                        font.family: Theme.fontSans
                        background: Rectangle {
                            color: Theme.panel2; radius: Theme.radius
                            border.color: Theme.lineSoft
                        }
                    }
                    TextField {
                        id: newCommand
                        Layout.fillWidth: true
                        placeholderText: "Qt app command (absolute)"
                        color: Theme.text
                        font.family: Theme.fontMono
                        font.pixelSize: Theme.fontSizeSmall
                        background: Rectangle {
                            color: Theme.panel2; radius: Theme.radius
                            border.color: Theme.lineSoft
                        }
                    }
                    RowLayout {
                        Layout.fillWidth: true
                        spacing: 6
                        Button {
                            text: "Create"
                            Layout.fillWidth: true
                            onClicked: window.createSession()
                        }
                        Button {
                            text: "Stop"
                            enabled: window.activeName !== ""
                            onClicked: daemon.stopSession(window.activeName)
                        }
                    }
                }

                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.lineSoft }

                // --- feedback ---
                FeedbackPanel {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 260
                    sessionName: window.activeName
                    notes: daemon.notes(window.activeName)
                    onResolveRequested: (id) => {
                        daemon.resolveNote(window.activeName, id)
                    }
                }

                Rectangle { Layout.fillWidth: true; height: 1; color: Theme.lineSoft }

                // --- settings ---
                SettingsPanel {
                    Layout.fillWidth: true
                    Layout.margins: 12
                }
            }
        }

        Rectangle { Layout.preferredWidth: 1; Layout.fillHeight: true; color: Theme.line }

        // --- main pane ---
        ColumnLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            ControlBar {
                Layout.fillWidth: true
                activeSession: window.activeSession
                humanControlling: window.humanControlling
                annotating: window.annotating
                onTakeControl: window.humanControlling = true
                onReleaseControl: window.humanControlling = false
                onFullscreenRequested: {
                    if (window.visibility === Window.FullScreen) window.showNormal()
                    else window.showFullScreen()
                }
                onAnnotateToggled: window.annotating = !window.annotating
            }

            Item {
                id: stageArea
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                SessionCanvas {
                    id: canvas
                    anchors.fill: parent
                    frame: daemon.frame
                    activeName: window.activeName
                    humanControlling: window.humanControlling
                    onPointerDown: (x, y) => daemon.click(x, y)
                    onPointerUp: (x, y) => daemon.click(x, y)
                }

                // The annotation layer and the note composer. A drawn region is
                // cropped out of the frame the human was looking at, so the note
                // still shows what they meant after the session has moved on.
                AnnotationOverlay {
                    id: overlay
                    anchors.fill: parent
                    annotating: window.annotating
                    sessionCanvas: canvas
                    onSendRequested: (rect) => {
                        window.pendingRect = rect
                        composer.open()
                    }
                }

                FeedbackComposer {
                    id: composer
                    visible: false
                    anchors.centerIn: parent
                    onSendRequested: (comment) => {
                        window.sendNote(comment)
                        composer.close()
                    }
                    onCancelRequested: composer.close()
                }

                // Empty state.
                Rectangle {
                    anchors.centerIn: parent
                    visible: window.activeName === ""
                    width: 420
                    height: 120
                    color: Theme.panel
                    radius: Theme.radius
                    border.color: Theme.lineSoft
                    Column {
                        anchors.centerIn: parent
                        spacing: 8
                        Text {
                            anchors.horizontalCenter: parent.horizontalCenter
                            text: "No session selected"
                            color: Theme.text
                            font.family: Theme.fontSans
                            font.pixelSize: 17
                        }
                        Text {
                            width: 360
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.WordWrap
                            text: "A session is one Qt application hosted by Lumen's daemon. Create one, then an agent attaches to it by name."
                            color: Theme.muted
                            font.family: Theme.fontSans
                            font.pixelSize: Theme.fontSize
                        }
                    }
                }

                // Human-control banner.
                Rectangle {
                    anchors.horizontalCenter: parent.horizontalCenter
                    anchors.bottom: parent.bottom
                    anchors.bottomMargin: 18
                    visible: window.humanControlling
                    width: bannerRow.width + 28
                    height: 34
                    radius: 17
                    color: Theme.human
                    Row {
                        id: bannerRow
                        anchors.centerIn: parent
                        spacing: 8
                        Rectangle {
                            anchors.verticalCenter: parent.verticalCenter
                            width: 8; height: 8; radius: 4
                            color: Theme.humanInk
                        }
                        Text {
                            text: "You have control — press Esc to release"
                            color: Theme.humanInk
                            font.family: Theme.fontSans
                            font.pixelSize: Theme.fontSize
                        }
                    }
                }
            }
        }
    }

    function connectSession(name) {
        window.activeName = name
        window.humanControlling = false
        window.annotating = false
        daemon.setActiveName(name)
        daemon.refreshNotes(name)
    }

    function createSession() {
        const name = newName.text.trim()
        const command = newCommand.text.trim()
        if (!name || !command) return
        daemon.createSession(name, command, false, "")
        newName.text = ""
        newCommand.text = ""
        window.connectSession(name)
    }

    // Send the drawn region and the note text to the daemon. The pixels are
    // taken from the frame the human was looking at, so the note keeps showing
    // what they meant even after the session redraws.
    function sendNote(comment) {
        if (!comment || !window.activeName) return
        const region = window.captureRegion(window.pendingRect)
        daemon.addNote(window.activeName, comment, region)
        window.annotating = false
        overlay.clear()
        window.pendingRect = null
    }

    // Crop the pending region out of the displayed frame, in frame pixels.
    function captureRegion(rect) {
        if (!rect || !daemon.frame) return null
        const img = daemon.frame
        if (!img.width || !img.height) return null
        // The stage preserves aspect ratio, so the displayed frame is inset
        // within the canvas; the region is in stage coordinates and must be
        // mapped back to frame pixels before cropping.
        const stageW = stageArea.width
        const stageH = stageArea.height
        const scale = Math.min(stageW / img.width, stageH / img.height)
        const offsetX = (stageW - img.width * scale) / 2
        const offsetY = (stageH - img.height * scale) / 2
        const x = Math.max(0, Math.round((rect.x - offsetX) / scale))
        const y = Math.max(0, Math.round((rect.y - offsetY) / scale))
        const w = Math.max(1, Math.round(rect.width / scale))
        const h = Math.max(1, Math.round(rect.height / scale))
        return img.copy(x, y, Math.min(w, img.width - x), Math.min(h, img.height - y))
    }

    // Escape releases human control. The handler lives on a Shortcut rather than
    // as Keys.onPressed on the window: an attached Keys property only exists on
    // an Item, and the window is not one.
    Shortcut {
        sequence: StandardKey.Cancel
        enabled: window.humanControlling
        onActivated: window.humanControlling = false
    }

    // One probe for the palette: if Theme did not resolve, its colours are
    // undefined, which is otherwise invisible — the window simply renders in the
    // default palette with no error of its own.
    Component.onCompleted: {
        console.log("lumen: Theme.bg =", Theme.bg)
    }

    Text {
        id: status
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        anchors.margins: 8
        color: Theme.danger
        font.family: Theme.fontSans
        font.pixelSize: Theme.fontSizeSmall
        text: ""
    }
}
