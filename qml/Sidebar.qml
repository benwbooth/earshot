import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

Rectangle {
    id: sidebar
    color: Theme.sidebar
    implicitWidth: 248

    readonly property var playlists: (Backend.libraryRevision, Api.playlists())

    component NavItem: Rectangle {
        id: item
        property string iconName
        property string label
        property string page
        property var args: ({})
        property string detail: ""
        property string coverSeed: ""
        readonly property bool selected: Nav.current === page && (page !== "playlist" || Nav.currentArgs.id === args.id)

        Layout.fillWidth: true
        implicitHeight: Math.max(40, itemRow.implicitHeight + 16)
        radius: 8
        color: selected ? Theme.surfaceRaised : (hover.hovered ? Theme.surface : "transparent")

        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: Nav.open(item.page, item.args) }

        RowLayout {
            id: itemRow
            anchors.fill: parent
            anchors.leftMargin: 12
            anchors.rightMargin: 12
            spacing: 12
            Icon {
                name: item.iconName
                size: 18
                color: item.selected ? Theme.accent : (hover.hovered ? Theme.text : Theme.textMuted)
            }
            Text {
                Layout.fillWidth: true
                text: item.label
                color: item.selected || hover.hovered ? Theme.text : Theme.textMuted
                font.pixelSize: Theme.fontBody
                font.weight: item.selected ? Font.DemiBold : Font.Medium
                wrapMode: Text.Wrap
            }
            Text {
                visible: item.detail !== ""
                text: item.detail
                color: Theme.textFaint
                font.pixelSize: Theme.fontSmall
            }
        }
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 14
        anchors.topMargin: 16
        spacing: 2

        NavItem { iconName: "home"; label: "Home"; page: "home" }
        NavItem { iconName: "search"; label: "Search"; page: "search" }
        NavItem { iconName: "radio"; label: "Reddit radio"; page: "radio" }
        NavItem { iconName: "tag"; label: "Bandcamp"; page: "bandcamp" }
        NavItem { iconName: "queue"; label: "Queue"; page: "queue"; detail: Player.queue.length > 0 ? (Player.queue.length - Player.index - 1 > 0 ? Player.queue.length - Player.index - 1 : "") : "" }

        Text {
            Layout.topMargin: 24
            Layout.leftMargin: 12
            Layout.bottomMargin: 6
            text: "YOUR LIBRARY"
            color: Theme.textFaint
            font.pixelSize: Theme.fontSmall - 1
            font.weight: Font.DemiBold
            font.letterSpacing: 1.2
        }
        NavItem { iconName: "album"; label: "Albums"; page: "albums"; detail: (Backend.libraryRevision, JSON.parse(Backend.savedAlbums()).length || "") }
        NavItem { iconName: "heart"; label: "Liked songs"; page: "playlist"; args: ({ id: "liked" }); detail: sidebar.playlists.length > 0 && sidebar.playlists[0].count > 0 ? sidebar.playlists[0].count : "" }

        RowLayout {
            Layout.topMargin: 24
            Layout.leftMargin: 12
            Layout.bottomMargin: 4
            Text {
                Layout.fillWidth: true
                text: "PLAYLISTS"
                color: Theme.textFaint
                font.pixelSize: Theme.fontSmall - 1
                font.weight: Font.DemiBold
                font.letterSpacing: 1.2
            }
            IconButton { iconName: "link"; iconSize: 15; tip: "Import a playlist from YouTube, SoundCloud or Bandcamp"; onClicked: Nav.askImportPlaylist() }
            IconButton { iconName: "plus"; iconSize: 16; tip: "New playlist"; onClicked: Nav.askNewPlaylist([]) }
        }

        ListView {
            id: playlistList
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 2
            model: sidebar.playlists.filter(p => !p.special)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
            KineticScroll { view: playlistList }
            delegate: NavItem {
                required property var modelData
                width: ListView.view.width
                iconName: "playlist"
                label: modelData.name
                page: "playlist"
                args: ({ id: modelData.id })
                detail: modelData.count > 0 ? modelData.count : ""
            }
            Text {
                visible: parent.count === 0
                anchors.top: parent.top
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.margins: 12
                text: "Make a playlist with +, or add songs from any list with the ··· button."
                wrapMode: Text.WordWrap
                color: Theme.textFaint
                font.pixelSize: Theme.fontSmall + 1
            }
        }
    }
}
