import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

Flickable {
    id: page
    readonly property string pageName: "home"
    property var args: ({})
    property var data_: Api.homeCache
    property bool loading: false
    property string error: ""

    function firstTracks() { return data_ ? data_.tracks : [] }

    function load() {
        loading = true
        error = ""
        Api.call(Backend.home(), function (result) {
            Api.homeCache = result
            page.data_ = result
            page.loading = false
        }, function (message) {
            page.loading = false
            page.error = message
        })
    }

    Component.onCompleted: if (!data_) load()

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    readonly property string greeting: {
        const hour = new Date().getHours()
        return hour < 5 ? "Up late?" : hour < 12 ? "Good morning" : hour < 18 ? "Good afternoon" : "Good evening"
    }

    // Soft colour wash behind the header
    Rectangle {
        width: parent.width
        height: 340
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(0.48, 0.36, 1, 0.28) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 40
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        Text {
            text: page.greeting
            color: Theme.text
            font.pixelSize: Theme.fontHero
            font.weight: Font.Black
            font.letterSpacing: -0.8
        }
        Text {
            Layout.topMargin: -12
            text: "What's popular now, the hits of every decade, and songs people are sharing on Reddit."
            color: Theme.textMuted
            font.pixelSize: Theme.fontLarge
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
        }

        StatusView {
            loading: page.loading && !page.data_
            error: page.data_ ? "" : page.error
            onRetry: page.load()
        }

        // Quick picks: the top songs as compact tiles
        GridLayout {
            visible: !!page.data_
            Layout.fillWidth: true
            Layout.topMargin: 8
            columns: page.width > 1100 ? 3 : 2
            columnSpacing: 12
            rowSpacing: 12
            Repeater {
                model: page.data_ ? page.data_.tracks.filter(t => Player.allowed(t)).slice(0, 6) : []
                delegate: Rectangle {
                    id: quick
                    required property var modelData
                    required property int index
                    readonly property bool isCurrent: Player.current !== null && Player.current.key === modelData.key
                    Layout.fillWidth: true
                    implicitHeight: Math.max(64, quickText.implicitHeight + 16)
                    radius: 8
                    color: quickHover.hovered ? Theme.surfaceHover : Qt.rgba(1, 1, 1, 0.06)
                    HoverHandler { id: quickHover; cursorShape: Qt.PointingHandCursor }
                    TapHandler { onTapped: quick.isCurrent ? Player.toggle() : Player.playTracks(page.data_.tracks.filter(t => Player.allowed(t)), quick.index, "Trending now") }
                    RowLayout {
                        anchors.fill: parent
                        spacing: 14
                        Cover {
                            Layout.preferredWidth: 64
                            Layout.fillHeight: true
                            radius: 8
                            source: quick.modelData.cover
                            seed: quick.modelData.key
                        }
                        ColumnLayout {
                            id: quickText
                            Layout.fillWidth: true
                            spacing: 2
                            Text { Layout.fillWidth: true; text: quick.modelData.title; color: quick.isCurrent ? Theme.accent : Theme.text; font.pixelSize: Theme.fontBody + 1; font.weight: Font.DemiBold; wrapMode: Text.Wrap }
                            Text { Layout.fillWidth: true; text: quick.modelData.artist; color: Theme.textMuted; font.pixelSize: Theme.fontSmall + 1; wrapMode: Text.Wrap }
                        }
                        IconButton {
                            Layout.rightMargin: 10
                            filled: true
                            iconSize: 16
                            iconName: quick.isCurrent && Player.busy ? "pause" : "play"
                            tip: quick.isCurrent && Player.busy ? "Pause" : "Play"
                            opacity: quickHover.hovered || quick.isCurrent ? 1 : 0
                            onClicked: quick.isCurrent ? Player.toggle() : Player.playTracks(page.data_.tracks.filter(t => Player.allowed(t)), quick.index, "Trending now")
                        }
                    }
                }
            }
        }

        SectionHeader {
            visible: !!page.data_
            title: "Trending songs"
            action: "SEE ALL"
            onActionClicked: Nav.open("chart", { kind: "genre", id: "0" })
        }
        TrackList {
            visible: !!page.data_
            Layout.fillWidth: true
            tracks: page.data_ ? page.data_.tracks : []
            limit: 10
            context: "Trending songs"
            showHeader: false
        }

        SectionHeader {
            visible: !!page.data_
            title: "Popular albums"
            action: "SEE ALL"
            onActionClicked: Nav.open("chart", { kind: "genre", id: "0" })
        }
        HorizontalShelf {
            id: albumShelf
            visible: !!page.data_
            maxRows: 1
            Repeater {
                model: albumShelf.take(page.data_ ? page.data_.albums : [])
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

        SectionHeader { visible: !!page.data_; title: "Browse by era" }
        HorizontalShelf {
            id: eraShelf
            albums: false
            visible: !!page.data_
            minCardWidth: 150
            spacing: 14
            Repeater {
                model: page.data_ ? page.data_.eras : []
                delegate: TileCard {
                    required property var modelData
                    width: eraShelf.cardWidth
                    implicitHeight: 84
                    title: modelData.name
                    subtitle: "Greatest hits"
                    tint: modelData.color
                    onClicked: Nav.open("chart", { kind: "era", id: modelData.id })
                }
            }
        }

        SectionHeader { visible: !!page.data_; title: "Browse by genre" }
        HorizontalShelf {
            id: genreShelf
            albums: false
            visible: !!page.data_
            minCardWidth: 200
            spacing: 14
            Repeater {
                model: page.data_ ? page.data_.genres : []
                delegate: TileCard {
                    required property var modelData
                    required property int index
                    width: genreShelf.cardWidth
                    title: modelData.name
                    image: modelData.picture
                    tint: ["#ff5f7e", "#7a5cff", "#3db8ff", "#3ddc97", "#f5c542", "#ff8a3d", "#d36cff"][index % 7]
                    onClicked: Nav.open("chart", { kind: "genre", id: modelData.id })
                }
            }
        }

        SectionHeader {
            visible: !!page.data_
            title: "Reddit radio"
            action: "ALL STATIONS"
            onActionClicked: Nav.open("radio")
        }
        HorizontalShelf {
            id: radioShelf
            albums: false
            visible: !!page.data_
            minCardWidth: 200
            spacing: 14
            maxRows: 1
            Repeater {
                model: radioShelf.take(JSON.parse(Backend.radioStations())[0].stations)
                delegate: TileCard {
                    required property var modelData
                    required property int index
                    width: radioShelf.cardWidth
                    title: modelData.name
                    subtitle: modelData.about
                    tint: ["#ff4500", "#ff8a3d", "#ff5f7e", "#d36cff", "#7a5cff", "#3db8ff"][index % 6]
                    onClicked: Nav.open("radio", { station: modelData.id })
                }
            }
        }
    }
}
