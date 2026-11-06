import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Popularity list for a genre or an era.
Flickable {
    id: page
    readonly property string pageName: "chart"
    property var args: ({})
    property var data_: null
    property bool loading: false
    property string error: ""

    function firstTracks() { return data_ ? data_.tracks : [] }

    function load() {
        loading = true
        error = ""
        const request = args.kind === "era" ? Backend.era(args.id) : Backend.genre(args.id)
        Api.call(request, function (result) {
            page.data_ = result
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

    Rectangle {
        width: parent.width
        height: 380
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.alpha(page.data_ && page.data_.color ? page.data_.color : Theme.accent2, 0.45) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 64
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        RowLayout {
            spacing: 28
            Cover {
                Layout.preferredWidth: 200
                Layout.preferredHeight: 200
                radius: Theme.radius
                source: page.data_ && page.data_.picture ? page.data_.picture : (page.data_ && page.data_.tracks.length > 0 ? page.data_.tracks[0].coverBig : "")
                seed: page.args.id || ""
                placeholderIcon: "music"
            }
            ColumnLayout {
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: page.args.kind === "era" ? "ERA" : "CHART"
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.5
                }
                Text {
                    Layout.fillWidth: true
                    text: page.data_ ? (page.args.kind === "era" ? page.data_.title : (page.data_.title === "All" ? "Top songs" : "Top " + page.data_.title)) : " "
                    color: Theme.text
                    font.pixelSize: 56
                    font.weight: Font.Black
                    font.letterSpacing: -1.2
                    wrapMode: Text.Wrap
                }
                Text {
                    text: page.data_ ? page.data_.subtitle + " · " + page.data_.tracks.length + " songs" : ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontBody + 1
                }
            }
        }

        RowLayout {
            spacing: 14
            visible: !!page.data_
            IconButton {
                filled: true
                iconName: page.data_ && Player.playingFrom(page.data_.title || "Chart", page.data_.tracks) && Player.playing ? "pause" : "play"
                iconSize: 24
                tip: "Play"
                onClicked: Player.playOrToggle(page.data_.tracks, page.data_.title || "Chart")
            }
            PillButton { text: "Shuffle"; iconName: "shuffle"; onClicked: Player.shuffleTracks(page.data_.tracks, page.data_.title) }
            IconButton {
                iconName: "more"
                tip: "More"
                onClicked: menu.openFor(page.data_.tracks, this)
            }
        }

        StatusView {
            loading: page.loading
            error: page.error
            onRetry: page.load()
        }

        TrackList {
            visible: !!page.data_
            Layout.fillWidth: true
            tracks: page.data_ ? page.data_.tracks : []
            context: page.data_ ? page.data_.title : ""
        }

        SectionHeader {
            visible: !!page.data_ && page.data_.albums.length > 0
            title: "Popular albums"
        }
        HorizontalShelf {
            id: albumShelf
            visible: !!page.data_ && page.data_.albums.length > 0
            Repeater {
                model: page.data_ ? page.data_.albums : []
                delegate: AlbumCard {
                    required property var modelData
                    width: albumShelf.cardWidth
                    title: modelData.title
                    subtitle: modelData.artist
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }
    }

    TrackMenu { id: menu }
}
