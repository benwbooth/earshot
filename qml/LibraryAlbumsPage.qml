import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Albums the listener saved.
Flickable {
    id: page
    readonly property string pageName: "albums"
    property var args: ({})
    readonly property var albums: (Backend.libraryRevision, JSON.parse(Backend.savedAlbums()))
    property string sort: "recent"
    readonly property var sorted: {
        const list = albums.slice()
        if (sort === "title")
            list.sort((a, b) => a.title.localeCompare(b.title))
        else if (sort === "artist")
            list.sort((a, b) => a.artist.localeCompare(b.artist) || (a.year || "").localeCompare(b.year || ""))
        else if (sort === "year")
            list.sort((a, b) => (b.year || "").localeCompare(a.year || ""))
        return list
    }

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 40
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        Text {
            text: "Your albums"
            color: Theme.text
            font.pixelSize: Theme.fontHero
            font.weight: Font.Black
            font.letterSpacing: -0.8
        }

        RowLayout {
            visible: page.albums.length > 0
            spacing: 8
            Text { text: page.albums.length + (page.albums.length === 1 ? " album" : " albums"); color: Theme.textMuted; Layout.rightMargin: 12 }
            Repeater {
                model: [["recent", "Recently saved"], ["title", "Title"], ["artist", "Artist"], ["year", "Year"]]
                delegate: PillButton {
                    required property var modelData
                    text: modelData[1]
                    checkedStyle: page.sort === modelData[0]
                    onClicked: page.sort = modelData[0]
                }
            }
        }

        StatusView {
            empty: page.albums.length === 0 ? "Albums you save appear here. Open any album and press the heart to keep it." : ""
            emptyIcon: "album"
        }

        HorizontalShelf {
            id: shelf
            Repeater {
                model: page.sorted
                delegate: AlbumCard {
                    required property var modelData
                    width: shelf.cardWidth
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
    }
}
