import QtQuick
import Earshot

// One line of text that, when it doesn't fit, glides sideways to show the
// rest (pausing at each end) instead of wrapping or being cut short. For the
// player bar and mini player, which shouldn't grow taller for long titles.
Item {
    id: marquee
    property alias text: label.text
    property alias color: label.color
    property alias font: label.font
    property color fadeColor: Theme.sidebar     // the background, for the soft edges
    readonly property real overflow: Math.max(0, label.implicitWidth - width)
    readonly property bool overflowing: overflow > 1

    implicitWidth: label.implicitWidth
    implicitHeight: label.implicitHeight
    clip: true

    Text {
        id: label
        y: 0
        color: Theme.text
        wrapMode: Text.NoWrap
        textFormat: Text.PlainText
    }

    SequentialAnimation {
        id: glide
        running: marquee.overflowing && marquee.visible
        loops: Animation.Infinite
        PauseAnimation { duration: 2500 }
        NumberAnimation {
            target: label; property: "x"; to: -marquee.overflow
            duration: Math.max(1500, marquee.overflow * 30); easing.type: Easing.InOutSine
        }
        PauseAnimation { duration: 2000 }
        NumberAnimation {
            target: label; property: "x"; to: 0
            duration: Math.max(700, marquee.overflow * 8); easing.type: Easing.InOutSine
        }
    }

    // Each new title starts from its beginning.
    onTextChanged: {
        label.x = 0
        if (glide.running)
            glide.restart()
    }
    onOverflowingChanged: if (!overflowing) label.x = 0

    // Soft edges where text runs out of view.
    Rectangle {
        visible: label.x < -1
        anchors.left: parent.left
        width: 16
        height: parent.height
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: marquee.fadeColor }
            GradientStop { position: 1; color: Qt.alpha(marquee.fadeColor, 0) }
        }
    }
    Rectangle {
        visible: marquee.overflowing && label.x > -marquee.overflow + 1
        anchors.right: parent.right
        width: 22
        height: parent.height
        gradient: Gradient {
            orientation: Gradient.Horizontal
            GradientStop { position: 0; color: Qt.alpha(marquee.fadeColor, 0) }
            GradientStop { position: 1; color: marquee.fadeColor }
        }
    }
}
