pragma Singleton
import QtQuick
import Earshot

// Turns the backend's request ids into callbacks.
QtObject {
    id: api

    property var pending: ({})
    property var homeCache: null
    property var radioCache: ({})
    property string lastSearch: ""
    property string lastStation: "listentothis"

    // Fetches an album's songs and plays them from the top.
    function playAlbum(id, title, shuffle) {
        Nav.notify("Loading " + (title || "album") + "…")
        call(Backend.album(id), function (album) {
            if (shuffle)
                Player.shuffleTracks(album.tracks, album.title)
            else
                Player.playTracks(album.tracks, 0, album.title)
        }, function (message) { Nav.notify(message) })
    }

    function call(requestId, onReply, onError) {
        pending[requestId] = { ok: onReply, fail: onError }
        return requestId
    }

    function cancel(requestId) {
        delete pending[requestId]
    }

    property Connections connections: Connections {
        target: Backend
        function onReply(request, json) {
            const handler = api.pending[request]
            if (!handler)
                return
            delete api.pending[request]
            let data = null
            try {
                data = JSON.parse(json)
            } catch (error) {
                if (handler.fail) handler.fail("Unexpected answer")
                return
            }
            if (handler.ok) handler.ok(data)
        }
        function onFailed(request, message) {
            const handler = api.pending[request]
            if (!handler)
                return
            delete api.pending[request]
            if (handler.fail) handler.fail(message)
        }
    }

    function playlists() {
        return JSON.parse(Backend.playlists())
    }

    function playlist(id) {
        return JSON.parse(Backend.playlist(id))
    }
}
