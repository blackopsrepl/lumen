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
                onTakeControl: window.humanControlling = true
                onReleaseControl: window.humanControlling = false
                onFullscreenRequested: {
                    if (window.visibility === Window.FullScreen) window.showNormal()
                    else window.showFullScreen()
                }
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
        daemon.setActiveName(name)
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

    Keys.onPressed: (event) => {
        if (event.key === Qt.Key_Escape) {
            if (window.humanControlling) window.humanControlling = false
        }
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
