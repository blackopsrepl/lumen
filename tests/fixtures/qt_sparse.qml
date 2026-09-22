import QtQuick
import QtQuick.Controls

// Publishes no named accessible object: a bare rectangle never appears in
// the tree, so only the window chrome remains. The window carries a title on
// purpose: a title names the window, not a target, so the tree must still
// measure as sparse. The sparse-tree E2E proves the warning surfaces instead
// of a silent 200.
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
