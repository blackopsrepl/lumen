import QtQuick
import QtQuick.Controls

// The toolbar. The address field, tabs and page scale are gone: they were
// browser concepts, and this application hosts Qt clients. Everything that
// described a *surface* rather than a page is kept.
Item {
    id: bar
    required property var activeSession
    required property bool humanControlling
    required property real zoomPercent
    required property bool annotating

    signal releaseControl()
    signal takeControl()
    signal fitRequested()
    signal actualSizeRequested()
    signal fullscreenRequested()
    signal annotateToggled()

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
                text: Math.round(bar.zoomPercent * 100) + "%"
                flat: true
                onClicked: bar.actualSizeRequested()
            }
            Button {
                text: "Fit"
                flat: true
                onClicked: bar.fitRequested()
            }
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
