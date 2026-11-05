import QtQuick
import QtQuick.Controls.Basic
import Earshot

Menu {
    id: menu
    property var tracks: []           // the songs this menu acts on
    property string playlistId: ""    // set when the songs are in one of your playlists
    property int playlistIndex: -1
    property int playlistLength: 0
    property int queueIndex: -1
    property var playlists: []

    readonly property var track: tracks.length === 1 ? tracks[0] : null
    readonly property bool liked: track ? (Backend.libraryRevision, Backend.isLiked(track.key)) : false

    signal removed()

    function openFor(list, anchorItem) {
        tracks = list
        playlists = Api.playlists()
        if (anchorItem)
            popup(anchorItem, anchorItem.width - width, anchorItem.height)
        else
            popup()
    }

    topPadding: 6
    bottomPadding: 6
    implicitWidth: 240

    background: Rectangle {
        implicitWidth: 240
        color: Theme.surfaceRaised
        radius: Theme.radius
        border.color: Theme.border
    }

    delegate: MenuItem { id: item; contentItem: Text { leftPadding: 6; text: item.text; color: item.enabled ? Theme.text : Theme.textFaint; font.pixelSize: Theme.fontBody; verticalAlignment: Text.AlignVCenter; elide: Text.ElideRight }
        background: Rectangle { color: item.highlighted ? Theme.surfaceHover : "transparent"; radius: 6; anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6 }
        arrow: Icon { x: item.width - width - 14; anchors.verticalCenter: parent.verticalCenter; name: "forward"; size: 14; visible: item.subMenu; color: Theme.textMuted }
        implicitHeight: 34 }

    MenuItem { text: menu.tracks.length > 1 ? "Play all next" : "Play next"; onTriggered: Player.playNext(menu.tracks) }
    MenuItem { text: "Add to queue"; onTriggered: Player.enqueue(menu.tracks) }

    Menu {
        id: addMenu
        title: "Add to playlist"
        topPadding: 6
        bottomPadding: 6
        background: Rectangle { implicitWidth: 220; color: Theme.surfaceRaised; radius: Theme.radius; border.color: Theme.border }
        delegate: menu.delegate

        MenuItem {
            text: "New playlist…"
            onTriggered: Nav.askNewPlaylist(menu.tracks)
        }
        MenuSeparator {
            contentItem: Rectangle { implicitHeight: 1; color: Theme.border }
        }
        Instantiator {
            model: menu.playlists.filter(p => p.id !== "liked")
            delegate: MenuItem {
                required property var modelData
                text: modelData.name
                onTriggered: {
                    const added = Backend.addToPlaylist(modelData.id, JSON.stringify(menu.tracks))
                    Nav.notify(added > 0 ? "Added " + (added === 1 ? "1 song" : added + " songs") + " to " + modelData.name
                                         : "Already in " + modelData.name)
                }
            }
            onObjectAdded: (index, object) => addMenu.insertItem(index + 2, object)
            onObjectRemoved: (index, object) => addMenu.removeItem(object)
        }
    }

    MenuItem {
        text: menu.liked ? "Remove from Liked songs" : "Add to Liked songs"
        visible: menu.track !== null
        height: visible ? implicitHeight : 0
        onTriggered: Backend.setLiked(JSON.stringify(menu.track), !menu.liked)
    }

    MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: Theme.border } }

    MenuItem {
        text: "Go to album"
        enabled: menu.track !== null && !!menu.track.albumId
        height: menu.track && menu.track.albumId ? implicitHeight : 0
        visible: height > 0
        onTriggered: Nav.album(menu.track.albumId)
    }
    MenuItem {
        text: "Go to artist"
        enabled: menu.track !== null && !!menu.track.artistId
        height: menu.track && menu.track.artistId ? implicitHeight : 0
        visible: height > 0
        onTriggered: Nav.artist(menu.track.artistId)
    }
    MenuItem {
        text: "Open on Reddit"
        height: menu.track && menu.track.link ? implicitHeight : 0
        visible: height > 0
        onTriggered: Qt.openUrlExternally(menu.track.link)
    }
    MenuItem {
        text: "Move up"
        height: menu.playlistId !== "" && menu.playlistIndex > 0 ? implicitHeight : 0
        visible: height > 0
        onTriggered: Backend.moveInPlaylist(menu.playlistId, menu.playlistIndex, menu.playlistIndex - 1)
    }
    MenuItem {
        text: "Move down"
        height: menu.playlistId !== "" && menu.playlistIndex >= 0 && menu.playlistIndex < menu.playlistLength - 1 ? implicitHeight : 0
        visible: height > 0
        onTriggered: Backend.moveInPlaylist(menu.playlistId, menu.playlistIndex, menu.playlistIndex + 1)
    }
    MenuItem {
        text: "Remove from this playlist"
        height: menu.playlistId !== "" && menu.playlistIndex >= 0 ? implicitHeight : 0
        visible: height > 0
        onTriggered: {
            Backend.removeFromPlaylist(menu.playlistId, menu.playlistIndex)
            menu.removed()
        }
    }
    MenuItem {
        text: "Remove from queue"
        height: menu.queueIndex >= 0 && menu.queueIndex !== Player.index ? implicitHeight : 0
        visible: height > 0
        onTriggered: Player.removeAt(menu.queueIndex)
    }
}
