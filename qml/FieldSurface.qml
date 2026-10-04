import QtQuick

// The surface of a text field: a well pressed into the chrome.
//
// A field of this material is the inverse of a button — the gradient runs dark
// at the top to lighter at the bottom, with the seam along the top edge, so it
// reads as recessed rather than raised. Used as
// `TextField { background: FieldSurface { } }`.
Rectangle {
    id: surface

    // The field this surface belongs to; a control's background is reparented to
    // the control, so `parent` is it.
    readonly property var control: parent

    implicitWidth: 180
    implicitHeight: Theme.controlHeight
    radius: Theme.radius
    border.width: 1
    border.color: control && control.activeFocus ? Theme.accent
                  : control && control.hovered ? Theme.metalEdge
                                               : Theme.lineSoft
    clip: true

    color: Theme.wellTop

    // The recessed seam, on the top edge where the light would fall.
    Rectangle {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: 1
        color: Qt.rgba(0, 0, 0, 0.55)
    }

    Behavior on border.color {
        ColorAnimation { duration: 90 }
    }
}
