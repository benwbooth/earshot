import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// A list of songs. Double-click (or the play button) plays from that song on.
ColumnLayout {
    id: list
    property var tracks: []
    property string context: ""
    property bool showCover: true
    property bool showAlbum: true
    property bool showNumber: true
    property bool showHeader: true
    property string playlistId: ""
    property int limit: -1
    property bool queueMode: false
    // Songs from the History page: their menu can forget them.
    property bool historyMode: false
    // A "Played" column; its heading sorts (newestFirst says which way).
    property bool showPlayed: false
    property bool newestFirst: true
    signal playedSortRequested()
    // Where the player bar takes you back to (default: the page shown).
    property var route: null
    // Where each song shown sits in `tracks` (explicit songs are left out
    // while they're hidden), and the songs shown.
    readonly property var shownAt: {
        const at = []
        for (let i = 0; i < (tracks || []).length; ++i)
            if (Player.allowed(tracks[i]))
                at.push(i)
        return at
    }
    readonly property var shown: shownAt.map(i => tracks[i])

    spacing: 2

    RowLayout {
        visible: list.showHeader && list.shown.length > 0
        Layout.fillWidth: true
        Layout.leftMargin: 12
        Layout.rightMargin: 8
        Layout.bottomMargin: 6
        spacing: 14
        Text { visible: list.showNumber; Layout.preferredWidth: 26; text: "#"; color: Theme.textFaint; font.pixelSize: Theme.fontSmall; horizontalAlignment: Text.AlignHCenter }
        Item { visible: list.showCover; Layout.preferredWidth: 40 }
        Text { Layout.fillWidth: true; Layout.preferredWidth: 3; text: "TITLE"; color: Theme.textFaint; font.pixelSize: Theme.fontSmall - 1; font.letterSpacing: 1 }
        Text { visible: list.showAlbum && list.width > 640; Layout.fillWidth: true; Layout.preferredWidth: 2; text: "ALBUM"; color: Theme.textFaint; font.pixelSize: Theme.fontSmall - 1; font.letterSpacing: 1 }
        Text {
            visible: list.showPlayed
            Layout.preferredWidth: 124
            text: "PLAYED " + (list.newestFirst ? "↓" : "↑")
            color: playedMouse.containsMouse ? Theme.text : Theme.textFaint
            font.pixelSize: Theme.fontSmall - 1
            font.letterSpacing: 1
            MouseArea {
                id: playedMouse
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                onClicked: list.playedSortRequested()
            }
            ToolTip.visible: playedMouse.containsMouse
            ToolTip.text: list.newestFirst ? "Newest first: click for oldest first" : "Oldest first: click for newest first"
            ToolTip.delay: 400
        }
        Item { Layout.preferredWidth: 36 }
        Text { Layout.preferredWidth: 44; text: "TIME"; horizontalAlignment: Text.AlignRight; color: Theme.textFaint; font.pixelSize: Theme.fontSmall - 1; font.letterSpacing: 1 }
        Item { Layout.preferredWidth: 32 }
    }

    Rectangle {
        visible: list.showHeader && list.shown.length > 0
        Layout.fillWidth: true
        Layout.bottomMargin: 6
        implicitHeight: 1
        color: Theme.border
    }

    Repeater {
        model: list.limit >= 0 ? list.shown.slice(0, list.limit) : list.shown
        delegate: TrackRow {
            required property var modelData
            required property int index
            Layout.fillWidth: true
            track: modelData
            number: modelData.trackNumber && !list.showCover ? modelData.trackNumber : index + 1
            showCover: list.showCover
            showAlbum: list.showAlbum
            showNumber: list.showNumber
            showPlayed: list.showPlayed
            onActivated: {
                if (list.queueMode)
                    Player.jump(list.shownAt[index])
                else
                    Player.playTracks(list.shown, index, list.context, list.route)
            }
            onMenuRequested: anchor => {
                menu.playlistId = list.playlistId
                menu.playlistIndex = list.playlistId !== "" ? list.shownAt[index] : -1
                menu.playlistLength = list.tracks.length
                menu.queueIndex = list.queueMode ? list.shownAt[index] : -1
                menu.historyMode = list.historyMode
                menu.openFor([modelData], anchor)
            }
        }
    }

    TrackMenu { id: menu }
}
