import QtQuick
import QtQuick.Controls

// Publishes no named accessible object: a bare rectangle never appears in
// the tree, so only the window chrome remains. The sparse-tree E2E uses this
// to prove the warning surfaces instead of a silent 200.
ApplicationWindow {
    visible: true
    width: 480
    height: 320

    Rectangle {
        anchors.centerIn: parent
        width: 120
        height: 60
        color: "#ff8800"
    }
}
