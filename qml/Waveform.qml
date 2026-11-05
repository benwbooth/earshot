import QtQuick
import Earshot

// Six bouncing bars, like the now-playing indicator on a phone's lock screen
// or notch. They dance while music plays and settle into dots when paused.
Item {
    id: wave
    property bool active: Player.playing
    property color color: Theme.accent
    property int bars: 6
    property real barWidth: 3
    property real gap: 2

    implicitWidth: bars * barWidth + (bars - 1) * gap
    implicitHeight: 16

    // Each bar has its own rhythm so the motion never looks mechanical.
    readonly property var speeds: [0.9, 1.35, 1.1, 1.6, 1.2, 0.8]
    property real phase: 0

    FrameAnimation {
        running: wave.active && wave.visible
        onTriggered: wave.phase += frameTime * 6
    }

    Row {
        anchors.centerIn: parent
        spacing: wave.gap
        Repeater {
            model: wave.bars
            delegate: Rectangle {
                required property int index
                readonly property real level: {
                    if (!wave.active)
                        return 0
                    const s = wave.speeds[index % wave.speeds.length]
                    const t = wave.phase * s + index * 1.7
                    // Two mixed waves give an uneven, music-like bounce.
                    return Math.max(0, Math.min(1, 0.5 + 0.32 * Math.sin(t) + 0.22 * Math.sin(t * 2.3 + index)))
                }
                width: wave.barWidth
                height: Math.max(wave.barWidth, wave.height * (0.2 + 0.8 * level))
                radius: width / 2
                color: wave.color
                anchors.verticalCenter: parent ? parent.verticalCenter : undefined
                Behavior on height {
                    enabled: !wave.active
                    NumberAnimation { duration: 220; easing.type: Easing.OutCubic }
                }
            }
        }
    }
}
