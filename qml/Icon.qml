import QtQuick
import QtQuick.Shapes

// The application's icons, drawn as vector paths.
//
// Font glyphs were the wrong choice: ⤢ and ⚙ are not in the UI font, and Qt
// falls back to a tofu box — a visible square where an icon should be. These are
// paths, so they are sharp at any size and take the palette's ink directly.
//
// Each icon is a line drawing on a 16x16 grid, stroked at a hairline weight with
// round caps, which is what makes a set of small icons read as one family rather
// than as a pile of clip art. `size` defaults to the toolbar's 15px but any icon
// can be drawn larger, because the paths scale.
Item {
    id: icon

    /// "expand" | "gear" | "note" | "control" | "release" | "stop" | "close" | "check"
    property string kind: "gear"
    property color ink: Theme.text
    property real size: 16
    /// Hairline weight; scaled with the grid so a larger icon keeps its
    /// proportion rather than looking thin.
    readonly property real weight: size * 0.085

    implicitWidth: size
    implicitHeight: size

    // --- the icon bodies -----------------------------------------------------
    // Each is hidden unless selected. A ShapePath has no `visible` of its own,
    // so the switch is at the Shape level.

    Shape {
        anchors.fill: parent
        visible: icon.kind === "expand"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: 0; startY: 0
            PathSvg { path: "M6.2 1.6H1.6V6.2 M9.8 14.4H14.4V9.8 M14.4 1.6L9.6 6.4 M1.6 14.4L6.4 9.6" }
        }
    }

    Shape {
        anchors.fill: parent
        visible: icon.kind === "gear"
        antialiasing: true
        // The ring is deliberately larger than the teeth are long: a gear whose
        // spokes reach the full grid reads as a sun, which is the wrong icon.
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight * 1.3
            fillColor: "transparent"
            startX: 0; startY: 0
            PathSvg { path: "M8 4.4A3.6 3.6 0 1 1 8 11.6A3.6 3.6 0 1 1 8 4.4" }
        }
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight * 1.35
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            startX: 0; startY: 0
            PathSvg { path: "M8 1.7V3.1 M8 12.9V14.3 M1.7 8H3.1 M12.9 8H14.3 M3.54 3.54L4.53 4.53 M11.47 11.47L12.46 12.46 M12.46 3.54L11.47 4.53 M4.53 11.47L3.54 12.46" }
        }
    }

    // A note: a speech bubble with the tail at the lower left, so it reads as
    // something spoken to the agent rather than as a page or a bookmark.
    Shape {
        anchors.fill: parent
        visible: icon.kind === "note"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: 0; startY: 0
            PathSvg { path: "M5 3.4H12.6A2.1 2.1 0 0 1 14.7 5.5V10A2.1 2.1 0 0 1 12.6 12.1H9.4L5.6 14.6V12.1H5A2.1 2.1 0 0 1 2.9 10V5.5A2.1 2.1 0 0 1 5 3.4Z" }
        }
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight * 0.9
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            startX: 0; startY: 0
            PathSvg { path: "M5.4 6.4H12.2 M5.4 8.9H10.2" }
        }
    }

    // Take control: an open hand over a surface — simplified to a pointer
    // entering a frame, which reads at 15px where a hand does not.
    Shape {
        anchors.fill: parent
        visible: icon.kind === "control"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: 0; startY: 0
            PathSvg { path: "M1.6 1.6H10.4 M1.6 1.6V10.4 M8.3 8.3L14.4 11L11.6 12.1L10.5 14.9L8.3 8.3" }
        }
    }

    // Release: the same pointer leaving.
    Shape {
        anchors.fill: parent
        visible: icon.kind === "release"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: 0; startY: 0
            PathSvg { path: "M8 8L14.4 11L11.6 12.1L10.5 14.9L8 8 M1.7 4.6H4.3 M1.7 8H4.3 M1.7 11.4H4.3" }
        }
    }

    Shape {
        anchors.fill: parent
        visible: icon.kind === "stop"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            startX: 0; startY: 0
            PathSvg { path: "M4.6 4.6H11.4V11.4H4.6Z" }
        }
    }

    Shape {
        anchors.fill: parent
        visible: icon.kind === "close"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight * 1.2
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            startX: 0; startY: 0
            PathSvg { path: "M3.6 3.6L12.4 12.4 M12.4 3.6L3.6 12.4" }
        }
    }

    /// Power: the daemon's start/stop. A ring with a gap, plus the stem.
    Shape {
        anchors.fill: parent
        visible: icon.kind === "power"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            startX: 0; startY: 0
            PathSvg { path: "M4.4 4.1A5 5 0 1 0 11.6 4.1 M8 1.4V7.6" }
        }
    }

    /// Login: an arrow entering a door, for enable-at-login.
    Shape {
        anchors.fill: parent
        visible: icon.kind === "login"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: 0; startY: 0
            PathSvg { path: "M6.2 2.4H3.2A1.4 1.4 0 0 0 1.8 3.8V12.2A1.4 1.4 0 0 0 3.2 13.6H6.2 M10.4 5.4L13.4 8L10.4 10.6 M13.4 8H6.2" }
        }
    }

    Shape {
        anchors.fill: parent
        visible: icon.kind === "check"
        antialiasing: true
        ShapePath {
            strokeColor: icon.ink
            strokeWidth: icon.weight * 1.3
            fillColor: "transparent"
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            startX: 0; startY: 0
            PathSvg { path: "M2.6 8.4L6.2 12L13.4 4.4" }
        }
    }
}
