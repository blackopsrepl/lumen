import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Daemon settings.
//
// Reached from the gear in the toolbar rather than shown in the main window:
// the daemon is configured once, and the window belongs to the session being
// watched. The daemon is a systemd user unit, so this is where it is started,
// stopped, and set to come up at login.
Dialog {
    id: settingsDialog
    title: "Lumen settings"
    modal: true
    anchors.centerIn: parent

    function refresh() {
        daemonControl.refresh()
    }

    onOpened: refresh()

    background: Rectangle {
        color: Theme.panel
        radius: Theme.radius
        border.color: Theme.line
        border.width: 1
    }

    header: Rectangle {
        color: Theme.panel
        implicitHeight: 44
        Text {
            anchors.verticalCenter: parent.verticalCenter
            anchors.left: parent.left
            anchors.leftMargin: 16
            text: "Lumen settings"
            color: Theme.text
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSizeTitle
        }
        Rectangle {
            anchors.bottom: parent.bottom
            width: parent.width
            height: 1
            color: Theme.line
        }
    }

    // A ColumnLayout computes its implicit size from its children, which is what
    // a Dialog needs in order to size itself; a plain Column does not, and the
    // content is silently clipped.
    contentItem: ColumnLayout {
        spacing: 14

        Text {
            text: "Daemon"
            color: Theme.faint
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSizeSmall
            font.capitalization: Font.AllUppercase
            font.letterSpacing: 0.8
        }

        Text {
            Layout.preferredWidth: 380
            wrapMode: Text.WordWrap
            text: "The daemon hosts every session and keeps running when this window is "
                  + "closed. It is managed as a systemd user service."
            color: Theme.muted
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSize
        }

        RowLayout {
            spacing: 8
            Button {
                text: daemonControl.active ? "Stop daemon" : "Start daemon"
                onClicked: {
                    if (daemonControl.active) daemonControl.stop()
                    else daemonControl.start()
                }
            }
            Button {
                text: daemonControl.enabled ? "Disable at login" : "Enable at login"
                onClicked: daemonControl.setEnabled(!daemonControl.enabled)
            }
        }

        RowLayout {
            spacing: 8
            Rectangle {
                Layout.alignment: Qt.AlignVCenter
                implicitWidth: 8
                implicitHeight: 8
                radius: 4
                color: daemonControl.active ? Theme.accent : Theme.faint
            }
            Text {
                Layout.alignment: Qt.AlignVCenter
                text: (daemonControl.active ? "Running" : "Stopped")
                      + (daemonControl.enabled ? " · starts at login" : "")
                color: Theme.muted
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSize
            }
        }
    }

    footer: DialogButtonBox {
        Button {
            text: "Close"
            DialogButtonBox.buttonRole: DialogButtonBox.AcceptRole
        }
        onAccepted: settingsDialog.close()
    }
}
