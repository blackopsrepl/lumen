import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    color: "#161719"
    anchors {
        top: true
        right: true
        bottom: true
        left: true
    }

    TextInput {
        anchors.centerIn: parent
        width: 520
        color: "#e8e6e1"
        font.pixelSize: 28
        focus: true
        text: "Lumen Quickshell"
        selectByMouse: true
        Component.onCompleted: selectAll()
    }
}
