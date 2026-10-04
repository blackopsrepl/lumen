import QtQuick
import QtQuick.Controls

// Pending notes for the active session.
//
// A note is addressed to one session and stays addressed to it: the panel never
// shows another session's notes, and resolving one acknowledges exactly the note
// it was rendered for.
//
// The list scrolls. It used to be given `parent.height - 60`, which is a guess
// at the heading's height: it clipped the last note when the pane was short and
// left a gap when it was tall. The heading is fixed and the list takes whatever
// is left, so the pane can be dragged to any height and the notes stay reachable.
Item {
    id: panel
    required property string sessionName
    required property var notes

    signal resolveRequested(var id)

    Column {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 10

        Row {
            spacing: 6
            Text {
                text: "Feedback"
                color: Theme.faint
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSizeSmall
                font.capitalization: Font.AllUppercase
                font.letterSpacing: 0.8
            }
            Rectangle {
                visible: panel.notes.length > 0
                anchors.verticalCenter: parent.verticalCenter
                width: badge.width + 12
                height: 16
                radius: 8
                color: Theme.human
                Text {
                    id: badge
                    anchors.centerIn: parent
                    text: panel.notes.length
                    color: Theme.humanInk
                    font.family: Theme.fontSans
                    font.pixelSize: 10
                }
            }
        }

        Text {
            visible: panel.notes.length === 0
            width: parent.width
            text: "No pending notes"
            color: Theme.faint
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSize
        }

        ListView {
            id: list
            width: parent.width
            // Everything below the heading, whatever height the pane is.
            height: parent.height - y
            clip: true
            model: panel.notes
            spacing: 12
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Column {
                required property var modelData
                width: list.width
                spacing: 6

                Text {
                    width: parent.width
                    text: modelData.comment
                    color: Theme.text
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.fontSize
                    wrapMode: Text.WordWrap
                }

                Image {
                    visible: modelData.screenshot === true
                    width: Math.min(parent.width, 200)
                    fillMode: Image.PreserveAspectFit
                    source: modelData.screenshot
                            ? "image://lumen-note/" + panel.sessionName + "/" + modelData.id
                            : ""
                }

                Row {
                    spacing: 8
                    Text {
                        anchors.verticalCenter: parent.verticalCenter
                        text: "#" + modelData.id
                        color: Theme.faint
                        font.family: Theme.fontMono
                        font.pixelSize: Theme.fontSizeSmall
                    }
                    IconButton {
                        iconKind: "check"
                        iconSize: 13
                        implicitWidth: 26
                        implicitHeight: 22
                        tooltipText: "Resolve this note"
                        onClicked: panel.resolveRequested(modelData.id)
                    }
                }
            }
        }
    }
}
