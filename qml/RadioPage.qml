import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Reddit radio: pick a music subreddit and its shared songs play in order.
Flickable {
    id: page
    readonly property string pageName: "radio"
    property var args: ({})
    readonly property var groups: JSON.parse(Backend.radioStations())
    property string station: args.station || Api.lastStation
    property var tracks: Api.radioCache[station] || null
    property bool loading: false
    property string error: ""

    function firstTracks() { return tracks || [] }

    // While Reddit asks for a pause: seconds left, and how old the saved list shown meanwhile is.
    property int waitSeconds: 0
    property int savedMinutesAgo: -1

    function load(name, force) {
        station = name
        Api.lastStation = name
        error = ""
        waitSeconds = 0
        savedMinutesAgo = -1
        countdown.stop()
        if (Api.radioCache[name] && !force) {
            tracks = Api.radioCache[name]
            return
        }
        tracks = null
        loading = true
        Api.call(Backend.radio(name), function (result) {
            if (page.station !== name)
                return
            page.loading = false
            if (result.retryIn) {
                // Show the saved list (if any) and try again when Reddit allows.
                page.tracks = result.tracks.length > 0 ? result.tracks : null
                page.savedMinutesAgo = result.savedMinutesAgo !== undefined ? result.savedMinutesAgo : -1
                page.waitSeconds = result.retryIn
                countdown.restart()
                return
            }
            const cache = Object.assign({}, Api.radioCache)
            cache[name] = result.tracks
            Api.radioCache = cache
            page.tracks = result.tracks
        }, function (message) {
            page.loading = false
            if (page.station === name)
                page.error = message
        })
    }

    Timer {
        id: countdown
        interval: 1000
        repeat: true
        onTriggered: {
            page.waitSeconds -= 1
            if (page.waitSeconds <= 0) {
                stop()
                page.load(page.station, true)
            }
        }
    }

    function tuneIn() {
        if (tracks && tracks.length > 0)
            Player.playTracks(tracks, 0, "r/" + station)
    }

    Component.onCompleted: load(station, false)

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    Rectangle {
        width: parent.width
        height: 320
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.rgba(1, 0.27, 0, 0.30) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 40
        width: page.width - Theme.pagePadding * 2
        spacing: 18

        Text {
            text: "Reddit radio"
            color: Theme.text
            font.pixelSize: Theme.fontHero
            font.weight: Font.Black
            font.letterSpacing: -0.8
        }
        Text {
            Layout.topMargin: -10
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: "Songs shared by people in music communities on Reddit, chosen by humans rather than algorithms. Pick a station and press play."
            color: Theme.textMuted
            font.pixelSize: Theme.fontLarge
        }

        // Station picker, grouped
        Repeater {
            model: page.groups
            delegate: ColumnLayout {
                id: group
                required property var modelData
                Layout.fillWidth: true
                spacing: 8
                Text {
                    text: group.modelData.name.toUpperCase()
                    color: Theme.textFaint
                    font.pixelSize: Theme.fontSmall - 1
                    font.weight: Font.DemiBold
                    font.letterSpacing: 1.2
                }
                Flow {
                    Layout.fillWidth: true
                    spacing: 8
                    Repeater {
                        model: group.modelData.stations
                        delegate: PillButton {
                            required property var modelData
                            text: modelData.name
                            checkedStyle: page.station === modelData.id
                            ToolTip.visible: hovered
                            ToolTip.text: modelData.about
                            ToolTip.delay: 500
                            onClicked: page.load(modelData.id, false)
                        }
                    }
                }
            }
        }

        RowLayout {
            Layout.topMargin: 16
            spacing: 16
            IconButton {
                filled: true
                iconName: Player.context === "r/" + page.station && Player.playing ? "pause" : "play"
                iconSize: 24
                enabled: !!page.tracks && page.tracks.length > 0
                onClicked: Player.context === "r/" + page.station && Player.current ? Player.toggle() : page.tuneIn()
            }
            ColumnLayout {
                spacing: 2
                Text {
                    text: "r/" + page.station
                    color: Theme.text
                    font.pixelSize: Theme.fontTitle
                    font.weight: Font.Bold
                }
                Text {
                    text: page.tracks && page.tracks.length > 0 ? page.tracks.length + " songs in the hot list" : (page.loading || page.waitSeconds > 0 ? "Tuning in…" : "")
                    color: Theme.textMuted
                    font.pixelSize: Theme.fontBody
                }
            }
            Item { Layout.fillWidth: true }
            PillButton {
                text: "Shuffle"
                iconName: "shuffle"
                enabled: !!page.tracks && page.tracks.length > 0
                onClicked: Player.shuffleTracks(page.tracks, "r/" + page.station)
            }
            PillButton {
                text: "Refresh"
                onClicked: page.load(page.station, true)
            }
            PillButton {
                text: "Save as playlist"
                iconName: "plus"
                enabled: !!page.tracks && page.tracks.length > 0
                onClicked: {
                    const id = Backend.createPlaylist("r/" + page.station + " · " + new Date().toLocaleDateString(Qt.locale(), "MMM d"), JSON.stringify(page.tracks))
                    Nav.notify("Saved " + page.tracks.length + " songs as a playlist")
                }
            }
        }

        // Reddit lets apps without an account load about one station a minute.
        Rectangle {
            visible: page.waitSeconds > 0
            Layout.fillWidth: true
            implicitHeight: waitText.implicitHeight + 24
            radius: Theme.radius
            color: Theme.surface
            border.color: Theme.border
            Text {
                id: waitText
                anchors.fill: parent
                anchors.margins: 12
                wrapMode: Text.Wrap
                color: Theme.textMuted
                font.pixelSize: Theme.fontBody
                text: "Reddit lets apps load about one station a minute, so r/" + page.station + " will load in "
                      + page.waitSeconds + " s." + (page.savedMinutesAgo >= 0 ? " Until then, here's the list from "
                      + (page.savedMinutesAgo < 60 ? page.savedMinutesAgo + " min" : Math.round(page.savedMinutesAgo / 60) + " h") + " ago." : "")
            }
        }

        StatusView {
            loading: page.loading && page.waitSeconds === 0
            error: page.error
            empty: page.tracks && page.tracks.length === 0 && page.waitSeconds === 0 ? "No songs in this station right now." : ""
            onRetry: page.load(page.station, true)
        }

        TrackList {
            visible: !!page.tracks
            Layout.fillWidth: true
            tracks: page.tracks || []
            showAlbum: false
            context: "r/" + page.station
        }
    }
}
