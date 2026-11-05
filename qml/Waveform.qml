import QtQuick
import Earshot

// Six bars, like the now-playing indicator in a phone's notch, driven by the
// music actually coming out of the speakers (bass on the left, treble on the
// right). They settle into dots when nothing plays.
Item {
    id: wave
    property bool active: Player.playing
    property color color: Theme.accent
    property real barWidth: 3
    property real gap: 2
    readonly property int bars: 6

    implicitWidth: bars * barWidth + (bars - 1) * gap
    implicitHeight: 16

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
