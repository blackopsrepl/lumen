import QtQuick
import QtQuick.Controls

// Publishes no accessible object at all: a bare rectangle never enters the
// tree, and Qt's content filler spans the window exactly, so nothing is
// addressable. The window carries a title on purpose: a title names the
// window, not a target. The empty-tree E2E proves the service refuses this
// tree instead of serving a bare registry skeleton.
ApplicationWindow {
    visible: true
    width: 480
    height: 320
    title: "Sparse Fixture"

    Rectangle {
        anchors.centerIn: parent
        width: 120
        height: 60
        color: "#ff8800"
    }
}
