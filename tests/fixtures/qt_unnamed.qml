import QtQuick
import QtQuick.Controls

// Publishes a real control that carries no name: an icon-only button appears
// in the tree with its own rectangle, but neither its text nor Accessible.name
// names it. The tree is addressable by reference, so the service must serve it
// with a warning rather than refuse it.
ApplicationWindow {
    visible: true
    width: 480
    height: 320
    title: "Unnamed Fixture"

    Button {
        anchors.centerIn: parent
        display: AbstractButton.IconOnly
    }
}
