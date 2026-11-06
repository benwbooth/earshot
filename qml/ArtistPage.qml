import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

Flickable {
    id: page
    readonly property string pageName: "artist"
    property var args: ({})
    property var artist: null
    property bool loading: false
    property string error: ""
    property bool showAllTop: false
    property string discography: "albums"

    function firstTracks() { return artist ? artist.topTracks : [] }

    function load() {
        loading = true
        error = ""
        Api.call(Backend.artist(args.id), function (result) {
            page.artist = result
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

    // Big artist photo header
    Item {
        id: hero
        width: parent.width
        height: 340
        clip: true
        Cover {
            anchors.fill: parent
            radius: 0
            source: page.artist ? page.artist.pictureBig : ""
            seed: page.args.id || ""
            placeholderIcon: "user"
            opacity: 0.85
        }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: Qt.rgba(0, 0, 0, 0.1) }
                GradientStop { position: 0.6; color: Qt.rgba(0.05, 0.06, 0.08, 0.6) }
                GradientStop { position: 1; color: Theme.background }
            }
        }
        ColumnLayout {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.leftMargin: Theme.pagePadding
            anchors.bottomMargin: 24
            spacing: 6
            Text {
                text: page.artist ? page.artist.name : " "
                color: "#ffffff"
                font.pixelSize: 64
                font.weight: Font.Black
                font.letterSpacing: -1.5
            }
            Text {
                visible: !!page.artist
                text: page.artist ? Number(page.artist.fans).toLocaleString(Qt.locale(), "f", 0) + " fans · " + page.artist.albumCount + " releases" : ""
                color: Qt.rgba(1, 1, 1, 0.8)
                font.pixelSize: Theme.fontBody + 1
            }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: hero.height + 8
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        StatusView {
            loading: page.loading
            error: page.error
            onRetry: page.load()
        }

        RowLayout {
            visible: !!page.artist
            spacing: 14
            IconButton {
                filled: true
                readonly property bool pause: !!page.artist && Player.playingFrom(page.artist.name, page.artist.topTracks) && Player.busy
                iconName: pause ? "pause" : "play"
                iconSize: 24
                tip: pause ? "Pause" : "Play top songs"
                onClicked: Player.playOrToggle(page.artist.topTracks, page.artist.name)
            }
            PillButton { text: "Shuffle"; iconName: "shuffle"; onClicked: Player.shuffleTracks(page.artist.topTracks, page.artist.name) }
        }

        SectionHeader { visible: !!page.artist; title: "Popular" }
        TrackList {
            visible: !!page.artist
            Layout.fillWidth: true
            tracks: page.artist ? page.artist.topTracks : []
            limit: page.showAllTop ? -1 : 5
            showHeader: false
            context: page.artist ? page.artist.name : ""
        }
        Text {
            visible: !!page.artist && page.artist.topTracks.length > 5
            text: page.showAllTop ? "Show less" : "See more"
            color: moreMouse.containsMouse ? Theme.text : Theme.textMuted
            font.pixelSize: Theme.fontBody
            font.weight: Font.DemiBold
            Layout.leftMargin: 12
            MouseArea { id: moreMouse; anchors.fill: parent; hoverEnabled: true; cursorShape: Qt.PointingHandCursor; onClicked: page.showAllTop = !page.showAllTop }
        }

        SectionHeader { visible: !!page.artist; title: "Discography" }
        RowLayout {
            visible: !!page.artist
            spacing: 8
            PillButton { text: "Albums"; checkedStyle: page.discography === "albums"; onClicked: page.discography = "albums" }
            PillButton { text: "Singles and EPs"; checkedStyle: page.discography === "singles"; onClicked: page.discography = "singles" }
        }
        HorizontalShelf {
            id: shelf
            visible: !!page.artist
            Repeater {
                model: page.artist ? (page.discography === "albums" ? page.artist.albums : page.artist.singles) : []
                delegate: AlbumCard {
                    required property var modelData
                    width: shelf.cardWidth
                    title: modelData.title
                    subtitle: modelData.year + (modelData.kind && modelData.kind !== "album" ? " · " + (modelData.kind === "ep" ? "EP" : modelData.kind.charAt(0).toUpperCase() + modelData.kind.slice(1)) : "")
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }
        Text {
            visible: !!page.artist && (page.discography === "albums" ? page.artist.albums : page.artist.singles).length === 0
            text: "Nothing here."
            color: Theme.textMuted
        }
    }
}
