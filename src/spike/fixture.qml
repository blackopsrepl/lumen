import QtQuick
import QtQuick.Window

// The session application for the spike: an ordinary Wayland client that knows
// nothing about Lumen. A click anywhere in it changes its window title, which
// is how the harness proves that synthesized input actually reached the client
// — the title is observed back through the compositor's toplevel.
Window {
    id: w
    visible: true
    width: 480
    height: 320
    title: "SPIKE-FIXTURE"
    color: "#203040"

    MouseArea {
        anchors.fill: parent
        onClicked: w.title = "SPIKE-FIXTURE-CLICKED"
    }

    Text {
        anchors.centerIn: parent
        text: "SPIKE"
        color: "#e6e6e6"
        font.pixelSize: 48
    }
}
