import QtQuick
import QtQuick.Controls

// Pending notes for the active session. A note is addressed to one session and
// stays addressed to it: the panel never shows another session's notes, and
// resolving one acknowledges exactly the note it was rendered for.
Item {
    id: panel
    required property string sessionName
    required property var notes

    signal resolveRequested(var id)

    Column {
        anchors.fill: parent
        anchors.margins: 14
        spacing: 8

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
            text: "No pending notes"
            color: Theme.faint
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSize
        }

        ListView {
            width: parent.width
            height: parent.height - 60
            clip: true
            model: panel.notes
            spacing: 10
            delegate: Column {
                required property var modelData
                width: ListView.view.width
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
                    width: Math.min(parent.width, 220)
                    fillMode: Image.PreserveAspectFit
                    source: modelData.screenshot
                            ? "image://lumen-note/" + panel.sessionName + "/" + modelData.id
                            : ""
                }

                Row {
                    spacing: 8
                    Text {
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
