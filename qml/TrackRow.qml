import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// One song in a list: number or cover, title, artist, album, length, like and more.
Rectangle {
    id: row
    property var track
    property int number: 0
    property bool showCover: true
    property bool showAlbum: true
    property bool showNumber: true
    readonly property bool isCurrent: Player.current !== null && track !== undefined && Player.current.key === track.key
    readonly property bool liked: (Backend.libraryRevision, Backend.isLiked(track.key))

    signal activated()
    signal menuRequested(Item anchor)

    implicitHeight: 56
    radius: 8
    color: hover.hovered ? Theme.surfaceHover : "transparent"

    HoverHandler { id: hover }
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onDoubleTapped: row.activated()
    }
    TapHandler {
        acceptedButtons: Qt.RightButton
        onTapped: row.menuRequested(moreButton)
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 12
        anchors.rightMargin: 8
        spacing: 14

        Item {
            visible: row.showNumber
            Layout.preferredWidth: 26
            Layout.fillHeight: true
            Text {
                anchors.centerIn: parent
                visible: !hover.hovered && !row.isCurrent
                text: row.number
                color: Theme.textMuted
                font.pixelSize: Theme.fontBody
                font.features: { "tnum": 1 }
            }
            Icon {
                anchors.centerIn: parent
                visible: hover.hovered || row.isCurrent
                name: row.isCurrent && Player.playing && !hover.hovered ? "volume" : (row.isCurrent && Player.playing ? "pause" : "play")
                size: 16
                color: row.isCurrent ? Theme.accent : Theme.text
            }
            MouseArea {
                anchors.fill: parent
                cursorShape: Qt.PointingHandCursor
                onClicked: row.isCurrent ? Player.toggle() : row.activated()
            }
        }

        Cover {
            visible: row.showCover
            Layout.preferredWidth: 40
            Layout.preferredHeight: 40
            radius: 6
            source: row.track.cover || ""
            seed: row.track.key
            MouseArea {
                anchors.fill: parent
                enabled: !row.showNumber
                cursorShape: Qt.PointingHandCursor
                onClicked: row.isCurrent ? Player.toggle() : row.activated()
            }
            Rectangle {
                anchors.fill: parent
                radius: 6
                color: Qt.rgba(0, 0, 0, 0.45)
                visible: !row.showNumber && (hover.hovered || row.isCurrent)
                Icon {
                    anchors.centerIn: parent
                    name: row.isCurrent && Player.playing ? "pause" : "play"
                    size: 16
                }
            }
        }

        ColumnLayout {
            Layout.fillWidth: true
            Layout.preferredWidth: 3
            spacing: 2
            Text {
                Layout.fillWidth: true
                text: row.track.title
                color: row.isCurrent ? Theme.accent : Theme.text
                font.pixelSize: Theme.fontBody + 1
                font.weight: Font.Medium
                elide: Text.ElideRight
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 6
                Rectangle {
                    visible: !!row.track.explicit
                    implicitWidth: 14; implicitHeight: 14; radius: 3
                    color: Theme.textFaint
                    Text { anchors.centerIn: parent; text: "E"; font.pixelSize: 9; font.bold: true; color: Theme.background }
                }
                Text {
                    Layout.fillWidth: true
                    text: row.track.artist + (row.track.source ? "  ·  " + row.track.source : "") + (row.track.tags ? "  ·  " + row.track.tags : "")
                    color: artistMouse.containsMouse && row.track.artistId ? Theme.text : Theme.textMuted
                    font.pixelSize: Theme.fontSmall + 1
                    font.underline: artistMouse.containsMouse && !!row.track.artistId
                    elide: Text.ElideRight
                    MouseArea {
                        id: artistMouse
                        anchors.fill: parent
                        hoverEnabled: true
                        enabled: !!row.track.artistId
                        cursorShape: Qt.PointingHandCursor
                        onClicked: Nav.artist(row.track.artistId)
                    }
                }
            }
        }

        Text {
            visible: row.showAlbum && row.width > 640
            Layout.fillWidth: true
            Layout.preferredWidth: 2
            text: row.track.album || ""
            color: albumMouse.containsMouse ? Theme.text : Theme.textMuted
            font.pixelSize: Theme.fontSmall + 1
            font.underline: albumMouse.containsMouse
            elide: Text.ElideRight
            MouseArea {
                id: albumMouse
                anchors.fill: parent
                hoverEnabled: true
                enabled: !!row.track.albumId
                cursorShape: Qt.PointingHandCursor
                onClicked: Nav.album(row.track.albumId)
            }
        }

        IconButton {
            iconName: row.liked ? "heart-filled" : "heart"
            iconSize: 16
            color: row.liked ? Theme.accent : Theme.textMuted
            hoverColor: row.liked ? Theme.accentHover : Theme.text
            opacity: row.liked || hover.hovered ? 1 : 0
            tip: row.liked ? "Remove from Liked songs" : "Add to Liked songs"
            onClicked: Backend.setLiked(JSON.stringify(row.track), !row.liked)
        }

        Text {
            Layout.preferredWidth: 44
            horizontalAlignment: Text.AlignRight
            text: row.track.duration > 0 ? Theme.duration(row.track.duration) : ""
            color: Theme.textMuted
            font.pixelSize: Theme.fontSmall + 1
            font.features: { "tnum": 1 }
        }

        IconButton {
            id: moreButton
            iconName: "more"
            iconSize: 16
            opacity: hover.hovered ? 1 : 0
            tip: "More"
            onClicked: row.menuRequested(moreButton)
        }
    }
}
