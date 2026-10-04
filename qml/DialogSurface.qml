import QtQuick

// The surface behind a dialog: a brushed-metal panel, lit from the top, with a
// brighter rim than a button so it reads as the frontmost layer.
Rectangle {
    id: surface

    radius: Theme.radiusLarge
    border.color: Qt.rgba(1, 1, 1, 0.14)
    border.width: 1
    clip: true

    gradient: Gradient {
        GradientStop { position: 0.0; color: Qt.lighter(Theme.metalTop, 1.05) }
        GradientStop { position: 0.06; color: Theme.metalTop }
        GradientStop { position: 0.5; color: Theme.metalMid }
        GradientStop { position: 1.0; color: Theme.metalBottom }
    }

    // Outer seam, darker than the rim, so the panel has a defined edge.
    Rectangle {
        anchors.fill: parent
        radius: parent.radius
        color: "transparent"
        border.color: Qt.rgba(0, 0, 0, 0.35)
        border.width: 1
    }
}
