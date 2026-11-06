import QtQuick
import QtQuick.Controls.Basic
import Earshot

// A thin slider that thickens and shows its knob on hover. With `tip`, a
// small bubble above the knob shows it (the volume's percentage) while the
// slider is dragged or pointed at.
Slider {
    id: slider
    property string tip: ""
    hoverEnabled: true

    Rectangle {
        visible: slider.tip !== "" && (slider.pressed || slider.hovered)
        x: slider.leftPadding + slider.visualPosition * slider.availableWidth - width / 2
        y: -height - 6
        z: 10
        width: bubbleText.implicitWidth + 14
        height: bubbleText.implicitHeight + 8
        radius: 6
        color: Theme.surfaceRaised
        border.color: Theme.border
        Text {
            id: bubbleText
            anchors.centerIn: parent
            text: slider.tip
            color: Theme.text
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            font.features: { "tnum": 1 }
        }
    }
    focusPolicy: Qt.NoFocus
    implicitHeight: 16
    padding: 0

    background: Rectangle {
        x: slider.leftPadding
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: slider.availableWidth
        height: slider.hovered || slider.pressed ? 6 : 4
        radius: height / 2
        color: Qt.rgba(1, 1, 1, 0.14)
        Behavior on height { NumberAnimation { duration: 100 } }

        Rectangle {
            width: slider.visualPosition * parent.width
            height: parent.height
            radius: height / 2
            color: slider.hovered || slider.pressed ? Theme.accent : Theme.text
        }
    }

    handle: Rectangle {
        x: slider.leftPadding + slider.visualPosition * slider.availableWidth - width / 2
        y: slider.topPadding + slider.availableHeight / 2 - height / 2
        width: 14
        height: 14
        radius: 7
        color: "#ffffff"
        visible: slider.hovered || slider.pressed
    }
}
