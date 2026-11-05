pragma Singleton
import QtQuick

// Navigation requests and short notices, handled by the main window.
QtObject {
    signal go(string page, var args)
    signal back()
    signal toast(string message)
    signal askNewPlaylist(var tracks)
    signal askImportPlaylist()

    property string current: "home"
    property var currentArgs: ({})

    function open(page, args) {
        go(page, args || {})
    }
    function album(id) {
        if (id) go("album", { id: id })
    }
    function artist(id) {
        if (id) go("artist", { id: id })
    }
    function playlist(id) {
        go("playlist", { id: id })
    }
    function notify(message) {
        toast(message)
    }
}
