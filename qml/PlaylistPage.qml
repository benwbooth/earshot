import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

Flickable {
    id: page
    readonly property string pageName: "playlist"
    property var args: ({})
    readonly property var playlist: (Backend.libraryRevision, Api.playlist(args.id || ""))
    readonly property bool special: !!playlist && playlist.special
    property string filter: ""
    readonly property var shown: {
        if (!playlist)
            return []
        const f = filter.trim().toLowerCase()
        if (f === "")
            return playlist.tracks
        return playlist.tracks.filter(t => (t.title + " " + t.artist + " " + (t.album || "")).toLowerCase().indexOf(f) >= 0)
    }
    // Started from this playlist's page (by its id, so a rename or another
    // playlist of the same name doesn't matter).
    readonly property bool playingThis: playlist !== null && Player.playingRoute("playlist", args.id || "")

    function firstTracks() { return playlist ? playlist.tracks : [] }

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    Rectangle {
        width: parent.width
        height: 400
        gradient: Gradient {
            GradientStop { position: 0; color: page.special ? Qt.rgba(1, 0.37, 0.5, 0.40) : Qt.rgba(0.24, 0.72, 1, 0.30) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 64
        width: page.width - Theme.pagePadding * 2
        spacing: 22

        StatusView {
            empty: page.playlist ? "" : "This playlist doesn't exist any more."
        }

        RowLayout {
            visible: !!page.playlist
            spacing: 28

            // Artwork: a heart for liked songs, otherwise a mosaic of the first covers
            Rectangle {
                Layout.preferredWidth: 232
                Layout.preferredHeight: 232
                radius: Theme.radius
                clip: true
                gradient: Gradient {
                    GradientStop { position: 0; color: page.special ? Theme.accent : "#2a3150" }
                    GradientStop { position: 1; color: page.special ? Theme.accent2 : "#151821" }
                }
                Icon {
                    anchors.centerIn: parent
                    visible: page.special || !page.playlist || page.playlist.covers.length === 0
                    name: page.special ? "heart-filled" : "playlist"
                    size: 72
                    color: "#ffffff"
                }
                Grid {
                    anchors.fill: parent
                    visible: !page.special && !!page.playlist && page.playlist.covers.length > 0
                    columns: page.playlist && page.playlist.covers.length >= 4 ? 2 : 1
                    Repeater {
                        model: page.playlist ? (page.playlist.covers.length >= 4 ? page.playlist.covers : page.playlist.covers.slice(0, 1)) : []
                        delegate: Cover {
                            required property var modelData
                            width: parent.columns === 2 ? 116 : 232
                            height: width
                            radius: 0
                            source: modelData
                        }
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignBottom
                spacing: 10
                Text {
                    text: "PLAYLIST"
                    color: Theme.text
                    font.pixelSize: Theme.fontSmall
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.5
                }
                Text {
                    Layout.fillWidth: true
                    text: page.playlist ? page.playlist.name : ""
                    color: Theme.text
                    font.pixelSize: page.playlist && page.playlist.name.length > 24 ? 40 : 56
                    font.weight: Font.Black
                    font.letterSpacing: -1
                    wrapMode: Text.Wrap
                    MouseArea {
                        anchors.fill: parent
                        enabled: !page.special
                        cursorShape: Qt.PointingHandCursor
                        onClicked: rename.ask(page.playlist.name)
                    }
                }
                Text {
                    text: page.playlist ? (page.playlist.count === 1 ? "1 song" : page.playlist.count + " songs") + (page.playlist.duration > 0 ? " · " + Theme.longDuration(page.playlist.duration) : "") : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontBody + 1
                }
            }
        }

        RowLayout {
            visible: !!page.playlist
            Layout.fillWidth: true
            spacing: 14
            IconButton {
                filled: true
                iconName: page.playingThis && Player.busy ? "pause" : "play"
                iconSize: 24
                enabled: page.playlist && page.playlist.count > 0
                tip: page.playingThis && Player.busy ? "Pause" : "Play"
                onClicked: page.playingThis ? Player.toggle() : Player.playTracks(page.playlist.tracks, 0, page.playlist.name)
            }
            PillButton {
                text: "Shuffle"
                iconName: "shuffle"
                enabled: page.playlist && page.playlist.count > 0
                onClicked: Player.shuffleTracks(page.playlist.tracks, page.playlist.name)
            }
            IconButton {
                visible: !page.special
                iconName: "edit"
                tip: "Rename"
                onClicked: rename.ask(page.playlist.name)
            }
            IconButton {
                visible: !page.special
                iconName: "trash"
                tip: "Delete playlist"
                onClicked: confirmDelete.open()
            }
            Item { Layout.fillWidth: true }
            TextField {
                id: filterField
                visible: page.playlist && page.playlist.count > 8
                Layout.preferredWidth: 220
                implicitHeight: 36
                leftPadding: 36
                placeholderText: "Find in playlist"
                placeholderTextColor: Theme.textFaint
                color: Theme.text
                font.pixelSize: Theme.fontBody
                onTextChanged: page.filter = text
                background: Rectangle { radius: 18; color: Theme.surface; border.color: filterField.activeFocus ? Theme.text : Theme.border }
                Icon { anchors.left: parent.left; anchors.leftMargin: 12; anchors.verticalCenter: parent.verticalCenter; name: "search"; size: 15; color: Theme.textMuted }
            }
        }

        StatusView {
            visible: !!page.playlist && page.playlist.count === 0
            empty: page.special ? "Songs you like show up here. Press the heart next to any song."
                                : "This playlist is empty. Use the ··· button next to any song or album to add it here."
            emptyIcon: page.special ? "heart" : "playlist"
        }

        TrackList {
            visible: !!page.playlist && page.playlist.count > 0
            Layout.fillWidth: true
            tracks: page.shown
            playlistId: page.filter === "" && page.playlist ? page.playlist.id : ""
            context: page.playlist ? page.playlist.name : ""
        }
    }

    TextPrompt {
        id: rename
        title: "Rename playlist"
        placeholder: "Playlist name"
        onAccepted: name => Backend.renamePlaylist(page.playlist.id, name)
    }

    Popup {
        id: confirmDelete
        modal: true
        anchors.centerIn: Overlay.overlay
        width: 400
        padding: 24
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
        background: Rectangle { color: Theme.surfaceRaised; radius: Theme.radiusLarge; border.color: Theme.border }
        contentItem: ColumnLayout {
            spacing: 16
            Text { text: "Delete playlist?"; color: Theme.text; font.pixelSize: Theme.fontTitle - 2; font.weight: Font.Bold }
            Text {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: page.playlist ? "\"" + page.playlist.name + "\" and its " + page.playlist.count + " songs will be removed from your library." : ""
                color: Theme.textMuted
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                PillButton { text: "Cancel"; onClicked: confirmDelete.close() }
                PillButton {
                    text: "Delete"
                    primary: true
                    implicitHeight: 36
                    onClicked: {
                        const name = page.playlist.name
                        confirmDelete.close()
                        Backend.deletePlaylist(page.playlist.id)
                        Nav.notify("Deleted " + name)
                        Nav.open("home")
                    }
                }
            }
        }
    }
}
