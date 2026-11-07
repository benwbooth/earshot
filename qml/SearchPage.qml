import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

Flickable {
    id: page
    readonly property string pageName: "search"
    property var args: ({})
    property var results: null
    property var youtube: null
    property string query: ""
    property string tab: "all"
    property bool loading: false
    property string error: ""
    property string youtubeError: ""
    property int request: -1
    property var ytAlbums: null
    property bool ytAlbumsLoading: false
    // YouTube's results come a page at a time, like youtube.com's endless list.
    property string ytNext: ""
    property bool ytMoreLoading: false
    property int ytPages: 0
    readonly property int ytMaxPages: 15
    // Real YouTube results come first, in YouTube's own order; the music
    // catalogue (for album details and artist pages) follows.
    readonly property bool youtubeFirst: true
    // Tabs that show YouTube's playlists and full-length videos.
    readonly property bool youtubeTab: tab === "all" || tab === "albums" || tab === "youtube"

    component YoutubeAlbums: ColumnLayout {
        id: section
        required property var host
        Layout.fillWidth: true
        spacing: 20

        SectionHeader {
            visible: section.host.query !== "" && section.host.youtubeTab
                     && (section.host.ytAlbumsLoading || (section.host.ytAlbums && section.host.ytAlbums.length > 0))
            title: "On YouTube"
            action: section.host.tab === "all" ? "SEE ALL" : ""
            onActionClicked: section.host.tab = "youtube"
        }
        Text {
            visible: section.host.ytAlbumsLoading && section.host.youtubeTab
            text: "Looking for playlists and full-length videos…"
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }
        HorizontalShelf {
            id: ytShelf
            visible: section.host.youtubeTab && !!section.host.ytAlbums
            maxRows: section.host.tab === "all" ? (section.host.youtubeFirst ? 2 : 1) : -1
            Repeater {
                model: ytShelf.take(section.host.ytAlbums || [])
                delegate: AlbumCard {
                    required property var modelData
                    width: ytShelf.cardWidth
                    title: modelData.title
                    subtitle: section.host.ytAlbumSubtitle(modelData)
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }
        TrackList {
            visible: section.host.tab === "all" && !!section.host.youtube && section.host.youtube.length > 0
            Layout.fillWidth: true
            tracks: section.host.youtube || []
            limit: 8
            showHeader: false
            showAlbum: false
            showNumber: false
            context: "YouTube: " + section.host.query
        }
    }

    function firstTracks() { return results ? results.tracks : [] }

    // Bandcamp, SoundCloud, Mixcloud and the Internet Archive: { albums, tracks (, tags) } each, or null while loading.
    property var bandcamp: null
    property var soundcloud: null
    property var mixcloud: null
    property var archive: null
    property string bandcampError: ""
    property string soundcloudError: ""
    property string mixcloudError: ""
    property string archiveError: ""

    function runServices() {
        bandcamp = null
        soundcloud = null
        mixcloud = null
        archive = null
        bandcampError = ""
        soundcloudError = ""
        mixcloudError = ""
        archiveError = ""
        if (query === "")
            return
        const q = query
        Api.call(Backend.searchMixcloud(q), function (found) {
            if (q === page.query) page.mixcloud = found
        }, function (message) {
            if (q === page.query) { page.mixcloud = { albums: [], tracks: [] }; page.mixcloudError = message }
        })
        Api.call(Backend.searchArchive(q), function (found) {
            if (q === page.query) page.archive = found
        }, function (message) {
            if (q === page.query) { page.archive = { albums: [], tracks: [] }; page.archiveError = message }
        })
        Api.call(Backend.searchBandcamp(q), function (found) {
            if (q === page.query) page.bandcamp = found
        }, function (message) {
            if (q === page.query) { page.bandcamp = { albums: [], tracks: [], tags: [] }; page.bandcampError = message }
        })
        Api.call(Backend.searchSoundcloud(q), function (found) {
            if (q === page.query) page.soundcloud = found
        }, function (message) {
            if (q === page.query) { page.soundcloud = { albums: [], tracks: [] }; page.soundcloudError = message }
        })
    }

    // One service's results: a shelf of albums or playlists and a list of songs.
    component ServiceSection: ColumnLayout {
        id: service
        required property var host
        required property string key       // its tab
        required property string name      // "Bandcamp"
        required property var found
        property string failure: ""
        property var tags: []
        readonly property bool shown: host.query !== "" && (host.tab === "all" || host.tab === key)
        readonly property bool full: host.tab === key

        Layout.fillWidth: true
        spacing: 16
        visible: shown && (!found || failure !== "" || found.albums.length + found.tracks.length + tags.length > 0)

        SectionHeader {
            title: "On " + service.name
            action: service.full ? "" : "SEE ALL"
            onActionClicked: service.host.tab = service.key
        }
        Text {
            visible: !service.found
            text: "Searching " + service.name + "…"
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }
        Text {
            visible: service.failure !== ""
            Layout.fillWidth: true
            text: service.failure
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
            wrapMode: Text.Wrap
        }
        TagChips { tags: service.tags.slice(0, service.full ? 12 : 6) }
        HorizontalShelf {
            id: serviceShelf
            visible: !!service.found && service.found.albums.length > 0
            maxRows: service.full ? -1 : 1
            Repeater {
                model: serviceShelf.take(service.found ? service.found.albums : [])
                delegate: AlbumCard {
                    required property var modelData
                    width: serviceShelf.cardWidth
                    title: modelData.title
                    subtitle: modelData.artist + (modelData.kind === "playlist" ? " · Playlist" : "")
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }
        TrackList {
            visible: !!service.found && service.found.tracks.length > 0
            Layout.fillWidth: true
            tracks: service.found ? service.found.tracks : []
            limit: service.full ? -1 : 5
            showHeader: service.full
            showNumber: false
            context: service.name + ": " + service.host.query
        }
    }

    // Playlists and full-length videos (soundtracks, live sets) that work as albums.
    function runYoutubeAlbums() {
        ytAlbums = null
        ytNext = ""
        ytPages = 0
        ytMoreLoading = false
        if (query === "")
            return
        ytAlbumsLoading = true
        const q = query
        Api.call(Backend.searchYoutubeAlbums(q, "", Api.preferMusic), function (found) {
            if (q !== page.query) return
            page.ytAlbums = found.albums
            page.youtube = found.tracks
            page.ytNext = found.next || ""
            page.ytPages = 1
            page.ytAlbumsLoading = false
            // YouTube's first page is short; fetch the second straight away.
            page.loadMoreYoutube()
        }, function (message) {
            if (q !== page.query) return
            page.ytAlbums = []
            page.youtube = []
            page.youtubeError = message
            page.ytAlbumsLoading = false
        })
    }

    // The next page of YouTube results, added after what's shown (repeats skipped).
    function loadMoreYoutube() {
        if (ytNext === "" || ytMoreLoading || ytAlbumsLoading || ytPages >= ytMaxPages)
            return
        ytMoreLoading = true
        const q = query
        Api.call(Backend.searchYoutubeAlbums(q, ytNext, Api.preferMusic), function (found) {
            if (q !== page.query) return
            const seen = {}
            for (const a of page.ytAlbums) seen[a.id] = true
            for (const t of page.youtube) seen[t.key] = true
            page.ytAlbums = page.ytAlbums.concat(found.albums.filter(a => !seen[a.id]))
            page.youtube = page.youtube.concat(found.tracks.filter(t => !seen[t.key]))
            page.ytNext = found.next || ""
            page.ytPages += 1
            page.ytMoreLoading = false
            page.loadMoreIfNearEnd()
        }, function (message) {
            if (q !== page.query) return
            page.ytMoreLoading = false
            page.ytNext = ""
        })
    }

    // The YouTube and Albums tabs keep going, like youtube.com, as you near the end.
    function loadMoreIfNearEnd() {
        if ((tab === "youtube" || tab === "albums") && contentY + height > contentHeight - 1200)
            loadMoreYoutube()
    }
    onContentYChanged: loadMoreIfNearEnd()
    onTabChanged: loadMoreIfNearEnd()

    function ytAlbumSubtitle(album) {
        const by = album.artist ? album.artist + " · " : ""
        if (album.kind === "playlist")
            return by + "Playlist" + (album.trackCount > 0 ? " · " + album.trackCount + " videos" : "")
        return by + (album.trackCount > 1 ? album.trackCount + " tracks · " : "") + Theme.longDuration(album.duration)
    }

    function run(text) {
        query = text.trim()
        Api.lastSearch = query
        youtube = null
        youtubeError = ""
        if (query === "") {
            results = null
            return
        }
        loading = true
        error = ""
        const q = query
        Api.cancel(request)
        request = Api.call(Backend.search(q), function (found) {
            if (q !== page.query) return
            page.results = found
            page.loading = false
        }, function (message) {
            page.loading = false
            page.error = message
        })
        runYoutubeAlbums()
        runServices()
    }


    Component.onCompleted: {
        const initial = args.query || Api.lastSearch || ""
        field.text = initial
        if (initial !== "")
            run(initial)
    }

    Connections {
        target: Nav
        function onSearch(query) {
            field.text = query
            debounce.restart()
        }
    }

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    Timer {
        id: debounce
        interval: 450
        onTriggered: page.run(field.text)
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 40
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        // The query is typed in the top bar; this field only holds it.
        TextField {
            id: field
            visible: false
            Layout.fillWidth: true
            Layout.maximumWidth: 640
            implicitHeight: 52
            leftPadding: 52
            rightPadding: 44
            placeholderText: "Songs, albums or artists"
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.fontLarge
            selectByMouse: true
            onTextEdited: debounce.restart()
            onAccepted: { debounce.stop(); page.run(text) }
            background: Rectangle {
                radius: height / 2
                color: field.activeFocus ? Theme.surfaceRaised : Theme.surface
                border.color: field.activeFocus ? Theme.text : Theme.border
                border.width: field.activeFocus ? 2 : 1
            }
            Icon {
                anchors.left: parent.left
                anchors.leftMargin: 18
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: 20
                color: Theme.textMuted
            }
            IconButton {
                visible: field.text !== ""
                anchors.right: parent.right
                anchors.rightMargin: 8
                anchors.verticalCenter: parent.verticalCenter
                iconName: "close"
                iconSize: 16
                onClicked: { field.text = ""; page.run(""); field.forceActiveFocus() }
            }
        }

        Flow {
            visible: page.query !== ""
            Layout.fillWidth: true
            spacing: 8
            Repeater {
                model: [["all", "All"], ["songs", "Songs"], ["albums", "Albums"], ["artists", "Artists"], ["youtube", "YouTube"], ["bandcamp", "Bandcamp"], ["soundcloud", "SoundCloud"], ["mixcloud", "Mixcloud"], ["archive", "Internet Archive"]]
                delegate: PillButton {
                    required property var modelData
                    text: modelData[1]
                    checkedStyle: page.tab === modelData[0]
                    onClicked: page.tab = modelData[0]
                }
            }
        }

        StatusView {
            readonly property bool catalogTab: ["all", "songs", "albums", "artists"].indexOf(page.tab) >= 0
            loading: catalogTab && page.loading && !page.results && !page.ytAlbums
            error: page.tab === "youtube" ? page.youtubeError : catalogTab ? page.error : ""
            empty: page.query === "" ? "Search for any song, album or artist. Earshot finds the music on YouTube, Bandcamp and SoundCloud."
                 : (catalogTab && page.tab !== "all" && page.results && page.results.tracks.length === 0 && page.results.albums.length === 0 ? "Nothing found for \"" + page.query + "\"" : "")
            emptyIcon: "search"
            onRetry: page.run(page.query)
        }
        StatusView {
            loading: page.tab === "youtube" && page.ytAlbumsLoading
        }

        // Soundtrack-style searches show the YouTube albums first.
        YoutubeAlbums { host: page; visible: page.youtubeFirst }

        // Top result + songs
        RowLayout {
            visible: page.tab === "all" && page.results && page.results.tracks.length > 0
            Layout.fillWidth: true
            spacing: 24

            ColumnLayout {
                visible: page.width > 980
                Layout.fillWidth: false
                Layout.preferredWidth: 380
                Layout.maximumWidth: 380
                Layout.alignment: Qt.AlignTop
                spacing: 12
                SectionHeader { title: "Top result"; Layout.topMargin: 0 }
                Rectangle {
                    id: topResult
                    readonly property var album: page.results && page.results.albums.length > 0 ? page.results.albums[0] : null
                    readonly property var artist: page.results && page.results.artists.length > 0 ? page.results.artists[0] : null
                    readonly property bool showArtist: artist !== null && artist.name.toLowerCase() === page.query.toLowerCase()
                    Layout.fillWidth: true
                    implicitHeight: 240
                    radius: Theme.radiusLarge
                    color: topHover.hovered ? Theme.surfaceHover : Theme.surface
                    HoverHandler { id: topHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler {
                        onTapped: topResult.showArtist ? Nav.artist(topResult.artist.id) : (topResult.album ? Nav.album(topResult.album.id) : null)
                    }
                    ColumnLayout {
                        anchors.fill: parent
                        anchors.margins: 20
                        spacing: 10
                        Cover {
                            Layout.preferredWidth: 100
                            Layout.preferredHeight: 100
                            round: topResult.showArtist
                            radius: 8
                            source: topResult.showArtist ? topResult.artist.picture : (topResult.album ? topResult.album.cover : "")
                            seed: page.query
                        }
                        Item { Layout.fillHeight: true }
                        Text {
                            Layout.fillWidth: true
                            text: topResult.showArtist ? topResult.artist.name : (topResult.album ? topResult.album.title : "")
                            color: Theme.text
                            font.pixelSize: 28
                            font.weight: Font.Bold
                            wrapMode: Text.Wrap
                        }
                        Text {
                            text: topResult.showArtist ? "Artist" : (topResult.album ? "Album · " + topResult.album.artist : "")
                            color: Theme.textMuted
                            font.pixelSize: Theme.fontBody
                        }
                    }
                    IconButton {
                        // The artist's songs among the results, or the album; pause while they play.
                        readonly property var artistTracks: topResult.showArtist && page.results ? page.results.tracks.filter(t => t.artistId === topResult.artist.id) : []
                        readonly property bool playingThis: topResult.showArtist ? Player.playingFrom(topResult.artist.name, artistTracks)
                                                                                 : !!topResult.album && Player.playingAlbum(topResult.album.id)
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 20
                        filled: true
                        iconName: playingThis && Player.busy ? "pause" : "play"
                        iconSize: 22
                        tip: playingThis && Player.busy ? "Pause" : "Play"
                        opacity: topHover.hovered || playingThis ? 1 : 0
                        onClicked: playingThis ? Player.toggle()
                                   : topResult.showArtist ? Player.playTracks(artistTracks, 0, topResult.artist.name)
                                   : Api.playAlbum(topResult.album.id, topResult.album.title)
                    }
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.preferredWidth: 2000
                Layout.alignment: Qt.AlignTop
                spacing: 12
                SectionHeader {
                    title: "Songs"
                    Layout.topMargin: 0
                    action: "SEE ALL"
                    onActionClicked: page.tab = "songs"
                }
                TrackList {
                    Layout.fillWidth: true
                    tracks: page.results ? page.results.tracks : []
                    limit: 5
                    showHeader: false
                    showNumber: false
                    showAlbum: false
                    context: "Search: " + page.query
                }
            }
        }

        TrackList {
            visible: page.tab === "songs" && !!page.results
            Layout.fillWidth: true
            tracks: page.results ? page.results.tracks : []
            context: "Search: " + page.query
        }

        YoutubeAlbums { host: page; visible: !page.youtubeFirst }

        SectionHeader {
            visible: (page.tab === "all" || page.tab === "albums") && page.results && page.results.albums.length > 0
            title: page.tab === "albums" ? "Albums in the catalogue" : "Albums"
            action: page.tab === "all" ? "SEE ALL" : ""
            onActionClicked: page.tab = "albums"
        }
        HorizontalShelf {
            id: albumShelf
            visible: (page.tab === "all" || page.tab === "albums") && !!page.results
            maxRows: page.tab === "all" ? 1 : -1
            Repeater {
                model: albumShelf.take(page.results ? page.results.albums : [])
                delegate: AlbumCard {
                    required property var modelData
                    width: albumShelf.cardWidth
                    title: modelData.title
                    subtitle: (modelData.year ? modelData.year + " · " : "") + modelData.artist
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }

        SectionHeader {
            visible: page.tab === "all" && page.results && page.results.artists.length > 0
            title: "Artists"
            action: "SEE ALL"
            onActionClicked: page.tab = "artists"
        }
        HorizontalShelf {
            id: artistShelf
            visible: (page.tab === "all" || page.tab === "artists") && !!page.results
            maxRows: page.tab === "all" ? 1 : -1
            Repeater {
                model: artistShelf.take(page.results ? page.results.artists : [])
                delegate: AlbumCard {
                    required property var modelData
                    width: artistShelf.cardWidth
                    round: true
                    playable: false
                    placeholderIcon: "user"
                    title: modelData.name
                    subtitle: "Artist"
                    image: modelData.pictureBig || modelData.picture
                    seed: modelData.id
                    onClicked: Nav.artist(modelData.id)
                }
            }
        }

        Text {
            visible: page.tab === "youtube" && page.youtube !== null
            text: "Straight from YouTube: videos, live sets, covers and remixes that aren't in the catalogue."
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }
        TrackList {
            visible: page.tab === "youtube" && page.youtube !== null
            Layout.fillWidth: true
            tracks: page.youtube || []
            showAlbum: false
            context: "YouTube: " + page.query
        }

        Text {
            visible: (page.tab === "youtube" || page.tab === "albums") && page.ytMoreLoading
            text: "Loading more from YouTube…"
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }

        ServiceSection {
            host: page
            key: "bandcamp"
            name: "Bandcamp"
            found: page.bandcamp
            failure: page.bandcampError
            tags: page.bandcamp && page.bandcamp.tags ? page.bandcamp.tags : []
        }
        ServiceSection {
            host: page
            key: "soundcloud"
            name: "SoundCloud"
            found: page.soundcloud
            failure: page.soundcloudError
        }
        ServiceSection {
            host: page
            key: "mixcloud"
            name: "Mixcloud"
            found: page.mixcloud
            failure: page.mixcloudError
        }
        ServiceSection {
            host: page
            key: "archive"
            name: "the Internet Archive"
            found: page.archive
            failure: page.archiveError
        }
    }
}
