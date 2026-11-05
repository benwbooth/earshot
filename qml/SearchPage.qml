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
    property bool youtubeLoading: false
    property string error: ""
    property string youtubeError: ""
    property int request: -1

    function firstTracks() { return results ? results.tracks : [] }

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
        if (tab === "youtube")
            runYoutube()
    }

    function runYoutube() {
        if (query === "" || youtubeLoading || youtube)
            return
        youtubeLoading = true
        const q = query
        Api.call(Backend.searchYoutube(q), function (found) {
            if (q !== page.query) return
            page.youtube = found.tracks
            page.youtubeLoading = false
        }, function (message) {
            page.youtubeLoading = false
            page.youtubeError = message
        })
    }

    onTabChanged: if (tab === "youtube") runYoutube()

    Component.onCompleted: {
        const initial = args.query || Api.lastSearch || ""
        field.text = initial
        if (initial !== "")
            run(initial)
        field.forceActiveFocus()
    }

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}

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

        TextField {
            id: field
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

        RowLayout {
            visible: page.query !== ""
            spacing: 8
            Repeater {
                model: [["all", "All"], ["songs", "Songs"], ["albums", "Albums"], ["artists", "Artists"], ["youtube", "YouTube"]]
                delegate: PillButton {
                    required property var modelData
                    text: modelData[1]
                    checkedStyle: page.tab === modelData[0]
                    onClicked: page.tab = modelData[0]
                }
            }
        }

        StatusView {
            loading: page.tab !== "youtube" && page.loading && !page.results
            error: page.tab !== "youtube" ? page.error : page.youtubeError
            empty: page.query === "" ? "Search for any song, album or artist. Earshot finds the music on YouTube when you press play."
                 : (page.tab !== "youtube" && page.results && page.results.tracks.length === 0 && page.results.albums.length === 0 ? "Nothing found for \"" + page.query + "\"" : "")
            emptyIcon: "search"
            onRetry: page.run(page.query)
        }
        StatusView {
            loading: page.tab === "youtube" && page.youtubeLoading
        }

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
                            elide: Text.ElideRight
                        }
                        Text {
                            text: topResult.showArtist ? "Artist" : (topResult.album ? "Album · " + topResult.album.artist : "")
                            color: Theme.textMuted
                            font.pixelSize: Theme.fontBody
                        }
                    }
                    IconButton {
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        anchors.margins: 20
                        filled: true
                        iconName: "play"
                        iconSize: 22
                        opacity: topHover.hovered ? 1 : 0
                        onClicked: topResult.showArtist ? Player.playTracks(page.results.tracks.filter(t => t.artistId === topResult.artist.id), 0, topResult.artist.name)
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

        SectionHeader {
            visible: page.tab === "all" && page.results && page.results.albums.length > 0
            title: "Albums"
            action: "SEE ALL"
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
    }
}
