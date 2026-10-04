import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Daemon settings.
//
// Reached from the gear in the toolbar rather than shown in the main window: the
// daemon is configured once, and the window belongs to the session being watched.
// The daemon is a systemd user unit, so this is where it is started, stopped and
// set to come up at login.
//
// The body is a ColumnLayout so the Dialog can compute an implicit size from its
// children — a plain Column reports none and the content is silently clipped.
Dialog {
    id: settingsDialog
    title: "Lumen settings"
    modal: true
    anchors.centerIn: parent
    padding: 0

    // An explicit width, so the dialog does not ask its contentItem for an
    // implicit size while that contentItem is itself filling the dialog — which
    // is a binding loop, and one Qt reports on every open.
    implicitWidth: 452

    function refresh() {
        daemonControl.refresh()
    }

    onOpened: refresh()

    background: DialogSurface { }

    contentItem: DialogFrame {
        heading: "Lumen settings"

        ColumnLayout {
            Layout.margins: 18
            spacing: 16

            Text {
                text: "Daemon"
                color: Theme.faint
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSizeSmall
                font.capitalization: Font.AllUppercase
                font.letterSpacing: 0.8
            }

            Text {
                Layout.preferredWidth: 384
                wrapMode: Text.WordWrap
                text: "The daemon hosts every session and keeps running when this window "
                      + "is closed. It is managed as a systemd user service."
                color: Theme.muted
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSize
                lineHeight: 1.35
            }

            // The daemon's state, as one line: a dot and what it means.
            RowLayout {
                spacing: 8
                StatusDot {
                    Layout.alignment: Qt.AlignVCenter
                    tone: daemonControl.active ? Theme.accent : Theme.faint
                }
                Text {
                    Layout.alignment: Qt.AlignVCenter
                    text: (daemonControl.active ? "Running" : "Stopped")
                          + (daemonControl.enabled ? " · starts at login" : "")
                    color: Theme.text
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.fontSize
                }
            }

            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.lineSoft }

            RowLayout {
                spacing: 8

                IconButton {
                    iconKind: "power"
                    tooltipText: daemonControl.active ? "Stop the daemon" : "Start the daemon"
                    tone: daemonControl.active
                    onClicked: {
                        if (daemonControl.active) daemonControl.stop()
                        else daemonControl.start()
                    }
                }
                IconButton {
                    iconKind: "login"
                    tooltipText: daemonControl.enabled ? "Do not start at login"
                                                      : "Start at login"
                    tone: daemonControl.enabled
                    onClicked: daemonControl.setEnabled(!daemonControl.enabled)
                }
                Item { Layout.fillWidth: true }
            }

            Rectangle { Layout.fillWidth: true; implicitHeight: 1; color: Theme.lineSoft }

            RowLayout {
                spacing: 8
                Item { Layout.fillWidth: true }
                IconButton {
                    iconKind: "close"
                    tooltipText: "Close settings"
                    onClicked: settingsDialog.close()
                }
            }
        }
    }
}
