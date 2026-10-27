import QtQuick
import QtQuick.Window

// A main window and a dialog that closes on its own: the shape of an
// application that opens a confirmation, a file picker or a second window.
//
// The main window must stay on screen after the dialog goes. A compositor that
// keeps one surface per session shows the dialog while it is up and then
// nothing at all — the application is running and its window is invisible.
Window {
    id: main
    visible: true
    width: 480
    height: 320
    title: "TWO-MAIN"
    color: "#203040"

    Window {
        id: dialog
        visible: true
        width: 200
        height: 100
        title: "TWO-DIALOG"
        color: "#a03030"
        Timer {
            interval: 1500
            running: true
            onTriggered: dialog.visible = false
        }
    }
}
