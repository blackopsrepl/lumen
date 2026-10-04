import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// The session list, in the viewer's grouping: agent-owned sessions first, then
// sessions no agent is attached to. A note left on the latter stays unread, so
// the split is a warning, not decoration.
Item {
    id: sidebar
    required property var sessions
    required property string activeName
    signal sessionActivated(string name)

    function group(list, agentOwned) {
        return list.filter(s => s.agentOwned === agentOwned)
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        // --- brand ---
        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 14
            spacing: 8
            Text {
                text: "lumen"
                color: Theme.text
                font.family: Theme.fontMono
                font.pixelSize: Theme.fontSizeTitle
                font.letterSpacing: 1
                Layout.fillWidth: true
            }
            Text {
                text: sidebar.sessions.length + " active"
                color: Theme.faint
                font.family: Theme.fontMono
                font.pixelSize: Theme.fontSizeSmall
            }
        }

        Rectangle { Layout.fillWidth: true; height: 1; color: Theme.lineSoft }

        // --- session list ---
        ListView {
            id: list
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: {
                const agents = sidebar.group(sidebar.sessions, true)
                const manual = sidebar.group(sidebar.sessions, false)
                const rows = []
                if (agents.length && manual.length) rows.push({ group: "Agent sessions · " + agents.length })
                for (const s of agents) rows.push(s)
                if (agents.length && manual.length) rows.push({ group: "Manual · no agent · " + manual.length })
                for (const s of manual) rows.push(s)
                return rows
            }
            delegate: Item {
                id: row
                required property var modelData
                width: list.width
                height: row.modelData.group ? 26 : 52

                Text {
                    visible: !!row.modelData.group
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.left: parent.left
                    anchors.leftMargin: 14
                    text: row.modelData.group || ""
                    color: Theme.faint
                    font.family: Theme.fontSans
                    font.pixelSize: Theme.fontSizeSmall
                    font.capitalization: Font.AllUppercase
                    font.letterSpacing: 0.8
                }

                Rectangle {
                    visible: !row.modelData.group
                    anchors.fill: parent
                    color: sidebar.activeName === row.modelData.name ? Theme.raised
                         : mouse.containsMouse ? Theme.panel2 : "transparent"

                    Column {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.verticalCenter: parent.verticalCenter
                        anchors.leftMargin: 14
                        anchors.rightMargin: 14
                        spacing: 3

                        Row {
                            spacing: 6
                            Text {
                                text: row.modelData.name
                                color: Theme.text
                                font.family: Theme.fontSans
                                font.pixelSize: Theme.fontSize
                                elide: Text.ElideRight
                            }
                            Rectangle {
                                width: chipText.width + 12
                                height: 16
                                radius: 8
                                color: row.modelData.agentOwned ? Theme.accent : Theme.raised
                                Text {
                                    id: chipText
                                    anchors.centerIn: parent
                                    text: row.modelData.agentOwned
                                          ? (row.modelData.owner || "agent") : "no agent"
                                    color: row.modelData.agentOwned ? Theme.accentInk : Theme.muted
                                    font.family: Theme.fontSans
                                    font.pixelSize: 10
                                }
                            }
                        }
                        Text {
                            width: parent.width
                            text: row.modelData.command
                            color: Theme.faint
                            font.family: Theme.fontMono
                            font.pixelSize: Theme.fontSizeSmall
                            elide: Text.ElideMiddle
                        }
                    }

                    MouseArea {
                        id: mouse
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: sidebar.sessionActivated(row.modelData.name)
                    }
                }
            }

            Text {
                visible: list.count === 0
                anchors.centerIn: parent
                text: "No sessions yet"
                color: Theme.faint
                font.family: Theme.fontSans
                font.pixelSize: Theme.fontSize
            }
        }
    }
}
