import QtQuick
import QtQuick.Window

// The session application for the spike: an ordinary Wayland client that knows
// nothing about Lumen.
//
// The title is the probe. It changes on hover, press, release and click, so the
// compositor can tell exactly how far a synthesized input event travelled: if a
// click does not produce the CLICKED title, the title it does show says whether
// the pointer entered the client and whether the press arrived.
Window {
    id: w
    visible: true
    width: 480
    height: 320
    title: "SPIKE-FIXTURE"
    color: "#203040"

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onEntered: if (w.title === "SPIKE-FIXTURE") w.title = "SPIKE-FIXTURE-ENTERED"
        onPressed: w.title = "SPIKE-FIXTURE-PRESSED"
        onReleased: w.title = "SPIKE-FIXTURE-RELEASED"
        onClicked: w.title = "SPIKE-FIXTURE-CLICKED"
    }

    Text {
        anchors.centerIn: parent
        text: "SPIKE"
        color: "#e6e6e6"
        font.pixelSize: 48
    }
}
