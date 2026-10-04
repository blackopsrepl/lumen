import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Settings: start and stop the lumen-daemon.
//
// The daemon is a systemd user unit, controlled through D-Bus. This is the only
// place the user starts or stops it — there is no other entry point.
ColumnLayout {
    id: settings
    spacing: 8

    Text {
        text: "Daemon"
        color: Theme.text
        font.family: Theme.fontSans
        font.pixelSize: Theme.fontSize
        font.bold: true
    }

    RowLayout {
        Layout.fillWidth: true
        spacing: 6
        Button {
            text: daemonControl.active ? "Stop" : "Start"
            Layout.fillWidth: true
            onClicked: daemonControl.active ? daemonControl.stop() : daemonControl.start()
        }
        Button {
            text: daemonControl.enabled ? "Disable" : "Enable"
            Layout.fillWidth: true
            onClicked: daemonControl.setEnabled(!daemonControl.enabled)
        }
    }

    Text {
        text: daemonControl.active ? "Running" : "Stopped"
        color: daemonControl.active ? Theme.accent : Theme.muted
        font.family: Theme.fontSans
        font.pixelSize: Theme.fontSizeSmall
    }

    Text {
        text: daemonControl.enabled ? "Starts at login" : "Does not start at login"
        color: Theme.muted
        font.family: Theme.fontSans
        font.pixelSize: Theme.fontSizeSmall
    }

    Component.onCompleted: daemonControl.refresh()
}
