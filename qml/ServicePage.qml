import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// YouTube, SoundCloud, Mixcloud or the Internet Archive: what the site features
// on its own front page (charts, trending, new releases, picks), then browsing
// by genre or any tag.
Flickable {
    id: page
    readonly property string pageName: "service"
    property var args: ({})        // { id: "youtube" | "soundcloud" | "mixcloud" | "archive" }
    property string tag: ""
    property string tagLabel: ""
    property var found: null
    property bool loading: false
    property string error: ""
    property int request: -1
    // The front page: { sections: [{ id, title, subtitle, albums, tracks, round }] }.
    property var featured: null
    property bool featuredLoading: false
    property string featuredError: ""
    property string openSection: ""     // the section shown in full
    readonly property int songs: 10     // songs a section shows until opened

    readonly property string service: args.id || "soundcloud"
    readonly property var info: ({
        youtube: {
            name: "YouTube", tint: "#ff0033",
            blurb: "Playlists, full albums and soundtracks (split into songs at their timestamps), and songs, straight from YouTube's own search.",
            hint: "Any genre, game, film or mood: city pop, zelda, rainy day…",
            genres: ["Lo-fi", "Video game", "Soundtrack", "Anime", "City pop", "Synthwave", "Jazz", "Classical", "Ambient",
                     "Hip-hop", "Rock", "Metal", "Electronic", "K-pop", "Chiptune", "Live concert", "Folk", "Blues", "Reggae", "Latin"]
        },
        soundcloud: {
            name: "SoundCloud", tint: "#ff5500",
            blurb: "Playlists and tracks from SoundCloud's artists, DJs and producers. They play in SoundCloud's own player.",
            hint: "Any genre or tag: lo-fi, vaporwave, chiptune…",
            genres: ["Electronic", "Hip-hop", "Lo-fi", "House", "Techno", "Ambient", "Indie", "Pop", "R&B", "Rock",
                     "Jazz", "Classical", "Soundtrack", "Chiptune", "Drum & bass", "Dubstep", "Metal", "Folk", "Reggae", "Latin"]
        },
        mixcloud: {
            name: "Mixcloud", tint: "#5000ff",
            blurb: "DJ mixes and radio shows, most popular first. They play in Mixcloud's own player.",
            hint: "Any genre or tag: deep house, disco, jungle…",
            genres: ["House", "Deep house", "Techno", "Drum & bass", "Ambient", "Hip hop", "Jazz", "Soul", "Funk", "Disco",
                     "Electronica", "Dubstep", "Reggae", "Rock", "Chillout", "Breakbeat", "Trance", "Jungle", "World", "Talk"]
        },
        archive: {
            name: "Internet Archive", tint: "#4a8fd4",
            blurb: "Live concert recordings, netlabel releases, 78 rpm records and other freely shared music from archive.org, most downloaded first.",
            hint: "Any subject: blues, grateful dead, bluegrass…",
            genres: [["collection:etree", "Live Music Archive"], ["collection:78rpm", "78 rpm records"], ["collection:netlabels", "Netlabels"],
                     ["collection:georgeblood", "Great 78 Project"], ["collection:opensource_audio", "Community audio"],
                     "Jazz", "Blues", "Classical", "Folk", "Rock", "Electronic", "Ambient", "Hip hop", "Country", "Bluegrass", "Experimental"]
        }
    })[service]

    // Genres are "Name" or ["query", "Name"].
    function genreTag(genre) { return Array.isArray(genre) ? genre[0] : genre }
    function genreLabel(genre) { return Array.isArray(genre) ? genre[1] : genre }

    function show(newTag, label) {
        tag = newTag
        tagLabel = label
        found = null
        error = ""
        loading = true
        Api.cancel(request)
        const wanted = newTag
        request = Api.call(Backend.browse(service, newTag), function (result) {
            if (wanted !== page.tag) return
            page.found = result
            page.loading = false
        }, function (message) {
            if (wanted !== page.tag) return
            page.loading = false
            page.error = message
        })
    }

    function firstTracks() { return found ? found.tracks : [] }

    function loadFeatured() {
        featuredError = ""
        featuredLoading = true
        Api.call(Backend.call("featured", JSON.stringify({ service: service })), function (result) {
            page.featured = result
            page.featuredLoading = false
        }, function (message) {
            page.featuredLoading = false
            page.featuredError = message
        })
    }

    Component.onCompleted: {
        loadFeatured()
        show(genreTag(info.genres[0]), genreLabel(info.genres[0]))
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
            GradientStop { position: 0; color: Qt.alpha(page.info.tint, 0.4) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 56
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        Text {
            text: page.info.name.toUpperCase()
            color: Theme.textMuted
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            font.letterSpacing: 1.5
        }
        Text {
            Layout.fillWidth: true
            text: "Discover"
            color: Theme.text
            font.pixelSize: 48
            font.weight: Font.Black
            font.letterSpacing: -1
            wrapMode: Text.Wrap
        }
        Text {
            Layout.fillWidth: true
            text: page.info.blurb
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody + 1
            wrapMode: Text.Wrap
        }

        StatusView {
            loading: page.featuredLoading
            error: page.featuredError
            onRetry: page.loadFeatured()
        }

        // The site's own front page, a section per chart, list or pick.
        Repeater {
            model: page.featured ? page.featured.sections : []
            delegate: ColumnLayout {
                id: section
                required property var modelData
                readonly property bool open: page.openSection === modelData.id
                readonly property string context: page.info.name + ": " + modelData.title
                Layout.fillWidth: true
                Layout.topMargin: 8
                spacing: 12
                SectionHeader {
                    title: section.modelData.title
                    action: section.open ? "SHOW LESS"
                          : section.modelData.albums.length > sectionShelf.limit || section.modelData.tracks.length > page.songs ? "SEE ALL" : ""
                    onActionClicked: page.openSection = section.open ? "" : section.modelData.id
                }
                Text {
                    visible: text !== ""
                    Layout.fillWidth: true
                    text: section.modelData.subtitle
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontBody
                    wrapMode: Text.Wrap
                }
                HorizontalShelf {
                    id: sectionShelf
                    visible: section.modelData.albums.length > 0
                    maxRows: section.open ? -1 : 1
                    Repeater {
                        model: sectionShelf.take(section.modelData.albums)
                        delegate: AlbumCard {
                            required property var modelData
                            width: sectionShelf.cardWidth
                            title: modelData.title
                            subtitle: modelData.artist + (modelData.year ? " · " + modelData.year : "")
                            image: modelData.coverBig || modelData.cover
                            seed: modelData.id
                            round: section.modelData.round === true
                            onClicked: Nav.album(modelData.id)
                            albumId: modelData.id
                            onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                        }
                    }
                }
                RowLayout {
                    visible: section.modelData.tracks.length > 0
                    spacing: 14
                    IconButton {
                        filled: true
                        readonly property bool pause: Player.playingFrom(section.context, section.modelData.tracks) && Player.busy
                        iconName: pause ? "pause" : "play"
                        iconSize: 22
                        tip: pause ? "Pause" : "Play all"
                        onClicked: Player.playOrToggle(section.modelData.tracks, section.context)
                    }
                    PillButton { text: "Shuffle"; iconName: "shuffle"; onClicked: Player.shuffleTracks(section.modelData.tracks, section.context) }
                }
                TrackList {
                    visible: section.modelData.tracks.length > 0
                    Layout.fillWidth: true
                    tracks: section.modelData.tracks
                    limit: section.open ? -1 : page.songs
                    showAlbum: false
                    context: section.context
                }
            }
        }

        SectionHeader {
            Layout.topMargin: 28
            title: "Browse by genre or tag"
        }

        TextField {
            id: tagField
            Layout.fillWidth: true
            Layout.maximumWidth: 520
            implicitHeight: 44
            leftPadding: 44
            placeholderText: page.info.hint
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.fontBody + 1
            selectByMouse: true
            onAccepted: if (text.trim() !== "") page.show(text.trim(), text.trim())
            background: Rectangle {
                radius: height / 2
                color: tagField.activeFocus ? Theme.surfaceRaised : Theme.surface
                border.color: tagField.activeFocus ? Theme.text : Theme.border
            }
            Icon {
                anchors.left: parent.left
                anchors.leftMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                name: "tag"
                size: 18
                color: Theme.textMuted
            }
        }

        Flow {
            Layout.fillWidth: true
            spacing: 8
            Repeater {
                model: page.info.genres
                delegate: PillButton {
                    required property var modelData
                    text: page.genreLabel(modelData)
                    checkedStyle: page.tag === page.genreTag(modelData)
                    onClicked: page.show(page.genreTag(modelData), page.genreLabel(modelData))
                }
            }
        }

        SectionHeader { title: page.tagLabel }

        StatusView {
            loading: page.loading
            error: page.error
            empty: page.found && page.found.albums.length + page.found.tracks.length === 0 ? "Nothing on " + page.info.name + " for \"" + page.tagLabel + "\"." : ""
            emptyIcon: "tag"
            onRetry: page.show(page.tag, page.tagLabel)
        }

        HorizontalShelf {
            id: shelf
            visible: !!page.found && page.found.albums.length > 0
            Repeater {
                model: page.found ? page.found.albums : []
                delegate: AlbumCard {
                    required property var modelData
                    width: shelf.cardWidth
                    title: modelData.title
                    subtitle: modelData.artist + (modelData.year ? " · " + modelData.year : "")
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }

        RowLayout {
            visible: !!page.found && page.found.tracks.length > 0
            spacing: 14
            IconButton {
                filled: true
                readonly property bool pause: Player.playingFrom(page.info.name + ": " + page.tagLabel, page.firstTracks()) && Player.busy
                iconName: pause ? "pause" : "play"
                iconSize: 22
                tip: pause ? "Pause" : "Play all"
                onClicked: Player.playOrToggle(page.found.tracks, page.info.name + ": " + page.tagLabel)
            }
            PillButton { text: "Shuffle"; iconName: "shuffle"; onClicked: Player.shuffleTracks(page.found.tracks, page.info.name + ": " + page.tagLabel) }
        }
        TrackList {
            visible: !!page.found && page.found.tracks.length > 0
            Layout.fillWidth: true
            tracks: page.found ? page.found.tracks : []
            showAlbum: false
            context: page.info.name + ": " + page.tagLabel
        }
    }
}
