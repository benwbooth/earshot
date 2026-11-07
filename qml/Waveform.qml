import QtQuick
import Earshot

// Six bars, like the now-playing indicator in a phone's notch, driven by the
// music actually coming out of the speakers (bass on the left, treble on the
// right). When nothing plays they fade out and give their space back.
Item {
    id: wave
    property bool active: Player.playing
    property bool shown: active
    property color color: Theme.accent
    property real barWidth: 3
    property real gap: 2
    readonly property int bars: 6

    implicitWidth: shown ? bars * barWidth + (bars - 1) * gap : 0
    implicitHeight: 16
    visible: implicitWidth > 0.5
    opacity: shown ? 1 : 0
    clip: true
    Behavior on implicitWidth { NumberAnimation { duration: 200; easing.type: Easing.OutCubic } }
    Behavior on opacity { NumberAnimation { duration: 200 } }

    Row {
        anchors.centerIn: parent
        spacing: wave.gap
        Repeater {
            model: wave.bars
            delegate: Rectangle {
                required property int index
                readonly property real level: wave.active ? (Player.levels[index] || 0) : 0
                anchors.verticalCenter: parent ? parent.verticalCenter : undefined
                width: wave.barWidth
                height: Math.max(wave.barWidth, wave.height * (0.15 + 0.85 * level))
                radius: width / 2
                color: wave.color
                Behavior on height { NumberAnimation { duration: 60; easing.type: Easing.OutQuad } }
            }
        }
    }
}
