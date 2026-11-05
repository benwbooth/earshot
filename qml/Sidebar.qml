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
        implicitHeight: 40
        radius: 8
        color: selected ? Theme.surfaceRaised : (hover.hovered ? Theme.surface : "transparent")

        HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
        TapHandler { onTapped: Nav.open(item.page, item.args) }

        RowLayout {
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
                elide: Text.ElideRight
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
        anchors.topMargin: 22
        spacing: 2

        RowLayout {
            Layout.leftMargin: 10
            Layout.bottomMargin: 20
            spacing: 10
            Rectangle {
                implicitWidth: 30
                implicitHeight: 30
                radius: 9
                gradient: Gradient {
                    orientation: Gradient.Horizontal
                    GradientStop { position: 0; color: Theme.accent }
                    GradientStop { position: 1; color: Theme.accent2 }
                }
                Icon { anchors.centerIn: parent; name: "music"; size: 17; color: "#ffffff" }
            }
            Text {
                text: "Earshot"
                color: Theme.text
                font.pixelSize: 20
                font.weight: Font.Bold
                font.letterSpacing: -0.3
            }
        }

        NavItem { iconName: "home"; label: "Home"; page: "home" }
        NavItem { iconName: "search"; label: "Search"; page: "search" }
        NavItem { iconName: "radio"; label: "Reddit radio"; page: "radio" }
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
            IconButton { iconName: "link"; iconSize: 15; tip: "Import a YouTube playlist"; onClicked: Nav.askImportPlaylist() }
            IconButton { iconName: "plus"; iconSize: 16; tip: "New playlist"; onClicked: Nav.askNewPlaylist([]) }
        }

        ListView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            spacing: 2
            model: sidebar.playlists.filter(p => !p.special)
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
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

        Rectangle {
            visible: !Backend.canPlay
            Layout.fillWidth: true
            implicitHeight: warn.implicitHeight + 20
            radius: 8
            color: Qt.rgba(1, 0.42, 0.42, 0.12)
            Text {
                id: warn
                anchors.fill: parent
                anchors.margins: 10
                wrapMode: Text.WordWrap
                text: "yt-dlp isn't installed, so songs can't play yet. Install it and restart Earshot."
                color: Theme.danger
                font.pixelSize: Theme.fontSmall
            }
        }
    }
}
