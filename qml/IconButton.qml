import QtQuick
import QtQuick.Controls

// An icon-only button: the surface, with the glyph drawn as a path.
//
// A text label was the wrong choice for this toolbar. The actions are few and
// each has an obvious symbol, so a row of word-buttons reads as heavier than the
// window needs — and the two symbols that were set as text (⤢, ⚙) rendered as
// tofu boxes because they are not in the UI font. Every control here is now an
// icon over a shared surface, with the label carried by a tooltip instead.
Button {
    id: control

    /// "expand" | "gear" | "note" | "control" | "release" | "stop" | "close" |
    /// "check" | "power" | "login"
    required property string iconKind
    /// The label, carried on hover rather than written in the button.
    property string tooltipText: ""
    /// Larger icons (the commit action) can override; the toolbar uses the default.
    property real iconSize: 16
    /// An accent-tinted button, for the commit action in a dialog.
    property bool tone: false

    hoverEnabled: true
    implicitWidth: 34
    implicitHeight: Theme.controlHeight
    highlighted: control.tone

    background: ButtonSurface { }

    contentItem: Icon {
        kind: control.iconKind
        size: control.iconSize
        ink: !control.enabled ? Theme.faint
             : control.tone ? Theme.accentInk
             : Theme.text
        opacity: !control.enabled ? 0.55 : control.hovered ? 1.0 : 0.82

        Behavior on opacity {
            NumberAnimation { duration: 90 }
        }
    }

    ToolTip.visible: control.hovered && control.tooltipText !== ""
    ToolTip.text: control.tooltipText
    ToolTip.delay: 500
}
