import QtQuick
import QtQuick.Controls

// The note composer, opened after a region is drawn.
Rectangle {
    id: composer
    color: Theme.raised
    radius: Theme.radius
    border.color: Theme.line
    border.width: 1
    width: 380
    height: 150

    signal sendRequested(string comment)
    signal cancelRequested()

    function open() {
        visible = true
        textArea.forceActiveFocus()
    }

    function close() {
        visible = false
        textArea.text = ""
    }

    Column {
        anchors.fill: parent
        anchors.margins: 12
        spacing: 8

        Text {
            text: "Note for the agent"
            color: Theme.muted
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSizeSmall
        }

        ScrollView {
            width: parent.width
            height: 60
            TextArea {
                id: textArea
                placeholderText: "Describe what should change…"
                color: Theme.text
                placeholderTextColor: Theme.faint
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSize
                background: Rectangle {
                    color: Theme.panel2
                    radius: Theme.radius
                    border.color: Theme.lineSoft
                }
                Keys.onPressed: (event) => {
                    if (event.key === Qt.Key_Return && (event.modifiers & Qt.ControlModifier)) {
                        composer.sendRequested(textArea.text)
                        event.accepted = true
                    }
                }
            }
        }

        Row {
            anchors.right: parent.right
            spacing: 8
            Button {
                text: "Cancel"
                flat: true
                onClicked: composer.cancelRequested()
            }
            Button {
                text: "Send note"
                highlighted: true
                onClicked: composer.sendRequested(textArea.text)
            }
        }
    }
}
