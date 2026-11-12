import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

AbstractButton {
    id: root
    property string iconName: ""
    property bool primary: false
    property bool checkedStyle: false
    // Rolls the icon like a die while something is under way.
    property bool rolling: false

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
                id: icon
                visible: root.iconName !== ""
                name: root.iconName
                size: root.primary ? 18 : 16
                color: root.checkedStyle ? Theme.background : "#ffffff"

                // A roll: a quarter turn with a hop, over and over; it settles upright when done.
                transform: [
                    Rotation { id: turn; origin.x: icon.width / 2; origin.y: icon.height / 2 },
                    Translate { id: hop }
                ]
                SequentialAnimation {
                    running: root.rolling
                    loops: Animation.Infinite
                    onRunningChanged: if (!running) { turn.angle = 0; hop.y = 0 }
                    ParallelAnimation {
                        NumberAnimation { target: turn; property: "angle"; from: 0; to: 90; duration: 260; easing.type: Easing.InOutQuad }
                        SequentialAnimation {
                            NumberAnimation { target: hop; property: "y"; from: 0; to: -4; duration: 130; easing.type: Easing.OutQuad }
                            NumberAnimation { target: hop; property: "y"; from: -4; to: 0; duration: 130; easing.type: Easing.InQuad }
                        }
                    }
                    PauseAnimation { duration: 40 }
                }
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
