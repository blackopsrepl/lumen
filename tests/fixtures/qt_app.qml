import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

ApplicationWindow {
    id: window
    visible: true
    width: 480
    height: 320
    title: "Lumen Qt Fixture"
    property int count: 0

    ColumnLayout {
        anchors.centerIn: parent
        spacing: 12

        Label {
            objectName: "counter"
            text: window.count.toString()
            Accessible.name: text
        }

        TextField {
            objectName: "nameField"
            placeholderText: "Name"
            Accessible.name: "Name field"
        }

        Button {
            objectName: "increment"
            text: "Increment"
            Accessible.name: "Increment"
            onClicked: window.count += 1
        }
    }
}
