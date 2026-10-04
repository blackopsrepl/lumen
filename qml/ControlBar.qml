import QtQuick
import QtQuick.Controls

// The toolbar. The address field, tabs and page scale are gone: they were
// browser concepts, and this application hosts Qt clients. The viewer displays a
// frame the daemon rendered, so there is no zoom or annotation state here.
Item {
    id: bar
    required property var activeSession
    required property bool humanControlling
    required property bool annotating

    signal releaseControl()
    signal takeControl()
    signal fullscreenRequested()
    signal annotateToggled()
    signal settingsRequested()

    implicitHeight: 44

    Rectangle {
        anchors.fill: parent
        color: Theme.panel

        Text {
            id: title
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: 14
            anchors.right: controls.left
            anchors.rightMargin: 12
            text: bar.activeSession
                  ? (bar.activeSession.title || bar.activeSession.name)
                  : "No session selected"
            color: bar.activeSession ? Theme.text : Theme.faint
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSize
            elide: Text.ElideRight
        }

        Row {
            id: controls
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: 14
            spacing: 6

            Button {
                text: "⤢"
                flat: true
                onClicked: bar.fullscreenRequested()
            }

            Rectangle {
                width: 1
                height: 22
                anchors.verticalCenter: parent.verticalCenter
                color: Theme.line
            }

            // The note path: draw a region on the frame, describe it, and it is
            // addressed to the session for the agent to pick up.
            Button {
                text: "Comment"
                flat: true
                highlighted: bar.annotating
                onClicked: bar.annotateToggled()
            }
            Button {
                text: bar.humanControlling ? "Release control" : "Take control"
                highlighted: bar.humanControlling
                onClicked: bar.humanControlling ? bar.releaseControl() : bar.takeControl()
            }

            // Settings live behind here rather than in the sidebar: the daemon
            // is configured once, and the main window belongs to the session.
            Button {
                text: "⚙"
                flat: true
                onClicked: bar.settingsRequested()
            }
        }

        // Bottom hairline, as the toolbar had.
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }
}
