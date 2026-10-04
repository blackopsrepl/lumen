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

    // The key probe. A key delivered through the compositor's seat is observable
    // the same way a click is: it is written into the title. The handler lives
    // on an Item, because an attached Keys property only exists on one.
    Item {
        id: keySink
        anchors.fill: parent
        focus: true
        // forceActiveFocus, not focus: inside a Wayland-hosted window the item
        // only receives keys once the window itself is active, and setting the
        // property alone does not make it the window's focus item.
        Component.onCompleted: keySink.forceActiveFocus()
        property string typed: ""
        Keys.onPressed: (event) => {
            keySink.typed += event.text
            w.title = "SPIKE-FIXTURE-TYPED:" + keySink.typed
            event.accepted = true
        }
    }

    Text {
        anchors.centerIn: parent
        text: "SPIKE"
        color: "#e6e6e6"
        font.pixelSize: 48
    }
}
