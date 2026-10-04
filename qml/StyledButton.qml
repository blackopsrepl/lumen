import QtQuick
import QtQuick.Controls

// A button with this application's surface and label.
//
// Styling a button through `background:` alone is not enough: the control still
// draws its own `text` on top of whatever the background paints, which renders
// every label twice. Overriding `contentItem` here is what keeps the label in
// one place, and makes the surface and the text a single unit that is styled
// once instead of at each call site.
Button {
    id: control

    background: ButtonSurface { }

    contentItem: Text {
        text: control.text
        color: !control.enabled ? Theme.faint
               : control.highlighted ? Theme.accentInk
                                     : Theme.text
        font.family: Theme.fontSans
        font.pixelSize: Theme.fontSize
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        elide: Text.ElideRight
    }

    implicitHeight: Theme.controlHeight
}
