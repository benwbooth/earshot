import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Bandcamp's front page (Album of the Day, the editors' picks, what's
// selling right now, Bandcamp Daily), then browse by genre and tag, or look
// up any tag.
Flickable {
    id: page
    readonly property string pageName: "bandcamp"
    property var args: ({})
    property var tags: Api.bandcampTags
    property bool loading: false
    property string error: ""
    // { highlight, sections: [{ id, title, about, link, rows, albums, tracks, articles }] }
    property var featured: null
    property bool featuredLoading: false
    property string featuredError: ""

    readonly property var tints: ["#ff5f7e", "#7a5cff", "#3db8ff", "#3ddc97", "#ff8a3d", "#f5c542", "#e05cff", "#5c8aff"]

    function loadFeatured() {
        featuredLoading = true
        featuredError = ""
        Api.call(Backend.call("bandcampFeatured", "{}"), function (result) {
            page.featured = result
            page.featuredLoading = false
        }, function (message) {
            page.featuredLoading = false
            page.featuredError = message
        })
    }
    function albumSubtitle(album) {
        return album.artist + (album.note ? " · " + album.note : "")
    }

    function load() {
        if (tags)
            return
        loading = true
        error = ""
        Api.call(Backend.bandcampTags(), function (result) {
            Api.bandcampTags = result
            page.tags = result
            page.loading = false
        }, function (message) {
            page.loading = false
            page.error = message
        })
    }
    function subgenres(parent) {
        return tags ? tags.subgenres.filter(s => s.parent === parent) : []
    }
    function openTag(text) {
        const words = text.trim()
        if (words !== "")
            Nav.open("bandcamp-tag", { tag: words, label: words.toLowerCase() })
    }

    // Typing in the tag box filters every known tag (Bandcamp's genres and
    // styles, plus tags seen on albums you've opened); the words themselves
    // are always offered as a tag too.
    readonly property string filter: tagField.text.trim().toLowerCase()
    readonly property var matches: {
        if (filter === "")
            return []
        const known = (tags ? tags.genres.concat(tags.subgenres) : []).concat(Api.seenBandcampTags)
        const seen = {}
        const found = []
        const typed = filter.replace(/[^\p{L}\p{N}]+/gu, "-").replace(/^-+|-+$/g, "")
        if (typed !== "") {
            found.push({ slug: typed, label: filter })
            seen[typed] = true
        }
        const words = filter.split(/\s+/)
        const hits = known.filter(t => words.every(w => t.label.toLowerCase().indexOf(w) >= 0 || t.slug.indexOf(w) >= 0))
        hits.sort((a, b) => (a.label.toLowerCase().indexOf(filter) === 0 ? 0 : 1) - (b.label.toLowerCase().indexOf(filter) === 0 ? 0 : 1)
                            || a.label.length - b.label.length)
        for (const tag of hits) {
            if (!seen[tag.slug]) {
                seen[tag.slug] = true
                found.push(tag)
            }
        }
        return found.slice(0, 60)
    }

    Component.onCompleted: {
        load()
        loadFeatured()
        if (args.id)
            tagField.text = args.id
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
            text: "BANDCAMP"
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
            text: "Bandcamp's Album of the Day, its editors' picks, what fans are buying right now and the best-selling and newest albums, then any genre or tag. Songs stream from Bandcamp itself."
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody + 1
            wrapMode: Text.Wrap
        }

        StatusView {
            loading: page.featuredLoading
            error: page.featuredError
            onRetry: page.loadFeatured()
        }

        // Today's Album of the Day, big, with what Bandcamp Daily says about it.
        Rectangle {
            id: highlight
            readonly property var album: page.featured && page.featured.highlight ? page.featured.highlight : null
            readonly property bool narrow: width < 620
            visible: !!album
            Layout.fillWidth: true
            implicitHeight: highlightGrid.implicitHeight + (narrow ? 32 : 40)
            radius: 16
            color: Theme.surface
            border.color: Theme.border

            GridLayout {
                id: highlightGrid
                anchors.fill: parent
                anchors.margins: highlight.narrow ? 16 : 20
                columns: highlight.narrow ? 1 : 2
                columnSpacing: 28
                rowSpacing: 16

                Cover {
                    Layout.preferredWidth: highlight.narrow ? Math.min(260, highlightGrid.width) : 220
                    Layout.preferredHeight: Layout.preferredWidth
                    Layout.alignment: highlight.narrow ? Qt.AlignHCenter : Qt.AlignVCenter
                    source: highlight.album ? (highlight.album.coverBig || highlight.album.cover) : ""
                    seed: highlight.album ? highlight.album.id : ""
                    radius: 12
                    HoverHandler { cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: Nav.album(highlight.album.id) }
                }
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.alignment: Qt.AlignVCenter
                    spacing: 8
                    Text {
                        Layout.fillWidth: true
                        text: highlight.album ? ["Album of the Day", highlight.album.genre, highlight.album.date].filter(p => !!p).join(" · ").toUpperCase() : ""
                        color: Theme.textMuted
                        font.pixelSize: Theme.fontSmall
                        font.weight: Font.DemiBold
                        font.letterSpacing: 1.5
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.fillWidth: true
                        text: highlight.album ? highlight.album.title : ""
                        color: Theme.text
                        font.pixelSize: highlight.narrow ? 24 : 30
                        font.weight: Font.ExtraBold
                        font.letterSpacing: -0.5
                        font.underline: titleHover.hovered
                        wrapMode: Text.Wrap
                        HoverHandler { id: titleHover; cursorShape: Qt.PointingHandCursor }
                        TapHandler { onTapped: Nav.album(highlight.album.id) }
                    }
                    Text {
                        Layout.fillWidth: true
                        text: highlight.album ? highlight.album.artist : ""
                        color: Theme.text
                        font.pixelSize: Theme.fontBody + 2
                        font.weight: Font.DemiBold
                        wrapMode: Text.Wrap
                    }
                    Text {
                        Layout.fillWidth: true
                        Layout.maximumWidth: 640
                        Layout.bottomMargin: 6
                        text: highlight.album ? highlight.album.blurb : ""
                        color: Theme.textMuted
                        font.pixelSize: Theme.fontBody + 1
                        lineHeight: 1.15
                        wrapMode: Text.Wrap
                    }
                    Flow {
                        Layout.fillWidth: true
                        spacing: 14
                        PillButton {
                            // Pause while it plays (started here, from its card or its page).
                            readonly property bool ours: !!highlight.album && Player.playingAlbum(highlight.album.id)
                            primary: true
                            iconName: ours && Player.busy ? "pause" : "play"
                            text: ours && Player.busy ? "Pause" : "Play"
                            onClicked: ours ? Player.toggle() : Api.playAlbum(highlight.album.id, highlight.album.title)
                        }
                        PillButton {
                            visible: !!highlight.album && !!highlight.album.article
                            iconName: "external"
                            text: "Read on Bandcamp Daily"
                            onClicked: Api.openWeb(highlight.album.article)
                        }
                    }
                }
            }
        }

        // The front page's sections: albums, songs or stories (which open on the web).
        Repeater {
            model: page.featured ? page.featured.sections : []
            delegate: ColumnLayout {
                id: section
                required property var modelData
                Layout.fillWidth: true
                spacing: 20

                SectionHeader {
                    title: section.modelData.title
                    action: section.modelData.link ? "OPEN ON THE WEB" : ""
                    onActionClicked: Api.openWeb(section.modelData.link)
                }
                Text {
                    visible: !!section.modelData.about
                    Layout.fillWidth: true
                    Layout.topMargin: -14
                    text: section.modelData.about || ""
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontSmall + 1
                    wrapMode: Text.Wrap
                }
                HorizontalShelf {
                    id: albumShelf
                    visible: section.modelData.albums.length > 0
                    maxRows: Math.max(1, section.modelData.rows || 1)
                    Repeater {
                        model: albumShelf.take(section.modelData.albums)
                        delegate: AlbumCard {
                            required property var modelData
                            width: albumShelf.cardWidth
                            title: modelData.title
                            subtitle: page.albumSubtitle(modelData)
                            image: modelData.coverBig || modelData.cover
                            seed: modelData.id
                            albumId: modelData.id
                            onClicked: Nav.album(modelData.id)
                            onPlayClicked: Api.playAlbum(modelData.id, modelData.title)
                        }
                    }
                }
                TrackList {
                    visible: section.modelData.tracks.length > 0
                    Layout.fillWidth: true
                    tracks: section.modelData.tracks
                    limit: 10
                    context: section.modelData.title
                    showHeader: false
                }
                HorizontalShelf {
                    id: storyShelf
                    visible: section.modelData.articles.length > 0
                    maxRows: Math.max(1, section.modelData.rows || 1)
                    Repeater {
                        model: storyShelf.take(section.modelData.articles)
                        delegate: AlbumCard {
                            required property var modelData
                            width: storyShelf.cardWidth
                            title: modelData.title
                            subtitle: [modelData.category, modelData.blurb].filter(p => !!p).join(" · ")
                            image: modelData.image
                            seed: modelData.url
                            playable: false
                            placeholderIcon: "external"
                            onClicked: Api.openWeb(modelData.url)
                        }
                    }
                }
            }
        }

        SectionHeader { title: "Browse by tag" }
        Text {
            Layout.fillWidth: true
            Layout.topMargin: -14
            text: "Best-selling, new and random albums from Bandcamp's artists and labels for any genre or tag."
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody + 1
            wrapMode: Text.Wrap
        }

        // Any tag at all, not only the genres below.
        TextField {
            id: tagField
            Layout.fillWidth: true
            Layout.maximumWidth: 520
            implicitHeight: 44
            leftPadding: 44
            placeholderText: "Filter tags, or type any tag: chiptune, vaporwave…"
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.fontBody + 1
            selectByMouse: true
            onAccepted: page.openTag(text)
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

        StatusView {
            loading: page.loading
            error: page.error
            onRetry: page.load()
        }

        // While filtering: the matching tags, the typed words first.
        SectionHeader {
            visible: page.filter !== ""
            title: page.matches.length > 1 ? "Tags matching \"" + page.filter + "\"" : "Tag"
        }
        TagChips {
            visible: page.filter !== ""
            prefix: ""
            tags: page.matches
        }

        SectionHeader { visible: !!page.tags && page.filter === ""; title: "Genres" }
        GridLayout {
            visible: !!page.tags && page.filter === ""
            Layout.fillWidth: true
            columns: Math.max(1, Math.floor(width / 240))
            columnSpacing: 16
            rowSpacing: 16
            Repeater {
                model: page.tags ? page.tags.genres : []
                delegate: TileCard {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    title: modelData.label
                    subtitle: page.subgenres(modelData.slug).length > 0 ? page.subgenres(modelData.slug).length + " styles" : ""
                    tint: page.tints[index % page.tints.length]
                    onClicked: Nav.open("bandcamp-tag", { tag: modelData.slug, label: modelData.label })
                }
            }
        }

        Repeater {
            model: page.tags ? page.tags.genres : []
            delegate: ColumnLayout {
                required property var modelData
                Layout.fillWidth: true
                visible: page.filter === "" && page.subgenres(modelData.slug).length > 0
                spacing: 10
                SectionHeader { title: modelData.label }
                TagChips {
                    prefix: ""
                    tags: page.subgenres(modelData.slug)
                }
            }
        }
    }
}
