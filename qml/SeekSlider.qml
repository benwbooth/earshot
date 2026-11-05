import QtQuick
import QtQuick.Controls.Basic
import Earshot

// A thin slider that thickens and shows its knob on hover.
Slider {
    id: slider
    hoverEnabled: true
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
