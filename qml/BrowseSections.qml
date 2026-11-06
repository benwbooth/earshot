import QtQuick
import QtQuick.Layouts
import Earshot

// Browse by era and genre, and Reddit radio: the home page's sections under
// Made for you. The eras and genres come with the charts (Api.homeCache).
ColumnLayout {
    id: browse
    property var data_: Api.homeCache

    function load() {
        Api.call(Backend.home(), function (result) {
            Api.homeCache = result
            browse.data_ = result
        }, function () {})
    }

    Component.onCompleted: if (!data_) load()

    Layout.fillWidth: true
    spacing: 20

    SectionHeader { visible: !!browse.data_; title: "Browse by era" }
    HorizontalShelf {
        id: eraShelf
        albums: false
        visible: !!browse.data_
        minCardWidth: 150
        spacing: 14
        Repeater {
            model: browse.data_ ? browse.data_.eras : []
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

    SectionHeader { visible: !!browse.data_; title: "Browse by genre" }
    HorizontalShelf {
        id: genreShelf
        albums: false
        visible: !!browse.data_
        minCardWidth: 200
        spacing: 14
        Repeater {
            model: browse.data_ ? browse.data_.genres : []
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
        title: "Reddit radio"
        action: "ALL STATIONS"
        onActionClicked: Nav.open("radio")
    }
    HorizontalShelf {
        id: radioShelf
        albums: false
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
