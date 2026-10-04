import QtQuick

// The surface of a button.
//
// Restraint is the point: a soft two-stop gradient a few percent apart, one
// hairline border, and a clear change on hover and press. No bright rim, no
// strong highlight — a control should sit in the window, not compete with the
// session's frame.
//
// Used as `Button { background: ButtonSurface { } }`. It reads `control`, the
// AbstractButton it belongs to, because a background item is not reparented to
// the button and cannot reach it any other way.
Rectangle {
    id: surface

    // Qt reparents a control's background to the control itself, so this is how
    // the surface reaches the button it belongs to. A `required property var
    // control` does not work: nothing assigns it, which leaves every state test
    // reading an undefined object.
    readonly property var control: parent

    readonly property bool accented: control && control.highlighted
    readonly property bool down: control && (control.pressed || control.down)

    implicitWidth: control && control.implicitContentWidth > 0
                   ? control.implicitContentWidth + 24
                   : 30
    implicitHeight: Theme.controlHeight
    radius: Theme.radius
    border.width: 1
    clip: true

    border.color: !control.enabled ? Theme.lineSoft
                  : control.visualFocus ? Theme.accent
                  : surface.down ? Theme.metalEdge
                  : control.hovered ? Theme.metalEdge
                  : accented ? Theme.accent
                  : Theme.line

    gradient: Gradient {
        GradientStop {
            position: 0.0
            color: !control.enabled ? Theme.panel
                   : accented ? (surface.down ? Qt.darker(Theme.accent, 1.12)
                                              : Qt.lighter(Theme.accent, 1.04))
                   : surface.down ? Theme.metalTopActive
                   : control.hovered ? Qt.lighter(Theme.metalTop, 1.10)
                   : control.flat ? "transparent" : Theme.metalTop
        }
        GradientStop {
            position: 1.0
            color: !control.enabled ? Theme.panel
                   : accented ? (surface.down ? Qt.darker(Theme.accent, 1.22)
                                              : Qt.darker(Theme.accent, 1.06))
                   : surface.down ? Theme.metalBottomActive
                   : control.hovered ? Qt.lighter(Theme.metalBottom, 1.14)
                   : control.flat ? "transparent" : Theme.metalBottom
        }
    }
}
