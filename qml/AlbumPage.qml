import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtQuick.Effects
import Earshot

Flickable {
    id: page
    readonly property string pageName: "album"
    property var args: ({})
    property var album: null
    property bool loading: false
    property string error: ""
    readonly property bool saved: (Backend.libraryRevision, Backend.isAlbumSaved(args.id || ""))
    // Started here or from the album's card (the same page), or its songs
    // played from another page showing this album.
    readonly property bool playingThis: Player.playingAlbum(args.id || "")
        || (Player.current !== null && album !== null && Player.current.albumId === album.id && Player.context === album.title)

    function firstTracks() { return album ? album.tracks : [] }

    // "ALBUM", "PLAYLIST ON SOUNDCLOUD", "ALBUM ON BANDCAMP"…
    function kindLabel() {
        if (!album)
            return ""
        const kind = { single: "SINGLE", ep: "EP", compile: "COMPILATION", playlist: "PLAYLIST", mix: "SHOW" }[album.kind] || "ALBUM"
        const where = { youtube: " ON YOUTUBE", soundcloud: " ON SOUNDCLOUD", bandcamp: " ON BANDCAMP", archive: " ON THE INTERNET ARCHIVE", mixcloud: " ON MIXCLOUD" }[album.source] || ""
        return kind + where
    }

    function load() {
        loading = true
        error = ""
        Api.call(Backend.album(args.id), function (result) {
            page.album = result
            Api.rememberBandcampTags(result.tags)
            Player.lookAhead(result.tracks[0])
            page.loading = false
        }, function (message) {
            page.loading = false
            page.error = message
        })
    }

    Component.onCompleted: load()

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    // Blurred artwork wash behind the header
    Item {
        width: parent.width
        height: 420
        clip: true
        opacity: 0.55
        Cover {
            id: wash
            anchors.centerIn: parent
            width: parent.width * 1.3
            height: width
            radius: 0
            source: page.album ? page.album.cover : ""
            seed: page.args.id || ""
            visible: false
            layer.enabled: true
        }
        MultiEffect {
            anchors.fill: wash
            source: wash
            blurEnabled: true
            blur: 1.0
            blurMax: 64
            saturation: 0.2
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(0.05, 0.06, 0.08, 0.55) }
                GradientStop { position: 1; color: Theme.background }
            }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 64
        width: page.width - Theme.pagePadding * 2
        spacing: 22

        RowLayout {
            spacing: 28
            Layout.fillWidth: true

            Cover {
                Layout.preferredWidth: 232
                Layout.preferredHeight: 232
                Layout.alignment: Qt.AlignBottom
                radius: Theme.radius
                source: page.album ? (page.album.coverBig || page.album.cover) : ""
                seed: page.args.id || ""
                placeholderIcon: "album"
                // Click to see the cover large.
                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    onClicked: Nav.showArt(page.album ? (page.album.coverBig || page.album.cover) : "")
                }
            }

            ColumnLayout {
                Layout.fillWidth: true
                Layout.alignment: Qt.AlignBottom
                spacing: 10
                Text {
                    text: page.kindLabel()
                    color: Theme.text
                    font.pixelSize: Theme.fontSmall
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.5
                }
                Text {
                    Layout.fillWidth: true
                    text: page.album ? page.album.title : " "
                    color: Theme.text
                    font.pixelSize: page.album && page.album.title.length > 22 ? 40 : 56
                    font.weight: Font.Black
                    font.letterSpacing: -1
                    wrapMode: Text.WordWrap
                }
                RowLayout {
                    spacing: 6
                    Text {
                        text: page.album ? page.album.artist : ""
                        color: artistMouse.containsMouse ? Theme.accent : Theme.text
                        font.pixelSize: Theme.fontBody + 1
                        font.weight: Font.Bold
                        MouseArea {
                            id: artistMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor
                            enabled: !!page.album && !!page.album.artistId
                            onClicked: Nav.artist(page.album.artistId)
                        }
                    }
                    Text {
                        text: page.album ? [page.album.year, page.album.trackCount + (page.album.trackCount === 1 ? " song" : " songs"), Theme.longDuration(page.album.duration)].filter(x => x).map(x => " · " + x).join("") : ""
                        color: Theme.textMuted
                        font.pixelSize: Theme.fontBody + 1
                    }
                }
            }
        }

        RowLayout {
            visible: !!page.album
            spacing: 14
            IconButton {
                filled: true
                iconName: page.playingThis && Player.busy ? "pause" : "play"
                iconSize: 24
                tip: page.playingThis && Player.busy ? "Pause" : "Play album"
                onClicked: page.playingThis ? Player.toggle() : Player.playTracks(page.album.tracks, 0, page.album.title)
            }
            PillButton {
                text: "Shuffle"
                iconName: "shuffle"
                onClicked: Player.shuffleTracks(page.album.tracks, page.album.title)
            }
            IconButton {
                iconName: page.saved ? "heart-filled" : "heart"
                iconSize: 22
                color: page.saved ? Theme.accent : Theme.textMuted
                hoverColor: page.saved ? Theme.accentHover : Theme.text
                tip: page.saved ? "Remove from your albums" : "Save to your albums"
                onClicked: {
                    Backend.setAlbumSaved(JSON.stringify(page.album), !page.saved)
                    Nav.notify(page.saved ? "Saved to your albums" : "Removed from your albums")
                }
            }
            IconButton {
                id: moreButton
                iconName: "more"
                iconSize: 20
                tip: "Add the whole album to the queue or a playlist"
                onClicked: menu.openFor(page.album.tracks, moreButton)
            }
            IconButton {
                readonly property string web: Api.albumPage(page.album)
                visible: web !== ""
                iconName: "external"
                iconSize: 20
                tip: "Open on " + Api.siteName(web)
                onClicked: Api.openWeb(web)
            }
        }

        StatusView {
            loading: page.loading
            error: page.error
            onRetry: page.load()
        }

        // A Bandcamp release's tags: each opens more albums like it.
        TagChips {
            tags: page.album && page.album.tags ? page.album.tags : []
        }

        TrackList {
            visible: !!page.album
            Layout.fillWidth: true
            tracks: page.album ? page.album.tracks : []
            showCover: false
            showAlbum: false
            context: page.album ? page.album.title : ""
        }

        ColumnLayout {
            visible: !!page.album
            spacing: 4
            Layout.topMargin: 8
            Text {
                text: page.album && page.album.releaseDate ? new Date(page.album.releaseDate + "T00:00:00").toLocaleDateString(Qt.locale(), "MMMM d, yyyy") : ""
                color: Theme.textMuted
                font.pixelSize: Theme.fontBody
            }
            Text {
                text: page.album ? [page.album.label, (page.album.genres || []).join(", ")].filter(x => x).join(" · ") : ""
                color: Theme.textFaint
                font.pixelSize: Theme.fontSmall + 1
            }
        }
    }

    TrackMenu { id: menu }
}
