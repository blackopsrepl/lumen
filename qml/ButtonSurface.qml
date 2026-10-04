import QtQuick

// The surface of a button.
//
// Three things make a small control read as a made object rather than a painted
// rectangle, and this has all three: a lit top edge and a shaded bottom edge
// (so the surface has thickness), a soft gradient between them, and a state that
// changes on hover *and* on press. Restraint still governs — the tones stay
// within the palette's own neutrals — but a control with no highlight at all
// just looks like an unfinished rectangle, which is what this replaced.
//
// Used as `Button { background: ButtonSurface { } }`. It reads `control`, the
// AbstractButton it belongs to: Qt reparents a background to its control, so
// `parent` is how it gets there.
Rectangle {
    id: surface

    readonly property var control: parent
    readonly property bool accented: control && control.highlighted
    readonly property bool down: control && (control.pressed || control.down)
    readonly property bool lit: control && control.enabled
                               && (control.hovered || surface.down)

    implicitWidth: control && control.implicitContentWidth > 0
                   ? control.implicitContentWidth + 24
                   : 30
    implicitHeight: Theme.controlHeight
    radius: Theme.radius
    border.width: 1
    clip: true

    border.color: !control.enabled ? Theme.lineSoft
                  : control.visualFocus ? Theme.accent
                  : accented ? (surface.down ? Qt.darker(Theme.accent, 1.2) : Theme.accent)
                  : surface.down ? Theme.metalEdge
                  : control.hovered ? Theme.metalEdge
                  : Theme.line

    gradient: Gradient {
        GradientStop {
            position: 0.0
            color: !control.enabled ? Theme.panel
                   : accented ? (surface.down ? Qt.darker(Theme.accent, 1.16)
                                              : Qt.lighter(Theme.accent, 1.10))
                   : surface.down ? Theme.metalTopActive
                   : control.hovered ? Qt.lighter(Theme.metalTop, 1.22)
                   : control.flat ? "transparent" : Theme.metalTop
        }
        GradientStop {
            position: 0.55
            color: !control.enabled ? Theme.panel
                   : accented ? (surface.down ? Qt.darker(Theme.accent, 1.22) : Theme.accent)
                   : surface.down ? Theme.metalTopActive
                   : control.hovered ? Qt.lighter(Theme.metalMid, 1.16)
                   : control.flat ? "transparent" : Theme.metalMid
        }
        GradientStop {
            position: 1.0
            color: !control.enabled ? Theme.panel
                   : accented ? (surface.down ? Qt.darker(Theme.accent, 1.3)
                                              : Qt.darker(Theme.accent, 1.08))
                   : surface.down ? Theme.metalBottomActive
                   : control.hovered ? Qt.lighter(Theme.metalBottom, 1.3)
                   : control.flat ? "transparent" : Theme.metalBottom
        }
    }

    // The lit top edge. A single hairline of near-white is what gives the
    // surface thickness; without it every button reads as flat.
    Rectangle {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: 1
        color: Qt.rgba(1, 1, 1, !surface.lit ? 0.0
                              : surface.accented ? 0.22
                              : surface.down ? 0.04 : 0.13)
        Behavior on color { ColorAnimation { duration: 110 } }
    }

    // A soft glow underneath the accent buttons, so the commit action reads as
    // the live one. Drawn as a faded ring rather than a shadow, which is what
    // keeps it from looking like a drop shadow on a dark surface.
    Rectangle {
        anchors.fill: parent
        anchors.margins: -2
        radius: parent.radius + 2
        color: "transparent"
        border.width: 3
        border.color: Theme.accent
        opacity: surface.accented && !surface.down ? 0.20 : 0.0
        Behavior on opacity { NumberAnimation { duration: 110 } }
    }

    Behavior on border.color { ColorAnimation { duration: 110 } }
}
