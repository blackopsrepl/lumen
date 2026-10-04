import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The chrome both dialogs share: a metal header bar over a body.
//
// Use it as the `contentItem` of a Dialog:
//
//     Dialog {
//         contentItem: DialogFrame {
//             heading: "Lumen settings"
//             ColumnLayout { ... body, ending in an action row ... }
//         }
//     }
//
// It is a ColumnLayout so the Dialog can compute an implicit size from its
// children — a plain Column reports none and the content is silently clipped,
// which is exactly what the hand-rolled settings dialog did.
ColumnLayout {
    id: frame

    required property string heading

    spacing: 0

    // --- header ---
    Rectangle {
        Layout.fillWidth: true
        implicitHeight: 46
        clip: true

        gradient: Gradient {
            GradientStop { position: 0.0; color: Qt.lighter(Theme.metalTop, 1.18) }
            GradientStop { position: 0.5; color: Qt.lighter(Theme.metalMid, 1.06) }
            GradientStop { position: 1.0; color: Theme.metalBottom }
        }

        // The lit top edge, as on every other piece of chrome here.
        Rectangle {
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: Qt.rgba(1, 1, 1, 0.13)
        }

        Rectangle {
            anchors.bottom: parent.bottom
            anchors.left: parent.left
            anchors.right: parent.right
            height: 1
            color: Qt.rgba(0, 0, 0, 0.55)
        }

        Text {
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: 18
            text: frame.heading
            color: Theme.text
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSizeTitle
            font.weight: Font.DemiBold
            style: Text.Raised
            styleColor: Qt.rgba(0, 0, 0, 0.5)
        }
    }
}
