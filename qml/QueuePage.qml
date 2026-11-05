import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

Flickable {
    id: page
    readonly property string pageName: "queue"
    property var args: ({})

    function firstTracks() { return Player.queue }

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

        RowLayout {
            Layout.fillWidth: true
            Text {
                Layout.fillWidth: true
                text: "Queue"
                color: Theme.text
                font.pixelSize: Theme.fontHero
                font.weight: Font.Black
                font.letterSpacing: -0.8
            }
            PillButton {
                visible: Player.queue.length > 0
                text: "Save as playlist"
                iconName: "plus"
                onClicked: Nav.askNewPlaylist(Player.queue)
            }
            PillButton {
                visible: Player.index + 1 < Player.queue.length
                text: "Clear upcoming"
                onClicked: Player.clearUpcoming()
            }
        }

        StatusView {
            empty: Player.queue.length === 0 ? "Nothing queued. Play an album, a chart or a radio station and its songs line up here." : ""
            emptyIcon: "queue"
        }

        SectionHeader {
            visible: Player.current !== null
            title: "Now playing" + (Player.context ? " · from " + Player.context : "")
        }
        TrackRow {
            visible: Player.current !== null
            Layout.fillWidth: true
            track: Player.current || ({ key: "", title: "", artist: "" })
            showNumber: false
            onActivated: Player.toggle()
            onMenuRequested: anchor => {
                menu.queueIndex = -1
                menu.openFor([Player.current], anchor)
            }
        }

        SectionHeader {
            visible: Player.index + 1 < Player.queue.length
            title: "Up next"
        }
        Repeater {
            model: Player.queue.slice(Player.index + 1)
            delegate: TrackRow {
                required property var modelData
                required property int index
                Layout.fillWidth: true
                track: modelData
                number: index + 1
                onActivated: Player.jump(Player.index + 1 + index)
                onMenuRequested: anchor => {
                    menu.queueIndex = Player.index + 1 + index
                    menu.openFor([modelData], anchor)
                }
            }
        }
    }

    TrackMenu { id: menu }
}
