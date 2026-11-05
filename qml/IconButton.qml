import QtQuick
import QtQuick.Controls.Basic
import Earshot

AbstractButton {
    id: root
    property string iconName
    property int iconSize: 20
    property color color: Theme.textMuted
    property color hoverColor: Theme.text
    property bool active: false
    property bool filled: false
    property string tip: ""

    implicitWidth: filled ? iconSize * 2 + 8 : iconSize + 16
    implicitHeight: implicitWidth
    hoverEnabled: true
    focusPolicy: Qt.NoFocus

    ToolTip.visible: tip !== "" && hovered
    ToolTip.text: tip
    ToolTip.delay: 600

    background: Rectangle {
        radius: width / 2
        color: root.filled ? (root.hovered ? Theme.accentHover : Theme.accent)
                           : (root.hovered ? Qt.rgba(1, 1, 1, 0.08) : "transparent")
        scale: root.pressed ? 0.94 : 1
        Behavior on scale { NumberAnimation { duration: 80 } }
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    contentItem: Item {
        Icon {
            anchors.centerIn: parent
            name: root.iconName
            size: root.iconSize
            color: root.filled ? "#ffffff" : root.active ? Theme.accent : (root.hovered ? root.hoverColor : root.color)
            opacity: root.enabled ? 1 : 0.35
        }
        Rectangle {
            visible: root.active && !root.filled
            width: 4; height: 4; radius: 2
            color: Theme.accent
            anchors.horizontalCenter: parent.horizontalCenter
            anchors.bottom: parent.bottom
            anchors.bottomMargin: 1
        }
    }
}
