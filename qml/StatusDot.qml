import QtQuick
import QtQuick.Controls

// A small round status dot. `tone` is the colour; the radius makes it circular
// regardless of size.
Rectangle {
    property color tone: Theme.faint

    width: 8
    height: 8
    radius: width / 2
    color: tone
}
