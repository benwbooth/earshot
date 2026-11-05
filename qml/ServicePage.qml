import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Browse SoundCloud, Mixcloud or the Internet Archive by genre or any tag.
Flickable {
    id: page
    readonly property string pageName: "service"
    property var args: ({})        // { id: "soundcloud" | "mixcloud" | "archive" }
    property string tag: ""
    property string tagLabel: ""
    property var found: null
    property bool loading: false
    property string error: ""
    property int request: -1

    readonly property string service: args.id || "soundcloud"
    readonly property var info: ({
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

    Component.onCompleted: show(genreTag(info.genres[0]), genreLabel(info.genres[0]))

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
            text: page.tagLabel || page.info.name
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
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }

        RowLayout {
            visible: !!page.found && page.found.tracks.length > 0
            spacing: 14
            IconButton {
                filled: true
                iconName: "play"
                iconSize: 22
                tip: "Play all"
                onClicked: Player.playTracks(page.found.tracks, 0, page.info.name + ": " + page.tagLabel)
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
