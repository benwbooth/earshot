import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Made for you: Discover Weekly, Daily Mixes, Release Radar and albums
// picked from what you listen to in Earshot.
Flickable {
    id: page
    readonly property string pageName: "foryou"
    property var args: ({})
    property var made: Api.forYouCache
    property bool loading: false
    property string error: ""
    property string selected: "discover-weekly"

    readonly property var mixes: made && !made.needMore ? [made.weekly].concat(made.daily || []).filter(m => m && m.tracks && m.tracks.length > 0) : []
    readonly property var mix: mixes.find(m => m.id === selected) || (mixes.length > 0 ? mixes[0] : null)
    readonly property var tints: ["#7a5cff", "#ff5f7e", "#3db8ff", "#3ddc97"]

    function firstTracks() { return mix ? mix.tracks : [] }

    function load(refresh) {
        loading = true
        error = ""
        Api.call(Backend.forYou(refresh), function (result) {
            Api.forYouCache = result
            page.made = result
            page.loading = false
        }, function (message) {
            page.loading = false
            page.error = message
        })
    }

    // Until there's enough history, look again on every visit and after each song counted.
    Component.onCompleted: if (!made || made.needMore) load(false)
    Connections {
        target: Backend
        function onLibraryRevisionChanged() {
            if (page.made && page.made.needMore && !page.loading)
                page.load(false)
        }
    }

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    Rectangle {
        width: parent.width
        height: 340
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.alpha(Theme.accent2, 0.45) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 56
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        RowLayout {
            Layout.fillWidth: true
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: "MADE FOR YOU"
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.5
                }
                Text {
                    Layout.fillWidth: true
                    text: "From what you listen to"
                    color: Theme.text
                    font.pixelSize: 48
                    font.weight: Font.Black
                    font.letterSpacing: -1
                    wrapMode: Text.Wrap
                }
                Text {
                    Layout.fillWidth: true
                    text: "Mixes of new songs like your favourites, new releases from your artists and albums you might like. Your listening history stays on this computer."
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontBody + 1
                    wrapMode: Text.Wrap
                }
            }
            PillButton {
                visible: !!page.made && !page.made.needMore
                text: page.loading ? "Making…" : "Make new mixes"
                iconName: "dice"
                enabled: !page.loading
                Layout.alignment: Qt.AlignBottom
                onClicked: page.load(true)
            }
        }

        StatusView {
            loading: page.loading && !page.made
            error: page.error
            empty: page.made && page.made.needMore
                   ? "Play a song for half a minute, or like one with the heart, and Earshot will make mixes from what you like."
                   : ""
            emptyIcon: "heart"
            onRetry: page.load(false)
        }
        Text {
            visible: page.loading && !page.made
            text: "Making your mixes: finding new songs like your favourites…"
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }

        // The mixes: pick one to see and play its songs.
        GridLayout {
            visible: page.mixes.length > 0
            Layout.fillWidth: true
            columns: Math.max(1, Math.min(4, Math.floor(width / 260)))
            columnSpacing: 16
            rowSpacing: 16
            Repeater {
                model: page.mixes
                delegate: TileCard {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    implicitHeight: 110
                    title: modelData.title
                    subtitle: modelData.tracks.length + " songs"
                    image: modelData.tracks.length > 0 ? (modelData.tracks[0].coverBig || modelData.tracks[0].cover || "") : ""
                    tint: page.tints[index % page.tints.length]
                    border.width: page.mix && page.mix.id === modelData.id ? 2 : 0
                    border.color: Theme.text
                    onClicked: page.selected = modelData.id
                }
            }
        }

        ColumnLayout {
            visible: !!page.mix
            Layout.fillWidth: true
            spacing: 12
            SectionHeader { title: page.mix ? page.mix.title : "" }
            Text {
                Layout.fillWidth: true
                text: page.mix ? page.mix.subtitle : ""
                color: Theme.textMuted
                font.pixelSize: Theme.fontBody
                wrapMode: Text.Wrap
            }
            RowLayout {
                spacing: 14
                IconButton {
                    filled: true
                    iconName: page.mix && Player.playingFrom(page.mix.title, page.mix.tracks) && Player.playing ? "pause" : "play"
                    iconSize: 22
                    tip: "Play"
                    onClicked: Player.playOrToggle(page.mix.tracks, page.mix.title, { page: "foryou", args: {} })
                }
                PillButton { text: "Shuffle"; iconName: "shuffle"; onClicked: Player.shuffleTracks(page.mix.tracks, page.mix.title, { page: "foryou", args: {} }) }
                PillButton {
                    text: "Save as playlist"
                    iconName: "plus"
                    onClicked: {
                        Backend.createPlaylist(page.mix.title + " · " + new Date().toLocaleDateString(Qt.locale(), "MMM d"), JSON.stringify(page.mix.tracks))
                        Nav.notify("Saved " + page.mix.tracks.length + " songs as a playlist")
                    }
                }
            }
            TrackList {
                Layout.fillWidth: true
                tracks: page.mix ? page.mix.tracks : []
                showAlbum: false
                context: page.mix ? page.mix.title : ""
            }
        }

        // New releases and albums by related artists.
        Repeater {
            model: page.made && !page.made.needMore ? [page.made.radar, page.made.albums].filter(s => s && s.albums) : []
            delegate: ColumnLayout {
                id: albumSection
                required property var modelData
                Layout.fillWidth: true
                Layout.topMargin: 16
                spacing: 12
                SectionHeader { title: albumSection.modelData.title }
                Text {
                    Layout.fillWidth: true
                    text: albumSection.modelData.albums.length > 0 ? albumSection.modelData.subtitle
                          : (albumSection.modelData.id === "release-radar" ? "No new releases from your artists in the last two months." : "Nothing yet: play a few more artists.")
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontBody
                    wrapMode: Text.Wrap
                }
                HorizontalShelf {
                    id: shelf
                    visible: albumSection.modelData.albums.length > 0
                    Repeater {
                        model: albumSection.modelData.albums
                        delegate: AlbumCard {
                            required property var modelData
                            width: shelf.cardWidth
                            title: modelData.title
                            subtitle: (modelData.year ? modelData.year + " · " : "") + modelData.artist
                            image: modelData.coverBig || modelData.cover
                            seed: modelData.id
                            onClicked: Nav.album(modelData.id)
                            onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                        }
                    }
                }
            }
        }
    }
}
