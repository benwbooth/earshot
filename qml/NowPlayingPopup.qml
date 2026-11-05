import QtQuick
import QtQuick.Layouts
import Earshot

// A small card in the corner of the screen when a new song starts, while
// Earshot's window isn't the one you're looking at. Click it to open Earshot.
Window {
    id: popup
    signal openRequested()

    property var track: null
    property int margin: 16

    width: 380
    height: Math.max(96, body.implicitHeight + 24)
    color: "transparent"
    flags: Qt.Tool | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.WindowDoesNotAcceptFocus
    title: "Earshot — now playing"

    readonly property bool wayland: Qt.platform.pluginName.indexOf("wayland") === 0

    // Where it sits on desktops that let windows place themselves.
    x: Screen.desktopAvailableX + Screen.desktopAvailableWidth - width - margin
    y: Screen.desktopAvailableY + Screen.desktopAvailableHeight - height - margin

    function announce(newTrack) {
        track = newTrack
        card.opacity = 0
        card.offset = 24
        show()
        card.opacity = 1
        card.offset = 0
        hideTimer.restart()
    }

    function dismiss() {
        card.opacity = 0
        closeTimer.restart()
    }

    // On Wayland, windows can't choose their own position; KDE's layer-shell
    // places the card in the corner instead (where it's available).
    Component.onCompleted: {
        if (!wayland)
            return
        const helper = Qt.createComponent("qrc:/qt/qml/Earshot/qml/NotificationLayerShell.qml")
        if (helper.status === Component.Ready)
            helper.createObject(popup, { target: popup })
    }

    Timer {
        id: hideTimer
        interval: 5000
        onTriggered: if (!hover.hovered) popup.dismiss(); else restart()
    }
    Timer {
        id: closeTimer
        interval: 220
        onTriggered: popup.hide()
    }

    Rectangle {
        id: card
        property real offset: 0
        width: parent.width
        height: parent.height
        y: offset
        radius: 16
        color: Theme.surfaceRaised
        border.color: Theme.border
        opacity: 0
        Behavior on opacity { NumberAnimation { duration: 200 } }
        Behavior on offset { NumberAnimation { duration: 260; easing.type: Easing.OutCubic } }

        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler {
            onTapped: {
                popup.openRequested()
                popup.dismiss()
            }
        }

        // Faint artwork wash, like the mini player
        Cover {
            anchors.fill: parent
            radius: 16
            source: popup.track ? (popup.track.cover || "") : ""
            seed: popup.track ? popup.track.key : ""
            opacity: 0.12
        }

        RowLayout {
            id: body
            anchors.fill: parent
            anchors.margins: 12
            spacing: 14

            Cover {
                Layout.preferredWidth: 72
                Layout.preferredHeight: 72
                Layout.alignment: Qt.AlignTop
                radius: 10
                source: popup.track ? (popup.track.cover || "") : ""
                seed: popup.track ? popup.track.key : ""
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignVCenter
                spacing: 3
                RowLayout {
                    spacing: 8
                    Waveform { Layout.preferredHeight: 12; barWidth: 2.5; gap: 1.5 }
                    Text {
                        text: "NOW PLAYING"
                        color: Theme.accent
                        font.pixelSize: Theme.fontSmall - 1
                        font.weight: Font.DemiBold
                        font.letterSpacing: 1
                    }
                }
                Text {
                    Layout.fillWidth: true
                    text: popup.track ? popup.track.title : ""
                    color: Theme.text
                    font.pixelSize: Theme.fontBody + 1
                    font.weight: Font.DemiBold
                    wrapMode: Text.Wrap
                }
                Text {
                    Layout.fillWidth: true
                    text: popup.track ? popup.track.artist + (popup.track.album ? " · " + popup.track.album : "") : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall + 1
                    wrapMode: Text.Wrap
                }
            }
        }
    }
}
