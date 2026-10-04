import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The toolbar: the controls for the session being watched.
//
// It carries no title. The session name is already in the sidebar's list and
// the frame below is its pixels, so naming it here a third time was noise — and
// a toolbar with nothing but icons on the right reads as chrome rather than as
// a second heading.
//
// Every control is an icon over the shared surface. The actions are few and each
// has an obvious symbol, so words here would only add weight; the label is
// carried by a tooltip. The row is separated into two groups — the fullscreen
// toggle stands alone, then the actions that operate on the session — by a
// hairline, which is what keeps a strip of icons legible.
Item {
    id: bar
    required property var activeSession
    required property bool humanControlling
    required property bool annotating
    /// The current view zoom, as a percentage, for the readout.
    required property int zoomPercent

    signal releaseControl()
    signal takeControl()
    signal fullscreenRequested()
    signal annotateToggled()
    signal settingsRequested()
    signal stopRequested()
    signal fitRequested()
    signal actualSizeRequested()

    implicitHeight: 48

    Rectangle {
        anchors.fill: parent
        clip: true

        // A soft two-stop chrome: enough to read as a surface, not enough to
        // compete with the frame below it.
        gradient: Gradient {
            GradientStop { position: 0.0; color: Theme.metalTop }
            GradientStop { position: 1.0; color: Theme.metalMid }
        }

        RowLayout {
            id: controls
            anchors.verticalCenter: parent.verticalCenter
            anchors.right: parent.right
            anchors.rightMargin: 14
            spacing: 4

            // The zoom readout. It is a button, not a label: clicking it returns
            // the view to actual size, which is how the old viewer behaved and
            // how every document viewer behaves.
            Button {
                id: zoomButton
                flat: true
                implicitWidth: Math.max(56, zoomText.implicitWidth + 20)
                background: ButtonSurface { }
                onClicked: bar.actualSizeRequested()
                contentItem: Text {
                    id: zoomText
                    text: bar.zoomPercent + "%"
                    color: zoomButton.hovered ? Theme.text : Theme.muted
                    font.family: Theme.fontMono
                    font.pixelSize: Theme.fontSizeSmall
                    horizontalAlignment: Text.AlignHCenter
                    verticalAlignment: Text.AlignVCenter
                }
                ToolTip.visible: hovered
                ToolTip.text: "Actual size (Ctrl+0)"
                ToolTip.delay: 500
            }

            IconButton {
                iconKind: "fit"
                tooltipText: "Fit to window (Ctrl+9)"
                onClicked: bar.fitRequested()
            }

            Rectangle {
                Layout.alignment: Qt.AlignVCenter
                Layout.leftMargin: 4
                Layout.rightMargin: 4
                implicitWidth: 1
                implicitHeight: 18
                color: Theme.line
            }

            IconButton {
                iconKind: "expand"
                tooltipText: "Full screen"
                onClicked: bar.fullscreenRequested()
            }

            Rectangle {
                Layout.alignment: Qt.AlignVCenter
                Layout.leftMargin: 4
                Layout.rightMargin: 4
                implicitWidth: 1
                implicitHeight: 18
                color: Theme.line
            }

            // The note path: draw a region on the frame, describe it, and it is
            // addressed to the session for the agent to pick up. Held down while
            // annotating, because the state matters more than the label here.
            IconButton {
                iconKind: "note"
                tooltipText: bar.annotating ? "Stop annotating" : "Leave a note"
                tone: bar.annotating
                onClicked: bar.annotateToggled()
            }

            IconButton {
                iconKind: bar.humanControlling ? "release" : "control"
                tooltipText: bar.humanControlling ? "Release control (Esc)"
                                                 : "Take control"
                enabled: !!bar.activeSession
                tone: bar.humanControlling
                onClicked: bar.humanControlling ? bar.releaseControl() : bar.takeControl()
            }

            // A session is started by an agent; ending the one being watched is
            // the only session-owning action that belongs to the human here.
            IconButton {
                iconKind: "stop"
                tooltipText: "Stop this session"
                enabled: !!bar.activeSession
                onClicked: bar.stopRequested()
            }

            Rectangle {
                Layout.alignment: Qt.AlignVCenter
                Layout.leftMargin: 4
                Layout.rightMargin: 4
                implicitWidth: 1
                implicitHeight: 18
                color: Theme.line
            }

            IconButton {
                iconKind: "gear"
                tooltipText: "Settings"
                onClicked: bar.settingsRequested()
            }
        }

        // Bottom hairline.
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }
}
