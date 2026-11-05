import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtCore
import Earshot

ApplicationWindow {
    id: window
    width: 1320
    height: 860
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    title: Player.current ? Player.current.title + " · " + Player.current.artist + " — Earshot" : "Earshot"
    color: Theme.background
    font.family: Theme.fontFamily
    font.pixelSize: Theme.fontBody

    palette {
        window: Theme.background
        windowText: Theme.text
        base: Theme.surface
        alternateBase: Theme.surfaceRaised
        text: Theme.text
        button: Theme.surfaceRaised
        buttonText: Theme.text
        highlight: Theme.accent
        highlightedText: "#ffffff"
        light: Theme.surfaceHover
        midlight: Theme.surfaceHover
        mid: Theme.border
        dark: Theme.textMuted
        shadow: "#000000"
        toolTipBase: Theme.surfaceRaised
        toolTipText: Theme.text
        placeholderText: Theme.textFaint
    }

    Settings {
        category: "window"
        property alias width: window.width
        property alias height: window.height
    }

    readonly property var pages: ({
        home: homePage, search: searchPage, radio: radioPage, chart: chartPage, album: albumPage,
        artist: artistPage, playlist: playlistPage, albums: albumsPage, queue: queuePage
    })

    function navigate(page, args) {
        const component = pages[page]
        if (!component)
            return
        args = args || {}
        const top = stack.currentItem
        if (top && top.pageName === page && JSON.stringify(top.args) === JSON.stringify(args))
            return
        if (page === "home" || page === "search" || page === "radio" || page === "albums" || page === "queue" || (page === "playlist" && top && top.pageName === "playlist"))
            stack.replace(null, component, { args: args })
        else
            stack.push(component, { args: args })
    }

    function syncNav() {
        const top = stack.currentItem
        if (top) {
            Nav.currentArgs = top.args || {}
            Nav.current = top.pageName
        }
    }

    Connections {
        target: Nav
        function onGo(page, args) { window.navigate(page, args) }
        function onBack() { if (stack.depth > 1) stack.pop() }
        function onToast(message) { toast.show(message) }
        function onAskNewPlaylist(tracks) {
            newPlaylist.tracks = tracks || []
            newPlaylist.ask("")
        }
        function onAskImportPlaylist() { importPrompt.ask("") }
    }

    Component { id: homePage; HomePage {} }
    Component { id: searchPage; SearchPage {} }
    Component { id: radioPage; RadioPage {} }
    Component { id: chartPage; ChartPage {} }
    Component { id: albumPage; AlbumPage {} }
    Component { id: artistPage; ArtistPage {} }
    Component { id: playlistPage; PlaylistPage {} }
    Component { id: albumsPage; LibraryAlbumsPage {} }
    Component { id: queuePage; QueuePage {} }

    ColumnLayout {
        id: shell
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Sidebar {
                Layout.fillHeight: true
            }

            Item {
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true

                Rectangle {
                    anchors.fill: parent
                    color: Theme.background
                }

                StackView {
                    id: stack
                    anchors.fill: parent
                    initialItem: homePage
                    onCurrentItemChanged: window.syncNav()
                    pushEnter: Transition {
                        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160 }
                        NumberAnimation { property: "y"; from: 12; to: 0; duration: 200; easing.type: Easing.OutCubic }
                    }
                    pushExit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 120 } }
                    popEnter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160 } }
                    popExit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 120 } }
                    replaceEnter: Transition { NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 160 } }
                    replaceExit: Transition { NumberAnimation { property: "opacity"; from: 1; to: 0; duration: 100 } }
                }

                IconButton {
                    visible: stack.depth > 1
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.margins: 14
                    z: 10
                    iconName: "back"
                    iconSize: 18
                    color: Theme.text
                    tip: "Back"
                    background: Rectangle { radius: width / 2; color: Qt.rgba(0, 0, 0, 0.55) }
                    onClicked: stack.pop()
                }
            }
        }

        PlayerBar {
            Layout.fillWidth: true
        }
    }

    // Short notices ("Added to playlist", errors)
    Rectangle {
        id: toast
        property string message
        function show(text) {
            message = text
            opacity = 1
            hideTimer.restart()
        }
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 112
        width: Math.min(toastText.implicitWidth + 40, window.width - 80)
        height: 44
        radius: 22
        color: Theme.text
        opacity: 0
        visible: opacity > 0
        z: 100
        Behavior on opacity { NumberAnimation { duration: 180 } }
        Text {
            id: toastText
            anchors.centerIn: parent
            width: parent.width - 40
            horizontalAlignment: Text.AlignHCenter
            text: toast.message
            color: Theme.background
            font.pixelSize: Theme.fontBody
            font.weight: Font.DemiBold
            elide: Text.ElideRight
        }
        Timer { id: hideTimer; interval: 2800; onTriggered: toast.opacity = 0 }
    }

    TextPrompt {
        id: newPlaylist
        property var tracks: []
        title: "New playlist"
        message: tracks.length === 1 ? "\"" + tracks[0].title + "\" will be added to it."
               : tracks.length > 1 ? tracks.length + " songs will be added to it." : ""
        placeholder: "Playlist name"
        acceptText: "Create"
        onAccepted: name => {
            const id = Backend.createPlaylist(name, JSON.stringify(tracks))
            Nav.notify("Created " + name)
            if (tracks.length === 0)
                Nav.playlist(id)
        }
    }

    TextPrompt {
        id: importPrompt
        title: "Import a YouTube playlist"
        message: "Paste a link to a YouTube or YouTube Music playlist. Its songs are copied into a new playlist here."
        placeholder: "https://www.youtube.com/playlist?list=…"
        acceptText: "Import"
        onAccepted: url => {
            Nav.notify("Importing playlist…")
            Api.call(Backend.importPlaylist(url), function (result) {
                const id = Backend.createPlaylist(result.title, JSON.stringify(result.tracks))
                Nav.notify("Imported " + result.tracks.length + " songs into " + result.title)
                Nav.playlist(id)
            }, function (message) { Nav.notify(message) })
        }
    }

    Shortcut { sequence: "Ctrl+F"; onActivated: Nav.open("search") }
    Shortcut { sequence: "Ctrl+L"; onActivated: Nav.open("search") }
    Shortcut { sequences: [StandardKey.Back, "Alt+Left"]; onActivated: if (stack.depth > 1) stack.pop() }
    Shortcut { sequence: "Ctrl+Right"; onActivated: Player.next(false) }
    Shortcut { sequence: "Ctrl+Left"; onActivated: Player.previous() }
    Shortcut { sequence: "Ctrl+S"; onActivated: Player.setShuffle(!Player.shuffle) }
    Shortcut { sequence: "Ctrl+R"; onActivated: Player.cycleRepeat() }
    Shortcut { sequence: "Ctrl+N"; onActivated: Nav.askNewPlaylist([]) }
    Shortcut {
        sequence: "Space"
        enabled: !(window.activeFocusItem instanceof TextInput) && !(window.activeFocusItem instanceof TextEdit)
        onActivated: Player.toggle()
    }
    Shortcut { sequence: "Media Play"; onActivated: Player.toggle() }
    Shortcut { sequence: "Toggle Media Play/Pause"; onActivated: Player.toggle() }
    Shortcut { sequence: "Media Pause"; onActivated: Player.toggle() }
    Shortcut { sequence: "Media Next"; onActivated: Player.next(false) }
    Shortcut { sequence: "Media Previous"; onActivated: Player.previous() }

    // Testing aids: --page=album:123 opens a page; --screenshot=file.png saves the window and quits.
    Component.onCompleted: {
        const args = Qt.application.arguments
        for (let i = 0; i < args.length; ++i) {
            const arg = args[i]
            if (arg.startsWith("--page=")) {
                const spec = arg.substring(7).split(":")
                const pageArgs = spec.length > 1 ? (spec[0] === "chart" ? { kind: spec[1], id: spec[2] } : { id: spec.slice(1).join(":") }) : {}
                Qt.callLater(() => window.navigate(spec[0], pageArgs))
            } else if (arg.startsWith("--search=")) {
                const q = arg.substring(9)
                Qt.callLater(() => window.navigate("search", { query: q }))
            } else if (arg.startsWith("--play-first")) {
                autoPlay.start()
            } else if (arg.startsWith("--screenshot=")) {
                screenshot.path = arg.substring(13)
                screenshot.start()
            }
        }
    }

    Timer {
        id: autoPlay
        interval: 5000
        onTriggered: {
            const page = stack.currentItem
            if (page && page.firstTracks && page.firstTracks().length > 0)
                Player.playTracks(page.firstTracks(), 0, "Test")
        }
    }

    Timer {
        id: screenshot
        property string path
        interval: 9000
        Component.onCompleted: {
            for (const arg of Qt.application.arguments)
                if (arg.startsWith("--screenshot-delay="))
                    interval = parseInt(arg.substring(19))
        }
        onTriggered: shell.grabToImage(result => {
            result.saveToFile(path)
            Qt.quit()
        })
    }
}
