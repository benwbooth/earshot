import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtCore
import Qt.labs.platform as Platform
import Earshot
import GlobalMenu

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
        // The normal (not maximised) size, noted while the window is a normal one.
        property int width: 1320
        property int height: 860
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
        if (windowSettings.width >= minimumWidth && windowSettings.height >= minimumHeight) {
            window.width = windowSettings.width
            window.height = windowSettings.height
        }
        if (wayland) {
            // (Every start: it also brings an earlier version's rule up to date.
            // KWin puts the window back where it was and maximises it if it was.)
            if (Backend.desktop() === "kde" && Backend.rememberWindowWithKde() && !windowSettings.askedKde)
                Nav.notify("KDE will now remember where Earshot's window is (System Settings › Window Rules)")
            windowSettings.askedKde = true
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
        if (!window.visible || window.visibility === Window.Minimized || window.visibility === Window.Hidden)
            return
        windowSettings.maximized = window.visibility === Window.Maximized
        if (window.visibility === Window.Windowed) {
            // (While maximising or leaving full screen the size passes through
            // the screen's; by now it has settled.)
            windowSettings.width = window.width
            windowSettings.height = window.height
            if (!wayland) {
                windowSettings.x = window.x
                windowSettings.y = window.y
            }
        }
        windowSettings.sync()
    }
    onXChanged: placeTimer.restart()
    onYChanged: placeTimer.restart()
    onWidthChanged: placeTimer.restart()
    onHeightChanged: placeTimer.restart()
    onVisibilityChanged: placeTimer.restart()
    // The menu's address goes on the window's surface, which exists only while
    // the window is shown.
    onVisibleChanged: if (visible) Qt.callLater(globalMenu.attach)
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
        saveView()
        Player.closeSession()
        quitting = true
        miniPlayer.close()
        Qt.quit()
    }

    function showWindow() {
        miniPlayer.hide()
        if (window.visibility === Window.Minimized)
            window.showNormal()
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
        // The now-playing card, as chosen in the settings menu.
        function onTrackStarted(track) {
            if (Api.nowPlayingCard === "always"
                    || (Api.nowPlayingCard === "background" && !window.active && !miniPlayer.active))
                nowPlaying.announce(track)
        }
    }

    // Short notices ("Added to playlist", errors): inside the window, or as the
    // desktop's own notifications when that's turned on in the settings menu
    // (through the tray; Earshot's own card where there's no tray).
    function notice(message) {
        if (!Api.desktopNotices)
            toast.show(message)
        else if (tray.available && tray.supportsMessages)
            tray.showMessage("Earshot", message, Platform.SystemTrayIcon.NoIcon, 4000)
        else
            nowPlaying.notify(message)
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
        home: forYouPage, trending: homePage, search: searchPage, radio: radioPage, chart: chartPage, album: albumPage,
        artist: artistPage, playlist: playlistPage, albums: albumsPage, queue: queuePage, history: historyPage,
        bandcamp: bandcampPage, "bandcamp-tag": bandcampTagPage, service: servicePage,
        // Made for you is Home now; "foryou" is kept for places saved before.
        foryou: forYouPage
    })

    function navigate(page, args) {
        const component = pages[page]
        if (!component)
            return
        args = args || {}
        const top = stack.currentItem
        if (top && top.pageName === page && JSON.stringify(top.args) === JSON.stringify(args))
            return
        // What was saved for the pages being replaced or covered no longer applies.
        pendingViews = pendingViews.slice(0, stack.depth)
        if (page === "home" || page === "search" || page === "radio" || page === "bandcamp" || page === "service" || page === "foryou" || page === "trending" || page === "albums" || page === "queue" || page === "history" || (page === "playlist" && top && top.pageName === "playlist"))
        {
            pendingViews = []
            stack.replace(null, component, { args: args })
        } else
            stack.push(component, { args: args })
    }

    function syncNav() {
        const top = stack.currentItem
        if (top) {
            Nav.currentArgs = top.args || {}
            Nav.current = top.pageName
            window.restoreView(top)
        }
        placeSave.restart()
    }

    // ---------------------------------------- where you were, from one run to the next
    // The pages open (the one shown and the ones Back returns to), each with its
    // tab, sort or mix and how far down it was scrolled. Saved a moment after
    // you go somewhere or scroll, every half minute and on quitting, so even a
    // forced quit (or dev.sh restarting the app) keeps it. Not kept when the
    // testing aids open a page or take a picture.
    Settings { id: viewSettings; category: "location" }
    property bool keepView: true
    // A page's own choices worth keeping, where it has them.
    readonly property var viewProperties: ["tab", "sort", "discography", "slice", "selected", "newestFirst", "openSection"]
    // Scroll and choices still to put back, by place in the stack (pages under the
    // top one are only made when Back reaches them).
    property var pendingViews: []
    property bool viewRestored: false

    function pageState(item) {
        const args = Object.assign({}, item.args || {})
        if (item.pageName === "search" && item.query !== undefined)
            args.query = item.query
        const state = {}
        for (const name of viewProperties)
            if (item[name] !== undefined && typeof item[name] !== "function")
                state[name] = item[name]
        return { page: item.pageName, args: args, state: state, y: Math.round(item.contentY - item.originY) }
    }

    function saveView() {
        if (!keepView || !viewRestored)
            return
        const pages = []
        for (let i = 0; i < stack.depth; ++i) {
            const item = stack.get(i, StackView.DontLoad)
            // A page Back hasn't reached yet still has what was saved for it.
            pages.push(item ? pageState(item) : (pendingViews[i] || null))
        }
        viewSettings.setValue("pages", JSON.stringify(pages.filter(p => p)))
        viewSettings.sync()
    }

    function restoreSavedView() {
        let pages = []
        try { pages = JSON.parse(viewSettings.value("pages", "[]")) } catch (e) { pages = [] }
        pages = (pages || []).filter(p => p && window.pages[p.page])
        if (pages.length === 0 || (pages.length === 1 && pages[0].page === "home" && !pages[0].y && !pages[0].args.mix)) {
            viewRestored = true
            return
        }
        pendingViews = pages
        const items = []
        for (const p of pages)
            items.push(window.pages[p.page], { args: p.args || {} })
        stack.replace(null, items)
        viewRestored = true
    }

    // Puts back a page's choices, then its scroll once there's that much to
    // scroll through (its songs and pictures arrive a moment later).
    function restoreView(item) {
        const index = item.StackView.index
        const saved = pendingViews[index]
        if (!saved || saved.page !== item.pageName)
            return
        pendingViews[index] = null
        for (const name in saved.state || {})
            if (item[name] !== undefined && item[name] !== saved.state[name])
                item[name] = saved.state[name]
        if (saved.y > 0)
            scrollRestore.begin(item, saved.y)
    }

    Timer {
        id: scrollRestore
        property var item: null
        property real y: 0
        property int tries: 0
        property real lastY: 0
        interval: 100
        repeat: true
        function begin(target, to) {
            item = target
            y = to
            tries = 0
            lastY = target.contentY
            restart()
        }
        onTriggered: {
            // Stop when the page is gone, after ten seconds, or once you scroll yourself.
            if (!item || stack.currentItem !== item || ++tries > 100 || Math.abs(item.contentY - lastY) > 1) {
                stop()
                return
            }
            const most = item.contentHeight - item.height
            item.contentY = item.originY + Math.max(0, Math.min(y, most))
            lastY = item.contentY
            if (most >= y)
                stop()
        }
    }

    Connections { target: Qt.application; function onAboutToQuit() { window.saveView() } }
    Timer { id: placeSave; interval: 1000; onTriggered: window.saveView() }
    Timer { interval: 30000; repeat: true; running: window.keepView; onTriggered: window.saveView() }
    Connections {
        target: stack.currentItem
        ignoreUnknownSignals: true
        function onContentYChanged() { if (!scrollRestore.running) placeSave.restart() }
        function onTabChanged() { placeSave.restart() }
        function onSortChanged() { placeSave.restart() }
        function onSelectedChanged() { placeSave.restart() }
        function onQueryChanged() { placeSave.restart() }
    }

    Connections {
        target: Nav
        function onGo(page, args) { window.navigate(page, args) }
        function onBack() { if (stack.depth > 1) stack.pop() }
        function onToast(message) { window.notice(message) }
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
    Component { id: historyPage; HistoryPage {} }
    Component { id: bandcampPage; BandcampPage {} }
    Component { id: bandcampTagPage; BandcampTagPage {} }
    Component { id: servicePage; ServicePage {} }
    Component { id: forYouPage; ForYouPage {} }

    ColumnLayout {
        id: shell
        anchors.fill: parent
        // A thousandth of a pixel down. At 130% scaling, text whose position
        // lands exactly on a half device pixel (any y ending in 5) has its
        // letters rounded up or down one by one, so round ones like e and o
        // sit a pixel below the rest. Off the exact half, they all agree.
        anchors.topMargin: 0.001
        spacing: 0

        TopBar {
            id: topBar
            Layout.fillWidth: true
            window: window
            canGoBack: stack.depth > 1
            onBackClicked: stack.pop()
            menuShown: appMenu.visible || settingsMenu.visible
            onMenuClicked: appMenu.openBelow(topBar.menuButton)
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
                    initialItem: forYouPage
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

    ArtViewer { id: artViewer }
    Connections {
        target: Nav
        function onShowArt(url) { artViewer.show(url) }
    }

    // A notice inside the window, just above the player bar, for a few seconds.
    Rectangle {
        id: toast
        property string message
        function show(text) {
            message = text
            opacity = 1
            toastTimer.restart()
        }
        anchors.horizontalCenter: shell.horizontalCenter
        anchors.bottom: shell.bottom
        anchors.bottomMargin: 110
        width: Math.min(toastText.implicitWidth + 32, shell.width - 32)
        height: toastText.implicitHeight + 20
        radius: 10
        color: Theme.surfaceRaised
        border.color: Theme.border
        opacity: 0
        visible: opacity > 0
        z: 10
        Behavior on opacity { NumberAnimation { duration: 180 } }
        Text {
            id: toastText
            anchors.centerIn: parent
            width: Math.min(implicitWidth, toast.parent.width - 64)
            text: toast.message
            color: Theme.text
            font.pixelSize: Theme.fontBody
            wrapMode: Text.Wrap
        }
        Timer { id: toastTimer; interval: 4000; onTriggered: toast.opacity = 0 }
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

    // ---------------------------------------- menus
    // Defined once: exported to the desktop's global menu bar (GlobalMenuBar:
    // hearth's top bar, Plasma's Global Menu) for this window and the mini
    // player, and shown by the title bar's menu button (AppMenu). Shortcuts
    // are only shown there: the Shortcuts below do the work.
    readonly property bool liked: Player.current ? (Backend.libraryRevision, Backend.isLiked(Player.current.key)) : false
    readonly property string songPage: Player.current ? Api.songPage(Player.current) : ""
    // Whether this page is the one shown (as the side pane marks it).
    function showing(page, id) {
        return Nav.current === page && ((page !== "playlist" && page !== "service") || Nav.currentArgs.id === id)
    }
    // The settings (Ctrl+, opens just these, below the menu button).
    readonly property var settingsItems: [
        { id: "card-heading", text: "Now-Playing Card When a Song Starts:", enabled: false },
        { id: "card-always", text: "Always", checkable: true, checked: Api.nowPlayingCard === "always" },
        { id: "card-background", text: "Only When Earshot Isn't in Front", checkable: true, checked: Api.nowPlayingCard === "background" },
        { id: "card-never", text: "Never", checkable: true, checked: Api.nowPlayingCard === "never" },
        // Off: "Added to playlist" and the like show inside the window instead.
        { id: "desktop-notices", text: "Notices as Desktop Notifications", checkable: true, checked: Api.desktopNotices },
        { separator: true },
        // Turns loud songs down and quiet ones up, so they all sound about as loud.
        { id: "leveling", text: "Level the Volume Between Songs", checkable: true, checked: Player.leveling },
        // Leaves songs marked explicit out of song lists, the queue and radio.
        { id: "hide-explicit", text: "Hide Explicit Songs", checkable: true, checked: Api.hideExplicit },
        // Songs, music videos and albums before other videos in YouTube searches (nothing is hidden).
        { id: "prefer-music", text: "Prefer Music in YouTube Results", checkable: true, checked: Api.preferMusic },
        { separator: true },
        // The same Earshot in a browser on this computer, sharing this window's playlists.
        { id: "web-version", text: "Open the Web Version" }
    ]
    readonly property var userPlaylists: sidebar.playlists.filter(p => !p.special)
    readonly property var menuModel: [
        { title: "Earshot", items: [
            { id: "about", text: "About Earshot" },
            { id: "settings", text: "Settings…", shortcut: "Ctrl+," },
            { separator: true }
        ].concat(window.settingsItems, [
            { separator: true },
            { id: "hide", text: "Hide Earshot", enabled: tray.available && window.visible },
            { id: "quit", text: "Quit Earshot", shortcut: "Ctrl+Q" } ]) },
        { title: "File", items: [
            { id: "new-playlist", text: "New Playlist…", shortcut: "Ctrl+N" },
            { id: "import-playlist", text: "Import a Playlist…" },
            { separator: true },
            { id: "close", text: "Close Window", enabled: window.visible } ] },
        { title: "Edit", items: [
            { id: "find", text: "Search…", shortcut: "Ctrl+F" },
            { id: "clear-search", text: "Clear Search", enabled: topBar.searchField.text !== "" },
            { id: "clear-history", text: "Clear Recent Searches", enabled: topBar.historyItems.length > 0 } ] },
        { title: "View", items: [
            { id: "back", text: "Back", shortcut: "Alt+Left", enabled: stack.depth > 1 },
            { separator: true },
            { id: "go-home", text: "Home", checkable: true, checked: window.showing("home") },
            { id: "go-search", text: "Search", checkable: true, checked: window.showing("search") },
            { id: "go-trending", text: "Trending", checkable: true, checked: window.showing("trending") },
            { id: "go-youtube", text: "YouTube", checkable: true, checked: window.showing("service", "youtube") },
            { id: "go-radio", text: "Reddit Radio", checkable: true, checked: window.showing("radio") },
            { id: "go-bandcamp", text: "Bandcamp", checkable: true, checked: window.showing("bandcamp") },
            { id: "go-soundcloud", text: "SoundCloud", checkable: true, checked: window.showing("service", "soundcloud") },
            { id: "go-mixcloud", text: "Mixcloud", checkable: true, checked: window.showing("service", "mixcloud") },
            { id: "go-archive", text: "Internet Archive", checkable: true, checked: window.showing("service", "archive") },
            { id: "go-queue", text: "Queue", checkable: true, checked: window.showing("queue") },
            { separator: true },
            { id: "go-albums", text: "Albums", checkable: true, checked: window.showing("albums") },
            { id: "go-liked", text: "Liked Songs", checkable: true, checked: window.showing("playlist", "liked") },
            { id: "go-history", text: "History", checkable: true, checked: window.showing("history") },
            { id: "playlists", text: "Playlists", enabled: window.userPlaylists.length > 0, items: window.userPlaylists.length > 0
                ? window.userPlaylists.map(p => ({ id: "playlist-" + p.id, text: p.name, checkable: true, checked: window.showing("playlist", p.id) }))
                : [{ id: "playlist-none", text: "No Playlists", enabled: false }] },
            { separator: true },
            { id: "album-list", text: "Show Albums as a List", checkable: true, checked: Api.albumList } ] },
        { title: "Controls", items: [
            { id: "play", text: Player.playing ? "Pause" : "Play", shortcut: "Space", enabled: Player.current !== null },
            { id: "next", text: "Next", shortcut: "Ctrl+Right", enabled: Player.hasNext },
            { id: "previous", text: "Previous", shortcut: "Ctrl+Left", enabled: Player.current !== null },
            { separator: true },
            { id: "shuffle", text: "Shuffle", shortcut: "Ctrl+S", checkable: true, checked: Player.shuffle },
            { id: "repeat", text: "Repeat", items: [
                { id: "repeat-0", text: "Off", checkable: true, checked: Player.repeat === 0 },
                { id: "repeat-1", text: "All", checkable: true, checked: Player.repeat === 1 },
                { id: "repeat-2", text: "This Song", checkable: true, checked: Player.repeat === 2 } ] },
            { id: "next-repeat", text: "Change Repeat", shortcut: "Ctrl+R" },
            // Keep playing songs like these when the queue runs out.
            { id: "radio", text: "Random Radio", checkable: true, checked: Player.radio },
            { separator: true },
            { id: "mute", text: "Mute", checkable: true, checked: Player.muted },
            { separator: true },
            { id: "like", text: window.liked ? "Remove from Liked Songs" : "Add to Liked Songs", enabled: Player.current !== null },
            { id: "source", text: Player.context !== "" ? "Go to " + Player.context : "Go to the Song's Album", enabled: Player.current !== null },
            { id: "artist", text: "Go to Artist", enabled: Player.current !== null && !!Player.current.artistId },
            { id: "open-on", text: window.songPage !== "" ? "Open on " + Api.siteName(window.songPage) : "Open on the Web", enabled: window.songPage !== "" } ] },
        { title: "Window", items: [
            { id: "minimize", text: "Minimize", enabled: window.visible },
            { id: "zoom", text: "Zoom", enabled: window.visible },
            { separator: true },
            { id: "mini", text: "Mini Player", shortcut: "Ctrl+M", checkable: true, checked: miniPlayer.visible } ] },
        { title: "Help", items: [
            { id: "website", text: "Earshot on GitHub" } ] }
    ]

    // The full window in front (for menu items chosen from the mini player).
    function front() {
        if (!window.visible || miniPlayer.visible)
            window.showWindow()
    }
    function openSettings() {
        front()
        settingsMenu.openBelow(topBar.menuButton)
    }
    function menuAction(id) {
        if (id.startsWith("playlist-")) { front(); Nav.playlist(id.slice(9)); return }
        if (id.startsWith("repeat-")) {
            // Repeat goes round off → all → this song.
            for (let i = 0; i < 3 && Player.repeat !== Number(id.slice(7)); ++i)
                Player.cycleRepeat()
            return
        }
        if (id.startsWith("go-")) {
            const page = id.slice(3)
            const services = ["youtube", "soundcloud", "mixcloud", "archive"]
            front()
            if (services.indexOf(page) >= 0) Nav.open("service", { id: page })
            else if (page === "liked") Nav.playlist("liked")
            else Nav.open(page)
            return
        }
        switch (id) {
        case "about": front(); aboutDialog.open(); break
        case "settings": openSettings(); break
        case "card-always": Api.nowPlayingCard = "always"; break
        case "card-background": Api.nowPlayingCard = "background"; break
        case "card-never": Api.nowPlayingCard = "never"; break
        case "desktop-notices": Api.desktopNotices = !Api.desktopNotices; break
        case "leveling": Player.leveling = !Player.leveling; break
        case "hide-explicit": Api.hideExplicit = !Api.hideExplicit; break
        case "prefer-music": Api.preferMusic = !Api.preferMusic; break
        case "web-version":
            Api.call(Backend.openWebUi(), function (result) {
                Api.openWeb(result.url)
                Nav.notify("The web version is open in your browser at " + result.url)
            }, function (message) { Nav.notify(message) })
            break
        case "hide": window.hide(); break
        case "quit": window.quitApp(); break
        case "new-playlist": front(); Nav.askNewPlaylist([]); break
        case "import-playlist": front(); Nav.askImportPlaylist(); break
        case "close": window.close(); break
        case "find": front(); topBar.searchField.forceActiveFocus(); break
        case "clear-search": topBar.searchField.text = ""; Nav.search(""); break
        case "clear-history": topBar.clearHistory(); break
        case "back": front(); if (stack.depth > 1) stack.pop(); break
        case "album-list": Api.albumList = !Api.albumList; break
        case "play": Player.toggle(); break
        case "next": Player.next(false); break
        case "previous": Player.previous(); break
        case "shuffle": Player.setShuffle(!Player.shuffle); break
        case "next-repeat": Player.cycleRepeat(); break
        case "radio": Player.radio = !Player.radio; break
        case "mute": Player.muted = !Player.muted; break
        case "like": if (Player.current) Backend.setLiked(JSON.stringify(Player.current), !window.liked); break
        case "source": front(); Player.openSource(); break
        case "artist": if (Player.current && Player.current.artistId) { front(); Nav.artist(Player.current.artistId) } break
        case "open-on": if (window.songPage !== "") Api.openWeb(window.songPage); break
        case "minimize": window.showMinimized(); break
        case "zoom": topBar.toggleMaximized(); break
        case "mini": Nav.toggleMini(); break
        case "website": Qt.openUrlExternally("https://github.com/benwbooth/earshot"); break
        }
    }

    GlobalMenuBar {
        id: globalMenu
        window: window
        model: window.menuModel
        onTriggered: id => window.menuAction(id)
    }
    // The mini player is a window of its own: the same menus there.
    GlobalMenuBar {
        id: miniMenu
        window: miniPlayer
        model: window.menuModel
        onTriggered: id => window.menuAction(id)
    }
    Connections {
        target: miniPlayer
        function onVisibleChanged() { if (miniPlayer.visible) Qt.callLater(miniMenu.attach) }
    }

    // Earshot's look for menu items: a check for toggles, the key on the right.
    Component {
        id: earshotMenuItem
        MenuItem {
            id: menuItem
            property string shortcutText
            implicitHeight: 34
            // The check is drawn below, in Earshot's colour.
            indicator: Item {}
            contentItem: RowLayout {
                spacing: 8
                Icon {
                    name: "check"
                    size: 14
                    color: Theme.accent
                    opacity: menuItem.checked ? 1 : 0
                }
                Text {
                    Layout.fillWidth: true
                    text: menuItem.text
                    color: menuItem.enabled ? Theme.text : Theme.textMuted
                    font.pixelSize: Theme.fontBody
                }
                Text {
                    visible: menuItem.shortcutText !== ""
                    Layout.leftMargin: 16
                    text: menuItem.shortcutText
                    color: Theme.textFaint
                    font.pixelSize: Theme.fontSmall + 1
                }
                // A row that opens a submenu: room for the arrow.
                Item { visible: !!menuItem.subMenu; implicitWidth: 14 }
            }
            arrow: Text {
                visible: !!menuItem.subMenu
                x: menuItem.width - width - 14
                y: Math.round((menuItem.height - height) / 2)
                text: "›"
                color: menuItem.enabled ? Theme.textMuted : Theme.textFaint
                font.pixelSize: Theme.fontBody + 3
            }
            background: Rectangle { color: menuItem.highlighted && menuItem.enabled ? Theme.surfaceHover : "transparent"; radius: 6; anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6 }
        }
    }
    // Submenus and separators in the same look (the settings menu's).
    Component {
        id: earshotSubMenu
        Menu {
            topPadding: 6
            bottomPadding: 6
            delegate: earshotMenuItem
            background: Rectangle { implicitWidth: 180; color: Theme.surfaceRaised; radius: Theme.radius; border.color: Theme.border }
        }
    }
    Component { id: earshotSeparator; MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: Theme.border } } }
    AppMenu {
        id: appMenu
        model: window.menuModel
        onTriggered: id => window.menuAction(id)
        itemDelegate: earshotMenuItem
        delegate: earshotMenuItem
        submenuDelegate: earshotSubMenu
        separatorDelegate: earshotSeparator
        topPadding: 6
        bottomPadding: 6
        background: Rectangle { implicitWidth: 180; color: Theme.surfaceRaised; radius: Theme.radius; border.color: Theme.border }
        onClosed: topBar.menuClosedAt = Date.now()
    }
    // Ctrl+,: the settings alone, as the Earshot menu has them.
    AppMenu {
        id: settingsMenu
        flat: true
        model: [{ title: "Settings", items: window.settingsItems }]
        onTriggered: id => window.menuAction(id)
        itemDelegate: earshotMenuItem
        delegate: earshotMenuItem
        submenuDelegate: earshotSubMenu
        separatorDelegate: earshotSeparator
        implicitWidth: 300
        topPadding: 6
        bottomPadding: 6
        background: Rectangle { color: Theme.surfaceRaised; radius: Theme.radius; border.color: Theme.border }
        onClosed: topBar.menuClosedAt = Date.now()
    }

    Popup {
        id: aboutDialog
        modal: true
        focus: true
        anchors.centerIn: Overlay.overlay
        padding: 28
        closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
        background: Rectangle { color: Theme.surfaceRaised; radius: Theme.radiusLarge; border.color: Theme.border }
        contentItem: ColumnLayout {
            spacing: 10
            Image {
                Layout.alignment: Qt.AlignHCenter
                source: Theme.icon("earshot-app")
                sourceSize: Qt.size(128, 128)
                Layout.preferredWidth: 64
                Layout.preferredHeight: 64
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "Earshot " + Qt.application.version
                color: Theme.text
                font.pixelSize: Theme.fontTitle - 2
                font.weight: Font.Bold
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "A music player for YouTube, Bandcamp, SoundCloud, Mixcloud and more."
                color: Theme.textMuted
                font.pixelSize: Theme.fontBody
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "<a href='https://github.com/benwbooth/earshot' style='color:" + Theme.accent + "'>github.com/benwbooth/earshot</a>"
                textFormat: Text.RichText
                font.pixelSize: Theme.fontBody
                onLinkActivated: link => Qt.openUrlExternally(link)
                HoverHandler { cursorShape: Qt.PointingHandCursor }
            }
            PillButton {
                Layout.alignment: Qt.AlignHCenter
                Layout.topMargin: 8
                text: "Close"
                onClicked: aboutDialog.close()
            }
        }
    }

    Shortcut { sequences: ["Ctrl+F", "Ctrl+L"]; onActivated: topBar.searchField.forceActiveFocus() }
    Shortcut { sequence: "Ctrl+Q"; context: Qt.ApplicationShortcut; onActivated: window.quitApp() }
    Shortcut { sequence: "Ctrl+,"; onActivated: window.openSettings() }
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
    // --settings-menu / --app-menu open those menus;
    // --screenshot=file.png saves the window and quits (--keep-view: from the saved page, and saving it).
    Component.onCompleted: {
        restorePlace()
        const args = Qt.application.arguments
        // The testing aids choose the page themselves (and leave the saved one alone).
        window.keepView = args.includes("--keep-view") || !args.some(a => /^--(page|search|screenshot|play-first)/.test(a))
        if (window.keepView)
            restoreSavedView()
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
                Qt.callLater(() => Player.playTracks([track], 0, track.album || "Test"))
            } else if (arg.startsWith("--web-version")) {
                Api.call(Backend.openWebUi(), result => console.info("Web version at", result.url), message => console.warn(message))
            } else if (arg.startsWith("--settings-menu")) {
                Qt.callLater(() => window.openSettings())
            } else if (arg.startsWith("--app-menu")) {
                Qt.callLater(() => appMenu.openBelow(topBar.menuButton))
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
