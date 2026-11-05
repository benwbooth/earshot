import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

AbstractButton {
    id: root
    property string iconName: ""
    property bool primary: false
    property bool checkedStyle: false

    implicitHeight: primary ? 44 : 36
    implicitWidth: row.implicitWidth + (primary ? 44 : 32)
    hoverEnabled: true
    focusPolicy: Qt.NoFocus

    background: Rectangle {
        radius: height / 2
        color: root.primary ? (root.hovered ? Theme.accentHover : Theme.accent)
             : root.checkedStyle ? Theme.text
             : (root.hovered ? Theme.surfaceHover : Theme.surfaceRaised)
        border.color: root.primary || root.checkedStyle ? "transparent" : Theme.border
        scale: root.pressed ? 0.97 : 1
        Behavior on scale { NumberAnimation { duration: 80 } }
    }

    contentItem: Item {
        RowLayout {
            id: row
            anchors.centerIn: parent
            spacing: 8
            Icon {
                visible: root.iconName !== ""
                name: root.iconName
                size: root.primary ? 18 : 16
                color: root.checkedStyle ? Theme.background : "#ffffff"
            }
            Text {
                text: root.text
                color: root.checkedStyle ? Theme.background : (root.primary ? "#ffffff" : Theme.text)
                font.pixelSize: root.primary ? Theme.fontBody + 1 : Theme.fontBody
                font.weight: Font.DemiBold
            }
        }
    }
}
