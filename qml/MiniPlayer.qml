import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtCore
import Earshot

// The compact player: artwork, song, controls and progress in a small
// window that stays on top. Drag it anywhere; the arrows bring back the full window.
Window {
    id: mini
    signal expandRequested()

    width: 420
    height: Math.max(112, miniText.implicitHeight + 28)
    minimumWidth: 320
    minimumHeight: 112
    flags: Qt.Window | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint
    color: "transparent"
    title: Player.current ? Player.current.title + " — Earshot" : "Earshot"

    Settings {
        category: "mini"
        property alias x: mini.x
        property alias y: mini.y
        property alias width: mini.width
    }

    readonly property var track: Player.current

    Rectangle {
        id: card
        anchors.fill: parent
        radius: 14
        color: Theme.surface
        border.color: Theme.border
        clip: true

        DragHandler {
            target: null
            onActiveChanged: if (active) mini.startSystemMove()
        }
        TapHandler {
            onDoubleTapped: mini.expandRequested()
        }
        HoverHandler { id: hover }

        // Faint artwork wash behind everything
        Cover {
            anchors.fill: parent
            radius: 14
            source: mini.track ? (mini.track.cover || "") : ""
            seed: mini.track ? mini.track.key : ""
            opacity: 0.12
        }

        RowLayout {
            anchors.fill: parent
            anchors.margins: 10
            anchors.bottomMargin: 14
            spacing: 12

            Cover {
                Layout.preferredWidth: 84
                Layout.preferredHeight: 84
                radius: 10
                source: mini.track ? (mini.track.cover || "") : ""
                seed: mini.track ? mini.track.key : ""
            }

            ColumnLayout {
                id: miniText
                Layout.fillWidth: true
                spacing: 2
                Text {
                    Layout.fillWidth: true
                    text: mini.track ? mini.track.title : "Nothing playing"
                    color: Theme.text
                    font.pixelSize: Theme.fontBody + 1
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                }
                Text {
                    Layout.fillWidth: true
                    text: mini.track ? mini.track.artist : "Earshot"
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall + 1
                    wrapMode: Text.Wrap
                }
                RowLayout {
                    spacing: 2
                    Layout.topMargin: 4
                    IconButton { iconName: "previous"; iconSize: 16; color: Theme.text; enabled: mini.track !== null; onClicked: Player.previous() }
                    Item {
                        implicitWidth: 38
                        implicitHeight: 38
                        IconButton {
                            id: playButton
                            anchors.fill: parent
                            visible: !Player.loading
                            iconName: Player.playing ? "pause" : "play"
                            enabled: mini.track !== null
                            onClicked: Player.toggle()
                            background: Rectangle { radius: width / 2; color: "#ffffff"; opacity: playButton.enabled ? 1 : 0.4 }
                            contentItem: Item {
                                Icon { anchors.centerIn: parent; name: playButton.iconName; size: 16; color: Theme.background }
                            }
                        }
                        BusyIndicator { anchors.fill: parent; visible: Player.loading; running: visible }
                    }
                    IconButton { iconName: "next"; iconSize: 16; color: Theme.text; enabled: Player.hasNext; onClicked: Player.next(false) }
                    Item { Layout.fillWidth: true }
                    Text {
                        text: Theme.duration(Player.position) + " / " + (Player.length > 0 ? Theme.duration(Player.length) : "-:--")
                        color: Theme.textFaint
                        font.pixelSize: Theme.fontSmall - 1
                        font.features: { "tnum": 1 }
                    }
                }
            }
        }

        // Expand and close buttons appear on hover
        Row {
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.margins: 4
            opacity: hover.hovered ? 1 : 0
            Behavior on opacity { NumberAnimation { duration: 120 } }
            IconButton { iconName: "expand"; iconSize: 14; tip: "Full window"; onClicked: mini.expandRequested() }
            IconButton { iconName: "close"; iconSize: 14; tip: "Hide to tray"; onClicked: mini.hide() }
        }

        // Progress along the bottom edge; click or drag to seek
        SeekSlider {
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.bottom: parent.bottom
            anchors.leftMargin: 10
            anchors.rightMargin: 10
            anchors.bottomMargin: 2
            from: 0
            to: Math.max(1, Player.length)
            enabled: mini.track !== null && Player.length > 0
            value: pressed ? value : Player.position
            onPressedChanged: if (!pressed) Player.seek(value)
        }
    }
}
