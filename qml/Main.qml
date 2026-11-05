import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtCore
import Qt.labs.platform as Platform
import Earshot

ApplicationWindow {
    id: window
    width: 1320
    height: 860
    minimumWidth: 900
    minimumHeight: 600
    visible: true
    flags: Qt.Window | Qt.FramelessWindowHint
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
        id: windowSettings
        category: "window"
        property alias width: window.width
        property alias height: window.height
        property alias sidebarWidth: sidebar.width
        property int x: -100000
        property int y: -100000
        property bool maximized: false
        property bool askedKde: false
    }

    // Where the window was last time. Windows, macOS and X11 let an app put
    // its window back; KDE on Wayland doesn't, but remembers it with a window
    // rule, which Earshot adds once.
    readonly property bool wayland: Qt.platform.pluginName.indexOf("wayland") === 0
    function restorePlace() {
        if (wayland) {
            if (!windowSettings.askedKde && Backend.desktop() === "kde") {
                windowSettings.askedKde = true
                if (Backend.rememberWindowWithKde())
                    Nav.notify("KDE will now remember where Earshot's window is (System Settings › Window Rules)")
            }
            return
        }
        const x = windowSettings.x, y = windowSettings.y
        // Only if that spot is still on a screen (a monitor may have gone).
        const onScreen = Qt.application.screens.some(s => x + 80 >= s.virtualX && x + 80 < s.virtualX + s.width
                                                          && y + 20 >= s.virtualY && y + 20 < s.virtualY + s.height)
        if (onScreen) {
            window.x = x
            window.y = y
        }
        if (windowSettings.maximized)
            window.showMaximized()
    }
    function savePlace() {
        if (wayland || !window.visible)
            return
        windowSettings.maximized = window.visibility === Window.Maximized
        if (window.visibility === Window.Windowed) {
            windowSettings.x = window.x
            windowSettings.y = window.y
        }
    }
    onXChanged: placeTimer.restart()
    onYChanged: placeTimer.restart()
    onVisibilityChanged: placeTimer.restart()
    Timer { id: placeTimer; interval: 400; onTriggered: window.savePlace() }

    // Closing the window keeps the music playing in the tray; quit from the tray menu.
    property bool quitting: false
    onClosing: close => {
        if (tray.available && !quitting) {
            close.accepted = false
            window.hide()
            if (!trayHint.shown) {
                tray.showMessage("Earshot is still playing", "It's in the system tray. Right-click the headphones icon to quit.", Platform.SystemTrayIcon.Information, 4000)
                trayHint.shown = true
            }
        }
    }

    // Qt 6 asks every window to close before quitting, and closing normally
    // hides to the tray, so mark the quit as real first.
    function quitApp() {
        savePlace()
        quitting = true
        miniPlayer.close()
        Qt.quit()
    }

    function showWindow() {
        miniPlayer.hide()
        window.show()
        window.raise()
        window.requestActivate()
    }

    // Compact mode: swap the full window for the small always-on-top player.
    function enterMini() {
        window.hide()
        miniPlayer.show()
        miniPlayer.raise()
    }

    // The corner card when a new song starts while you're in another window.
    NowPlayingPopup {
        id: nowPlaying
        onOpenRequested: window.showWindow()
    }
    Connections {
        target: Player
        function onTrackStarted(track) {
            if (!window.active && !miniPlayer.active)
                nowPlaying.announce(track)
        }
    }

    MiniPlayer {
        id: miniPlayer
        onExpandRequested: window.showWindow()
    }

    Settings {
        id: trayHint
        category: "tray"
        property bool shown: false
    }

    // Monochrome tray icon that suits the panel: light glyph on dark panels, dark on light ones.
    Platform.SystemTrayIcon {
        id: tray
        visible: true
        icon.source: Qt.styleHints.colorScheme === Qt.ColorScheme.Light ? Theme.icon("tray-dark") : Theme.icon("tray-light")
        tooltip: Player.current ? Player.current.title + " — " + Player.current.artist : "Earshot"
        onActivated: reason => {
            if (reason === Platform.SystemTrayIcon.Trigger || reason === Platform.SystemTrayIcon.DoubleClick) {
                if (window.visible && window.active)
                    window.hide()
                else
                    window.showWindow()
            } else if (reason === Platform.SystemTrayIcon.MiddleClick) {
                Player.toggle()
            }
        }
        menu: Platform.Menu {
            Platform.MenuItem {
                text: Player.current ? Player.current.title + " — " + Player.current.artist : "Nothing playing"
                icon.name: "audio-x-generic"
                enabled: false
            }
            Platform.MenuSeparator {}
            Platform.MenuItem { text: Player.playing ? "Pause" : "Play"; icon.name: Player.playing ? "media-playback-pause" : "media-playback-start"; enabled: Player.current !== null; onTriggered: Player.toggle() }
            Platform.MenuItem { text: "Next"; icon.name: "media-skip-forward"; enabled: Player.hasNext; onTriggered: Player.next(false) }
            Platform.MenuItem { text: "Previous"; icon.name: "media-skip-backward"; enabled: Player.current !== null; onTriggered: Player.previous() }
            Platform.MenuItem { text: Player.shuffle ? "Shuffle is on" : "Shuffle"; icon.name: "media-playlist-shuffle"; checkable: true; checked: Player.shuffle; onTriggered: Player.setShuffle(!Player.shuffle) }
            Platform.MenuSeparator {}
            Platform.MenuItem { text: window.visible ? "Hide Earshot" : "Show Earshot"; icon.name: window.visible ? "window-minimize" : "window-restore"; onTriggered: window.visible ? window.hide() : window.showWindow() }
            Platform.MenuItem { text: miniPlayer.visible ? "Full window" : "Mini player"; icon.name: miniPlayer.visible ? "view-fullscreen" : "view-restore"; onTriggered: miniPlayer.visible ? window.showWindow() : window.enterMini() }
            Platform.MenuItem { text: "Quit"; icon.name: "application-exit"; onTriggered: window.quitApp() }
        }
    }


    readonly property var pages: ({
        home: homePage, search: searchPage, radio: radioPage, chart: chartPage, album: albumPage,
        artist: artistPage, playlist: playlistPage, albums: albumsPage, queue: queuePage,
        bandcamp: bandcampPage, "bandcamp-tag": bandcampTagPage, service: servicePage
    })

    function navigate(page, args) {
        const component = pages[page]
        if (!component)
            return
        args = args || {}
        const top = stack.currentItem
        if (top && top.pageName === page && JSON.stringify(top.args) === JSON.stringify(args))
            return
        if (page === "home" || page === "search" || page === "radio" || page === "bandcamp" || page === "service" || page === "albums" || page === "queue" || (page === "playlist" && top && top.pageName === "playlist"))
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
        function onToggleMini() {
            if (miniPlayer.visible)
                window.showWindow()
            else
                window.enterMini()
        }
        function onSearch(query) {
            if (topBar.searchField.text !== query)
                topBar.searchField.text = query
            if (Nav.current !== "search")
                window.navigate("search", { query: query })
        }
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
    Component { id: bandcampPage; BandcampPage {} }
    Component { id: bandcampTagPage; BandcampTagPage {} }
    Component { id: servicePage; ServicePage {} }

    ColumnLayout {
        id: shell
        anchors.fill: parent
        spacing: 0

        TopBar {
            id: topBar
            Layout.fillWidth: true
            window: window
            canGoBack: stack.depth > 1
            onBackClicked: stack.pop()
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Sidebar {
                id: sidebar
                width: 248
                Layout.preferredWidth: width
                Layout.fillHeight: true

                // Drag the side pane's edge to make it wider or narrower.
                Item {
                    anchors.right: parent.right
                    anchors.rightMargin: -4
                    width: 8
                    height: parent.height
                    z: 10
                    HoverHandler { id: edgeHover; cursorShape: Qt.SizeHorCursor }
                    DragHandler {
                        id: edgeDrag
                        target: null
                        xAxis.enabled: true
                        yAxis.enabled: false
                        cursorShape: Qt.SizeHorCursor
                        property real startWidth: 0
                        onActiveChanged: if (active) startWidth = sidebar.width
                        onTranslationChanged: if (active) sidebar.width = Math.round(Math.max(180, Math.min(window.width * 0.45, startWidth + translation.x)))
                    }
                    Rectangle {
                        anchors.horizontalCenter: parent.horizontalCenter
                        width: 2
                        height: parent.height
                        color: Theme.accent
                        opacity: edgeDrag.active ? 0.8 : edgeHover.hovered ? 0.4 : 0
                        Behavior on opacity { NumberAnimation { duration: 120 } }
                    }
                }
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
            }
        }

        PlayerBar {
            Layout.fillWidth: true
        }
    }

    // YouTube's player, tucked behind everything (it has to stay in a shown
    // window, since browsers pause media in hidden pages).
    YouTubeEngine {
        id: youtube
        z: -1
        Component.onCompleted: Player.engine = youtube
    }

    ResizeEdges { window: window }

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
        title: "Import a playlist"
        message: "Paste a link to a YouTube or YouTube Music playlist, a SoundCloud playlist, album or song, or a Bandcamp album or song. Its songs are copied into a new playlist here."
        placeholder: "https://…"
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

    Shortcut { sequences: ["Ctrl+F", "Ctrl+L"]; onActivated: topBar.searchField.forceActiveFocus() }
    Shortcut { sequence: "Ctrl+Q"; context: Qt.ApplicationShortcut; onActivated: window.quitApp() }
    Shortcut { sequence: "Ctrl+M"; context: Qt.ApplicationShortcut; onActivated: Nav.toggleMini() }
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

    // Testing aids: --page=album:123 opens a page; --search=text [--search-tab=youtube] searches;
    // --play-video=ID plays one YouTube video; --play-track=JSON plays one song;
    // --screenshot=file.png saves the window and quits.
    Component.onCompleted: {
        restorePlace()
        const args = Qt.application.arguments
        for (let i = 0; i < args.length; ++i) {
            const arg = args[i]
            if (arg.startsWith("--page=")) {
                const spec = arg.substring(7).split(":")
                const pageArgs = spec.length > 1 ? (spec[0] === "chart" ? { kind: spec[1], id: spec[2] } : { id: spec.slice(1).join(":") }) : {}
                Qt.callLater(() => window.navigate(spec[0], pageArgs))
            } else if (arg.startsWith("--search=")) {
                const q = arg.substring(9)
                Qt.callLater(() => Nav.search(q))
            } else if (arg.startsWith("--search-tab=")) {
                const tab = arg.substring(13)
                Qt.callLater(() => { if (stack.currentItem && stack.currentItem.pageName === "search") stack.currentItem.tab = tab })
            } else if (arg.startsWith("--play-video=")) {
                const id = arg.substring(13)
                Qt.callLater(() => Player.playTracks([{ key: "yt:" + id, youtubeId: id, title: "Test video " + id, artist: "YouTube", duration: 0, cover:"https://i.ytimg.com/vi/" + id + "/hqdefault.jpg" }], 0, "Test"))
            } else if (arg.startsWith("--play-track=")) {
                const track = JSON.parse(arg.substring(13))
                Qt.callLater(() => Player.playTracks([track], 0, "Test"))
            } else if (arg.startsWith("--test-notification")) {
                testNotification.start()
            } else if (arg.startsWith("--play-first")) {
                autoPlay.start()
            } else if (arg.startsWith("--screenshot=")) {
                screenshot.path = arg.substring(13)
                screenshot.start()
            }
        }
    }

    Timer {
        id: testNotification
        interval: 3000
        onTriggered: nowPlaying.announce(Player.current || {
            key: "test", title: "Dragon Warrior: Overture March", artist: "Koichi Sugiyama", album: "Dragon Warrior (NES)", cover: ""
        })
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
            window.quitApp()
        })
    }
}
