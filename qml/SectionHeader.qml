import QtQuick
import QtQuick.Layouts
import Earshot

RowLayout {
    id: header
    property string title
    property string action: ""
    signal actionClicked()

    Layout.fillWidth: true
    Layout.topMargin: 12
    spacing: 12

    Text {
        Layout.fillWidth: true
        text: header.title
        color: Theme.text
        font.pixelSize: Theme.fontTitle
        font.weight: Font.Bold
        elide: Text.ElideRight
    }
    Text {
        visible: header.action !== ""
        text: header.action
        color: mouse.containsMouse ? Theme.text : Theme.textMuted
        font.pixelSize: Theme.fontSmall + 1
        font.weight: Font.DemiBold
        MouseArea {
            id: mouse
            anchors.fill: parent
            hoverEnabled: true
            cursorShape: Qt.PointingHandCursor
            onClicked: header.actionClicked()
        }
    }
}
