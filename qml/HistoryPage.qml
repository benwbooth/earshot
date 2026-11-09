import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// The songs you've listened to, newest first, each once.
Flickable {
    id: page
    readonly property string pageName: "history"
    property var args: ({})
    property var plays: []
    property bool loaded: false
    property string filter: ""
    // Newest first (as the server answers), or oldest first.
    property bool newestFirst: true
    readonly property var ordered: newestFirst ? plays : plays.slice().reverse()
    readonly property var shown: {
        const f = filter.trim().toLowerCase()
        if (f === "")
            return ordered
        return ordered.filter(t => (t.title + " " + t.artist + " " + (t.album || "")).toLowerCase().indexOf(f) >= 0)
    }
    readonly property bool playingThis: Player.playingRoute("history", "history")

    function firstTracks() { return plays }

    function reload() {
        Api.call(Backend.call("recentlyPlayed", "{}"), function (list) {
            page.plays = list || []
            page.loaded = true
        }, function (message) { page.loaded = true })
    }
    function play(start) {
        Player.playTracks(page.shown, start || 0, "History", { page: "history", args: { id: "history" } })
    }

    Component.onCompleted: reload()
    Connections {
        target: Backend
        function onLibraryRevisionChanged() { page.reload() }
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

        RowLayout {
            Layout.fillWidth: true
            spacing: 14
            Text {
                Layout.fillWidth: true
                text: "History"
                color: Theme.text
                font.pixelSize: Theme.fontHero
                font.weight: Font.Black
                font.letterSpacing: -0.8
            }
            TextField {
                id: filterField
                visible: page.plays.length > 8
                Layout.preferredWidth: 220
                implicitHeight: 36
                leftPadding: 36
                placeholderText: "Find in history"
                placeholderTextColor: Theme.textFaint
                color: Theme.text
                font.pixelSize: Theme.fontBody
                onTextChanged: page.filter = text
                background: Rectangle { radius: 18; color: Theme.surface; border.color: filterField.activeFocus ? Theme.text : Theme.border }
                Icon { anchors.left: parent.left; anchors.leftMargin: 12; anchors.verticalCenter: parent.verticalCenter; name: "search"; size: 15; color: Theme.textMuted }
            }
        }

        RowLayout {
            visible: page.plays.length > 0
            Layout.fillWidth: true
            spacing: 14
            IconButton {
                filled: true
                iconName: page.playingThis && Player.busy ? "pause" : "play"
                iconSize: 24
                tip: page.playingThis && Player.busy ? "Pause" : "Play"
                onClicked: page.playingThis ? Player.toggle() : page.play(0)
            }
            PillButton {
                text: "Shuffle"
                iconName: "shuffle"
                onClicked: Player.shuffleTracks(page.shown, "History", { page: "history", args: { id: "history" } })
            }
            Text {
                Layout.fillWidth: true
                text: page.plays.length === 1 ? "1 song" : page.plays.length + " songs"
                color: Theme.textMuted
                font.pixelSize: Theme.fontBody
            }
            PillButton {
                text: "Clear history"
                iconName: "trash"
                onClicked: confirmClear.open()
            }
        }

        StatusView {
            visible: page.loaded && page.plays.length === 0
            empty: "Songs you listen to show up here, newest first."
            emptyIcon: "history"
        }

        TrackList {
            visible: page.shown.length > 0
            Layout.fillWidth: true
            tracks: page.shown
            context: "History"
            historyMode: true
            showPlayed: true
            newestFirst: page.newestFirst
            onPlayedSortRequested: page.newestFirst = !page.newestFirst
            route: ({ page: "history", args: { id: "history" } })
        }
    }

    Popup {
        id: confirmClear
        modal: true
        anchors.centerIn: Overlay.overlay
        width: 400
        padding: 24
        Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }
        background: Rectangle { color: Theme.surfaceRaised; radius: Theme.radiusLarge; border.color: Theme.border }
        contentItem: ColumnLayout {
            spacing: 16
            Text { text: "Clear history?"; color: Theme.text; font.pixelSize: Theme.fontTitle - 2; font.weight: Font.Bold }
            Text {
                Layout.fillWidth: true
                wrapMode: Text.WordWrap
                text: "Every song you've listened to will be forgotten. Made for you learns your taste from it, so it starts over too."
                color: Theme.textMuted
            }
            RowLayout {
                Layout.alignment: Qt.AlignRight
                spacing: 10
                PillButton { text: "Cancel"; onClicked: confirmClear.close() }
                PillButton {
                    text: "Clear"
                    primary: true
                    implicitHeight: 36
                    onClicked: {
                        confirmClear.close()
                        Api.call(Backend.call("clearHistory", "{}"), function () {
                            page.plays = []
                            Nav.notify("History cleared")
                        }, function (message) { Nav.notify(message) })
                    }
                }
            }
        }
    }
}
