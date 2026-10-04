import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtCore

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
    /// Bumped when the client reports a session's notes changed, so the
    /// feedback panel's binding re-reads them.
    property int notesRevision: 0
    /// The region the human drew, held until the note text arrives.
    property var pendingRect: null
    property var activeSession: {
        for (const s of daemon.sessions) {
            if (s.name === window.activeName) return s
        }
        return null
    }

    // UI state the human set by hand, remembered. This is viewer-only — it
    // describes a window, not a session — so it lives in QSettings under the
    // application name, not in the daemon's config.
    Settings {
        id: splitSettings
        category: "viewer"
        property real feedbackHeight: 0
    }

    // An agent driving a session shows it, so what is being driven is visible.
    // The first session to appear is selected on its own: with nothing selected
    // there is no frame subscription and the view stays blank, which reads as
    // the stream being broken rather than as nothing having been chosen.
    Connections {
        target: daemon
        function onSessionsChanged() {
            if (window.activeName !== "" && !window.activeSession) {
                // The session being watched is gone.
                window.activeName = ""
                daemon.setActiveName("")
            }
            if (window.activeName === "" && daemon.sessions.length > 0) {
                window.connectSession(daemon.sessions[0].name)
            }
        }
    }

    RowLayout {
        anchors.fill: parent
        spacing: 0

        // --- sidebar ---
        Rectangle {
            Layout.preferredWidth: 288
            Layout.fillHeight: true
            clip: true
            color: Theme.metalBottom

            ColumnLayout {
                id: sidebarColumn
                anchors.fill: parent
                spacing: 0

                // The feedback pane is resizable: the session list and the notes
                // both want the space, and which deserves more depends on what
                // the human is doing.
                //
                // Three things made the first attempt feel bad, and all three
                // are fixed here rather than tuned:
                //
                //  - A 7px strip is a hard thing to hit. The grabbable band is
                //    18px (the visual line stays 7px, centred in it), and it
                //    reaches into the panes either side so the pointer is
                //    already inside it when the hand arrives.
                //  - Grabbing moved the pane by the distance from the band's
                //    top, so the divider jumped to the cursor. The grab is
                //    anchored instead: the pane takes the offset since press,
                //    and whatever row you grabbed stays under the pointer.
                //  - The height died with the window. It is remembered, so the
                //    split survives a restart and is set once, not every launch.
                //
                // Bounds come from the ColumnLayout, not `parent.height` — the
                // MouseAreas span the band, whose own height is 18. Minimums are
                // each item's Layout.minimumHeight; only the ceiling is
                // computed, from whatever the panes were given.
                readonly property int dividerPx: 18
                readonly property int linePx: 7
                readonly property int sessionListMin: 160
                readonly property int feedbackMin: 90
                readonly property int feedbackDefault: 240

                function feedbackCeiling() {
                    return Math.max(feedbackMin,
                                    height - sessionListMin - dividerPx);
                }

                function clampFeedback(h) {
                    return Math.max(feedbackMin, Math.min(h, feedbackCeiling()));
                }

                // A saved height is only meaningful for this window size: a
                // split set on a tall window would swallow a short one.
                function restoreFeedbackHeight() {
                    if (splitSettings.feedbackHeight > 0) {
                        feedbackPane.Layout.preferredHeight =
                            clampFeedback(splitSettings.feedbackHeight);
                    }
                }

                Component.onCompleted: restoreFeedbackHeight()
                onHeightChanged: restoreFeedbackHeight()

                SessionSidebar {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    Layout.minimumHeight: sidebarColumn.sessionListMin
                    sessions: daemon.sessions
                    activeName: window.activeName
                    onSessionActivated: (name) => window.connectSession(name)
                }

                Rectangle {
                    id: feedbackDivider
                    Layout.fillWidth: true
                    implicitHeight: sidebarColumn.dividerPx
                    color: "transparent"

                    Rectangle {
                        anchors.centerIn: parent
                        width: 34
                        height: dividerMouse.containsMouse || dividerMouse.pressed
                                ? 5 : 3
                        radius: height / 2
                        color: dividerMouse.containsMouse || dividerMouse.pressed
                               ? Theme.accent : Theme.lineStrong
                    }

                    MouseArea {
                        id: dividerMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        preventStealing: true
                        cursorShape: Qt.SizeVerCursor
                        property real pressGlobalY: 0
                        property real pressHeight: 0
                        onPressed: (mouse) => {
                            // Window coordinates, not band-local ones. The band
                            // moves as the pane resizes, so measuring `mouse.y`
                            // against it feeds the band's own movement back into
                            // the next delta: the pane then chased the pointer at
                            // roughly half speed and wobbled. A point mapped to
                            // the window is the pointer's real position and is
                            // unaffected by what the divider does.
                            pressGlobalY = mapToItem(null, mouse.x, mouse.y).y
                            pressHeight = feedbackPane.height
                        }
                        onPositionChanged: (mouse) => {
                            if (!pressed) return
                            const globalY = mapToItem(null, mouse.x, mouse.y).y
                            feedbackPane.Layout.preferredHeight =
                                sidebarColumn.clampFeedback(
                                    pressHeight - (globalY - pressGlobalY))
                        }
                        onReleased: splitSettings.feedbackHeight = feedbackPane.height
                        onDoubleClicked: {
                            feedbackPane.Layout.preferredHeight =
                                sidebarColumn.clampFeedback(
                                    sidebarColumn.feedbackDefault)
                            splitSettings.feedbackHeight = feedbackPane.height
                        }
                    }
                }

                // --- feedback ---
                FeedbackPanel {
                    id: feedbackPane
                    Layout.fillWidth: true
                    Layout.preferredHeight: sidebarColumn.feedbackDefault
                    Layout.minimumHeight: sidebarColumn.feedbackMin
                    sessionName: window.activeName
                    // `daemon.notes(name)` is a method call, and a binding to a
                    // method call is evaluated once and never again — so the
                    // panel was handed the empty list from before the notes
                    // arrived and stayed empty however many notes came in. It is
                    // re-read here when the client says that session's notes
                    // changed, which is the event that actually carries them.
                    notes: {
                        void notesRevision
                        return daemon.notes(window.activeName)
                    }
                    Connections {
                        target: daemon
                        function onNotesChanged(session) {
                            if (session === window.activeName) notesRevision++
                        }
                    }
                    onResolveRequested: (id) => {
                        daemon.resolveNote(window.activeName, id)
                    }
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
                zoomPercent: canvas.zoomPercent
                onTakeControl: window.humanControlling = true
                onReleaseControl: window.humanControlling = false
                onFullscreenRequested: {
                    if (window.visibility === Window.FullScreen) window.showNormal()
                    else window.showFullScreen()
                }
                onAnnotateToggled: window.annotating = !window.annotating
                onSettingsRequested: settingsDialog.open()
                onFitRequested: canvas.fit()
                onActualSizeRequested: canvas.actualSize()
                onStopRequested: {
                    if (window.activeName !== "") daemon.stopSession(window.activeName)
                }
            }

            // The stage: a recessed well, so the frame sits in the machine
            // rather than on it. One hairline border, no bright rim.
            Rectangle {
                Layout.fillWidth: true
                Layout.fillHeight: true
                Layout.margins: 10
                clip: true
                radius: Theme.radius
                border.width: 1
                border.color: Theme.metalSeam
                color: Theme.wellTop

                Item {
                    id: stageArea
                    anchors.fill: parent
                    anchors.margins: 1
                    clip: true

                SessionCanvas {
                    id: canvas
                    anchors.fill: parent
                    frame: daemon.frame
                    frameUrl: daemon.frameUrl
                    activeName: window.activeName
                    humanControlling: window.humanControlling
                    annotating: window.annotating
                    onPointerDown: (x, y) => daemon.click(x, y)
                    onPointerUp: (x, y) => daemon.click(x, y)
                    onKeyTyped: (text) => daemon.type(text)
                }

                // The annotation layer and the note composer. A drawn region is
                // cropped out of the frame the human was looking at, so the note
                // still shows what they meant after the session has moved on.
                AnnotationOverlay {
                    id: overlay
                    anchors.fill: parent
                    // Above the canvas, below the composer.
                    z: 1
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
                    z: 2
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
                            text: "Sessions are started by an agent with lumen-cli. Start one, then it appears here and you can watch and steer it."
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
    }

    function connectSession(name) {
        window.activeName = name
        window.humanControlling = false
        window.annotating = false
        // Every session opens fitted. Without this, switching sessions carries
        // the previous one's zoom and pan across, so a session can open at 400%
        // and panned off its own edge — which reads as a broken view rather than
        // as a zoom the human set on something else entirely.
        canvas.resetView()
        daemon.setActiveName(name)
        daemon.refreshNotes(name)
    }

    // Send the drawn region and the note text to the daemon. The region is
    // measured in frame pixels here — in QML, which knows the stage — and the
    // crop itself happens in C++, because QImage::copy is not invokable and the
    // crop that used to live here threw at the call site without an error.
    function sendNote(comment) {
        if (!comment || !window.activeName) return
        const rect = window.frameRegion(window.pendingRect)
        daemon.addNote(window.activeName, comment,
                       rect ? rect.x : 0, rect ? rect.y : 0,
                       rect ? rect.width : 0, rect ? rect.height : 0)
        window.annotating = false
        overlay.clear()
        window.pendingRect = null
    }

    // Map the drawn rectangle from stage coordinates to frame pixels.
    //
    // Both ends of the mapping live on the canvas (mapToFrame and zoom/pan), so
    // this asks it rather than recomputing the transform — a second copy of the
    // maths here is exactly how a click and a note region drift apart once zoom
    // exists. Returns geometry, not pixels: QML cannot build a QImage.
    function frameRegion(rect) {
        if (!rect || !daemon.frame) return null
        if (!canvas.frameW || !canvas.frameH) return null
        // The overlay and the stage are siblings inside the same well, so the
        // drawn rectangle is already in the canvas's own coordinates — the
        // inset lives inside the canvas, and mapToFrame accounts for it.
        const topLeft = canvas.mapToFrame(rect.x, rect.y)
        const bottomRight = canvas.mapToFrame(rect.x + rect.width, rect.y + rect.height)
        const x = Math.max(0, Math.round(Math.min(topLeft.x, bottomRight.x)))
        const y = Math.max(0, Math.round(Math.min(topLeft.y, bottomRight.y)))
        const w = Math.max(1, Math.round(Math.abs(bottomRight.x - topLeft.x)))
        const h = Math.max(1, Math.round(Math.abs(bottomRight.y - topLeft.y)))
        return {
            x: x,
            y: y,
            width: Math.min(w, canvas.frameW - x),
            height: Math.min(h, canvas.frameH - y)
        }
    }

    // Escape releases human control. The handler lives on a Shortcut rather than
    // as Keys.onPressed on the window: an attached Keys property only exists on
    // an Item, and the window is not one.
    Shortcut {
        sequence: StandardKey.Cancel
        enabled: window.humanControlling
        onActivated: window.humanControlling = false
    }

    // Zoom keys, as the old viewer had them. They are disabled while the human
    // is driving a session, because then +/- are the session's keystrokes.
    Shortcut {
        sequences: ["+", "="]
        enabled: !window.humanControlling
        onActivated: canvas.zoomCenter(1.2)
    }
    Shortcut {
        sequences: ["-", "_"]
        enabled: !window.humanControlling
        onActivated: canvas.zoomCenter(1 / 1.2)
    }
    Shortcut {
        sequence: "Ctrl+0"
        onActivated: canvas.actualSize()
    }
    Shortcut {
        sequence: "Ctrl+9"
        onActivated: canvas.fit()
    }

    // Keyboard input goes to the session while the human holds control. The
    // handler is on the stage rather than the window because an attached Keys
    // property only exists on an Item.
    Item {
        id: keySink
        anchors.fill: parent
        focus: window.humanControlling
        Keys.onPressed: (event) => canvas.sendKey(event)
    }

    // Daemon configuration, reached from the gear in the toolbar.
    SettingsPanel {
        id: settingsDialog
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
