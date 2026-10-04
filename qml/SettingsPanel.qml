import QtQuick
import QtQuick.Controls

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
        height: 44
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

    contentItem: Column {
        spacing: 14
        // Explicit sizing: a Dialog does not lay out its content item for you,
        // so without padding and a width the content is clipped. The height is
        // left to the Column, which derives it from its children.
        leftPadding: 16
        rightPadding: 16
        topPadding: 16
        bottomPadding: 16
        width: 420

        Text {
            text: "Daemon"
            color: Theme.faint
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSizeSmall
            font.capitalization: Font.AllUppercase
            font.letterSpacing: 0.8
        }

        Text {
            text: "The daemon hosts every session and keeps running when this window is "
                  + "closed. It is managed as a systemd user service."
            width: 380
            wrapMode: Text.WordWrap
            color: Theme.muted
            font.family: Theme.fontSans
            font.pixelSize: Theme.fontSize
        }

        Row {
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

        Row {
            spacing: 8
            Rectangle {
                anchors.verticalCenter: parent.verticalCenter
                width: 8
                height: 8
                radius: 4
                color: daemonControl.active ? Theme.accent : Theme.faint
            }
            Text {
                anchors.verticalCenter: parent.verticalCenter
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
