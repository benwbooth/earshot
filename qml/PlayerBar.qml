import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// The now-playing bar along the bottom of the window.
Rectangle {
    id: bar
    implicitHeight: 92
    color: Theme.sidebar

    readonly property var track: Player.current
    readonly property bool liked: track ? (Backend.libraryRevision, Backend.isLiked(track.key)) : false

    Rectangle { anchors.top: parent.top; width: parent.width; height: 1; color: Theme.border }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 16
        anchors.rightMargin: 20
        spacing: 16

        // Song
        RowLayout {
            Layout.preferredWidth: (bar.width - 32) * 0.3
            Layout.maximumWidth: 380
            Layout.fillHeight: true
            spacing: 14
            Cover {
                Layout.preferredWidth: 60
                Layout.preferredHeight: 60
                radius: 8
                visible: bar.track !== null
                source: bar.track ? (bar.track.cover || "") : ""
                seed: bar.track ? bar.track.key : ""
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: bar.track && bar.track.albumId ? Nav.album(bar.track.albumId) : Nav.open("queue")
                }
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 3
                Text {
                    Layout.fillWidth: true
                    text: bar.track ? bar.track.title : "Nothing playing"
                    color: Theme.text
                    font.pixelSize: Theme.fontBody + 1
                    font.weight: Font.DemiBold
                    elide: Text.ElideRight
                }
                Text {
                    Layout.fillWidth: true
                    text: bar.track ? bar.track.artist : "Pick a song, album or radio station"
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall + 1
                    elide: Text.ElideRight
                    MouseArea {
                        anchors.fill: parent
                        enabled: bar.track !== null && !!bar.track.artistId
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Nav.artist(bar.track.artistId)
                    }
                }
                Text {
                    Layout.fillWidth: true
                    visible: Player.context !== "" && bar.track !== null
                    text: "From " + Player.context
                    color: Theme.textFaint
                    font.pixelSize: Theme.fontSmall
                    elide: Text.ElideRight
                }
            }
            IconButton {
                visible: bar.track !== null
                iconName: bar.liked ? "heart-filled" : "heart"
                iconSize: 18
                color: bar.liked ? Theme.accent : Theme.textMuted
                hoverColor: bar.liked ? Theme.accentHover : Theme.text
                tip: bar.liked ? "Remove from Liked songs" : "Add to Liked songs"
                onClicked: Backend.setLiked(JSON.stringify(bar.track), !bar.liked)
            }
        }

        // Controls
        ColumnLayout {
            Layout.fillWidth: true
            Layout.alignment: Qt.AlignVCenter
            spacing: 6

            RowLayout {
                Layout.alignment: Qt.AlignHCenter
                spacing: 14
                IconButton {
                    iconName: "shuffle"
                    iconSize: 18
                    active: Player.shuffle
                    tip: Player.shuffle ? "Shuffle on" : "Shuffle off"
                    onClicked: Player.setShuffle(!Player.shuffle)
                }
                IconButton {
                    iconName: "previous"
                    iconSize: 20
                    enabled: bar.track !== null
                    color: Theme.text
                    tip: "Previous"
                    onClicked: Player.previous()
                }
                Item {
                    implicitWidth: 44
                    implicitHeight: 44
                    IconButton {
                        id: playButton
                        anchors.fill: parent
                        filled: false
                        iconName: Player.playing ? "pause" : "play"
                        iconSize: 20
                        enabled: bar.track !== null
                        visible: !Player.loading
                        tip: Player.playing ? "Pause" : "Play"
                        onClicked: Player.toggle()
                        background: Rectangle {
                            radius: width / 2
                            color: "#ffffff"
                            opacity: playButton.enabled ? 1 : 0.4
                            scale: playButton.pressed ? 0.94 : (playButton.hovered ? 1.06 : 1)
                            Behavior on scale { NumberAnimation { duration: 90 } }
                        }
                        contentItem: Item {
                            Icon { anchors.centerIn: parent; name: playButton.iconName; size: 20; color: Theme.background }
                        }
                    }
                    BusyIndicator {
                        anchors.fill: parent
                        visible: Player.loading
                        running: visible
                        palette.dark: Theme.accent
                    }
                }
                IconButton {
                    iconName: "next"
                    iconSize: 20
                    enabled: Player.hasNext
                    color: Theme.text
                    tip: "Next"
                    onClicked: Player.next(false)
                }
                IconButton {
                    iconName: Player.repeat === 2 ? "repeat-one" : "repeat"
                    iconSize: 18
                    active: Player.repeat > 0
                    tip: ["Repeat off", "Repeat all", "Repeat this song"][Player.repeat]
                    onClicked: Player.cycleRepeat()
                }
            }

            RowLayout {
                Layout.fillWidth: true
                Layout.maximumWidth: 640
                Layout.alignment: Qt.AlignHCenter
                spacing: 10
                Text {
                    Layout.preferredWidth: 40
                    horizontalAlignment: Text.AlignRight
                    text: Theme.duration(seek.pressed ? seek.value : Player.position)
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall - 1
                    font.features: { "tnum": 1 }
                }
                SeekSlider {
                    id: seek
                    Layout.fillWidth: true
                    from: 0
                    to: Math.max(1, Player.length)
                    enabled: bar.track !== null && Player.length > 0
                    value: pressed ? value : Player.position
                    onPressedChanged: if (!pressed) Player.seek(value)
                }
                Text {
                    Layout.preferredWidth: 40
                    text: Player.length > 0 ? Theme.duration(Player.length) : "-:--"
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall - 1
                    font.features: { "tnum": 1 }
                }
            }

            Text {
                Layout.alignment: Qt.AlignHCenter
                visible: Player.error !== "" && !Player.loading
                text: Player.error
                color: Theme.danger
                font.pixelSize: Theme.fontSmall - 1
                elide: Text.ElideRight
                Layout.maximumWidth: 560
            }
        }

        // Volume and queue
        RowLayout {
            Layout.preferredWidth: (bar.width - 32) * 0.3
            Layout.maximumWidth: 380
            Layout.alignment: Qt.AlignRight | Qt.AlignVCenter
            spacing: 6
            Item { Layout.fillWidth: true }
            IconButton {
                iconName: "queue"
                iconSize: 18
                active: Nav.current === "queue"
                tip: "Queue"
                onClicked: Nav.current === "queue" ? Nav.back() : Nav.open("queue")
            }
            IconButton {
                iconName: Player.muted || Player.volume === 0 ? "volume-mute" : "volume"
                iconSize: 18
                tip: Player.muted ? "Unmute" : "Mute"
                onClicked: Player.muted = !Player.muted
            }
            SeekSlider {
                Layout.preferredWidth: 110
                from: 0
                to: 1
                value: Player.volume
                onMoved: {
                    Player.volume = value
                    Player.muted = false
                }
            }
        }
    }
}
