import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The note composer, opened after a region is drawn.
//
// It floats over the frame, so it carries a shadow and the raised tone to read
// as a layer above the canvas rather than a hole in it.
Rectangle {
    id: composer
    color: Theme.raised
    radius: Theme.radiusLarge
    border.color: Theme.lineStrong
    border.width: 1
    width: 400
    height: 176

    signal sendRequested(string comment)
    signal cancelRequested()

    layer.enabled: true

    function open() {
        visible = true
        textArea.forceActiveFocus()
    }

    function close() {
        visible = false
        textArea.text = ""
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 10

        Text {
            text: "Note for the agent"
            color: Theme.text
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSize
            font.weight: Font.Medium
        }

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true

            TextArea {
                id: textArea
                placeholderText: "Describe what should change…"
                color: Theme.text
                placeholderTextColor: Theme.faint
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSize
                wrapMode: TextArea.Wrap
                background: Rectangle {
                    color: Theme.panel2
                    radius: Theme.radius
                    border.width: 1
                    border.color: textArea.activeFocus ? Theme.accent : Theme.lineSoft

                    Behavior on border.color {
                        ColorAnimation { duration: 90 }
                    }
                }
                Keys.onPressed: (event) => {
                    if (event.key === Qt.Key_Return && (event.modifiers & Qt.ControlModifier)) {
                        composer.sendRequested(textArea.text)
                        event.accepted = true
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            Text {
                Layout.alignment: Qt.AlignVCenter
                text: "Ctrl+Enter to send"
                color: Theme.faint
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSizeSmall
            }

            Item { Layout.fillWidth: true }

            IconButton {
                iconKind: "close"
                tooltipText: "Discard this note"
                onClicked: composer.cancelRequested()
            }
            IconButton {
                iconKind: "check"
                tooltipText: "Send the note to the agent (Ctrl+Enter)"
                tone: true
                enabled: textArea.text.trim().length > 0
                onClicked: composer.sendRequested(textArea.text)
            }
        }
    }
}
