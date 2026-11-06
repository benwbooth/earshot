import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Albums on Bandcamp with one tag: best-selling, new or random, loading more as you scroll.
Flickable {
    id: page
    readonly property string pageName: "bandcamp-tag"
    property var args: ({})       // { tag, label }
    property string slice: "top"
    property var albums: []
    property string next: ""
    property bool loading: false
    property bool loadingMore: false
    property string error: ""
    property int request: -1

    readonly property string tag: args.tag || args.id || ""
    readonly property string label: args.label || tag
    readonly property var related: Api.bandcampTags ? Api.bandcampTags.subgenres.filter(s => s.parent === tag) : []

    function firstTracks() { return [] }

    function load() {
        Api.cancel(request)
        albums = []
        next = ""
        loading = true
        loadingMore = false
        error = ""
        const wanted = slice
        request = Api.call(Backend.bandcampTag(page.tag, wanted, ""), function (result) {
            if (wanted !== page.slice) return
            page.albums = result.albums
            page.next = result.next
            page.loading = false
            page.loadMoreIfNearEnd()
        }, function (message) {
            page.loading = false
            page.error = message
        })
    }

    function loadMore() {
        if (next === "" || loading || loadingMore)
            return
        loadingMore = true
        const wanted = slice
        Api.call(Backend.bandcampTag(page.tag, wanted, next), function (result) {
            if (wanted !== page.slice) return
            const seen = {}
            for (const album of page.albums) seen[album.id] = true
            page.albums = page.albums.concat(result.albums.filter(a => !seen[a.id]))
            page.next = result.next
            page.loadingMore = false
            page.loadMoreIfNearEnd()
        }, function (message) {
            page.loadingMore = false
            page.next = ""
        })
    }

    function loadMoreIfNearEnd() {
        if (contentY + height > contentHeight - 1200)
            loadMore()
    }
    onContentYChanged: loadMoreIfNearEnd()
    onSliceChanged: load()

    Component.onCompleted: {
        load()
        // The genre list, for related tags, if Bandcamp's page hasn't fetched it yet.
        if (!Api.bandcampTags)
            Api.call(Backend.bandcampTags(), function (result) { Api.bandcampTags = result })
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
            GradientStop { position: 0; color: Qt.alpha("#1da0c3", 0.4) }
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
            text: "BANDCAMP TAG"
            color: Theme.textMuted
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            font.letterSpacing: 1.5
        }
        Text {
            Layout.fillWidth: true
            text: page.label
            color: Theme.text
            font.pixelSize: 56
            font.weight: Font.Black
            font.letterSpacing: -1.2
            wrapMode: Text.Wrap
        }

        RowLayout {
            spacing: 8
            Repeater {
                model: [["top", "Best-selling"], ["new", "New arrivals"], ["rand", "Surprise me"]]
                delegate: PillButton {
                    required property var modelData
                    text: modelData[1]
                    checkedStyle: page.slice === modelData[0]
                    onClicked: {
                        if (page.slice === modelData[0] && modelData[0] === "rand")
                            page.load()
                        page.slice = modelData[0]
                    }
                }
            }
        }

        TagChips {
            prefix: ""
            tags: page.related
        }

        StatusView {
            loading: page.loading
            error: page.error
            empty: !page.loading && page.error === "" && page.albums.length === 0 ? "Bandcamp has no albums tagged \"" + page.label + "\"." : ""
            emptyIcon: "tag"
            onRetry: page.load()
        }

        HorizontalShelf {
            id: shelf
            visible: page.albums.length > 0
            Repeater {
                model: page.albums
                delegate: AlbumCard {
                    required property var modelData
                    width: shelf.cardWidth
                    title: modelData.title
                    subtitle: modelData.artist + (modelData.location ? " · " + modelData.location : "")
                    image: modelData.coverBig || modelData.cover
                    seed: modelData.id
                    onClicked: Nav.album(modelData.id)
                    albumId: modelData.id
                    onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                }
            }
        }

        Text {
            visible: page.loadingMore
            text: "Loading more from Bandcamp…"
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }
    }
}
