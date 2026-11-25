import QtQuick
import QtQuick.Controls.Basic
import Earshot

// A song's menu: the shared macOS 26 menu (MacMenu, from
// hearth/shared/app-menus) in Earshot's colours.

MacMenu {
    id: menu
    dark: true
    accent: Theme.accent
    property var tracks: []           // the songs this menu acts on
    property string playlistId: ""    // set when the songs are in one of your playlists
    property int playlistIndex: -1
    property int playlistLength: 0
    property int queueIndex: -1
    property bool historyMode: false  // set on the History page
    property var playlists: []

    readonly property var track: tracks.length === 1 ? tracks[0] : null
    readonly property bool liked: track ? (Backend.libraryRevision, Backend.isLiked(track.key)) : false
    readonly property string webPage: Api.songPage(track)

    signal removed()

    function openFor(list, anchorItem) {
        tracks = list
        playlists = Api.playlists()
        if (anchorItem)
            popup(anchorItem, anchorItem.width - width, anchorItem.height)
        else
            popup()
    }


    MacMenuItem { text: menu.tracks.length > 1 ? "Play all next" : "Play next"; onTriggered: Player.playNext(menu.tracks) }
    MacMenuItem { text: "Add to queue"; onTriggered: Player.enqueue(menu.tracks) }
    MacMenuItem {
        text: "Open on " + Api.siteName(menu.webPage)
        visible: menu.webPage !== ""
        height: visible ? implicitHeight : 0
        onTriggered: Api.openWeb(menu.webPage)
    }

    MacMenu {
        id: addMenu
        title: "Add to playlist"
        dark: true
        accent: Theme.accent

        MacMenuItem {
            text: "New playlist…"
            onTriggered: Nav.askNewPlaylist(menu.tracks)
        }
        MacMenuSeparator {}
        Instantiator {
            model: menu.playlists.filter(p => p.id !== "liked")
            delegate: MacMenuItem {
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

    MacMenuItem {
        text: menu.liked ? "Remove from Liked songs" : "Add to Liked songs"
        visible: menu.track !== null
        height: visible ? implicitHeight : 0
        onTriggered: Backend.setLiked(JSON.stringify(menu.track), !menu.liked)
    }

    MacMenuSeparator {}

    MacMenuItem {
        text: "Go to album"
        enabled: menu.track !== null && !!menu.track.albumId
        height: menu.track && menu.track.albumId ? implicitHeight : 0
        visible: height > 0
        onTriggered: Nav.album(menu.track.albumId)
    }
    MacMenuItem {
        text: "Go to artist"
        enabled: menu.track !== null && !!menu.track.artistId
        height: menu.track && menu.track.artistId ? implicitHeight : 0
        visible: height > 0
        onTriggered: Nav.artist(menu.track.artistId)
    }
    MacMenuItem {
        text: "Open on Reddit"
        height: menu.track && menu.track.link ? implicitHeight : 0
        visible: height > 0
        onTriggered: Qt.openUrlExternally(menu.track.link)
    }
    MacMenuItem {
        text: "Move up"
        height: menu.playlistId !== "" && menu.playlistIndex > 0 ? implicitHeight : 0
        visible: height > 0
        onTriggered: Backend.moveInPlaylist(menu.playlistId, menu.playlistIndex, menu.playlistIndex - 1)
    }
    MacMenuItem {
        text: "Move down"
        height: menu.playlistId !== "" && menu.playlistIndex >= 0 && menu.playlistIndex < menu.playlistLength - 1 ? implicitHeight : 0
        visible: height > 0
        onTriggered: Backend.moveInPlaylist(menu.playlistId, menu.playlistIndex, menu.playlistIndex + 1)
    }
    MacMenuItem {
        text: "Remove from this playlist"
        height: menu.playlistId !== "" && menu.playlistIndex >= 0 ? implicitHeight : 0
        visible: height > 0
        onTriggered: {
            Backend.removeFromPlaylist(menu.playlistId, menu.playlistIndex)
            menu.removed()
        }
    }
    MacMenuItem {
        text: "Remove from queue"
        height: menu.queueIndex >= 0 && menu.queueIndex !== Player.index ? implicitHeight : 0
        visible: height > 0
        onTriggered: Player.removeAt(menu.queueIndex)
    }
    MacMenuItem {
        text: "Remove from History"
        height: menu.historyMode && menu.track !== null ? implicitHeight : 0
        visible: height > 0
        onTriggered: Api.call(Backend.call("forgetPlay", JSON.stringify({ key: menu.track.key })), function () {}, function (message) { Nav.notify(message) })
    }
}
