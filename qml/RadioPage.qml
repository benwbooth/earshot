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

    // The listener's own stations ("Your stations"), again whenever the library changes.
    property var saved: []
    function loadSaved() {
        Api.call(Backend.call("savedStations", "{}"), function (result) { page.saved = result || [] })
    }
    Connections {
        target: Backend
        function onLibraryRevisionChanged() { page.loadSaved() }
    }
    function removeStation(id) {
        page.saved = page.saved.filter(item => item.id !== id)
        Api.call(Backend.call("removeStation", JSON.stringify({ subreddit: id })), null)
        Nav.notify("Removed r/" + id + " from your stations")
    }

    // A subreddit typed into the box, kept among "Your stations" once it loads with songs.
    property string adding: ""
    property string typedError: ""
    function same(a, b) { return a.toLowerCase() === b.toLowerCase() }
    function tuneTo(text) {
        Api.call(Backend.call("radioStationName", JSON.stringify({ text: text })), function (name) {
            page.typedError = ""
            stationField.text = ""
            page.adding = name
            page.load(name, false)
        }, function (message) {
            page.typedError = message
        })
    }
    // The core skips built-in stations and ones already kept.
    function keepIfTyped(name, subreddit) {
        if (adding === "" || !same(adding, name))
            return
        adding = ""
        Api.call(Backend.call("saveStation", JSON.stringify({ subreddit: subreddit })), function (added) {
            if (added)
                Nav.notify("Added r/" + subreddit + " to your stations")
        })
    }

    function load(name, force) {
        station = name
        Api.lastStation = name
        error = ""
        waitSeconds = 0
        savedMinutesAgo = -1
        countdown.stop()
        if (adding !== "" && !same(adding, name))
            adding = ""
        if (Api.radioCache[name] && !force) {
            tracks = Api.radioCache[name]
            keepIfTyped(name, name)
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
            page.keepIfTyped(name, result.subreddit || name)
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

    Component.onCompleted: {
        load(station, false)
        loadSaved()
    }

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

        // Any subreddit; ones that load with songs are kept under "Your stations".
        RowLayout {
            Layout.fillWidth: true
            spacing: 12
            TextField {
                id: stationField
                Layout.fillWidth: true
                Layout.maximumWidth: 520
                implicitHeight: 44
                leftPadding: 44
                placeholderText: "Tune in to any subreddit: r/vaporwave, or a link to one"
                placeholderTextColor: Theme.textFaint
                color: Theme.text
                font.pixelSize: Theme.fontBody + 1
                selectByMouse: true
                onAccepted: page.tuneTo(text)
                onTextEdited: page.typedError = ""
                background: Rectangle {
                    radius: height / 2
                    color: stationField.activeFocus ? Theme.surfaceRaised : Theme.surface
                    border.color: stationField.activeFocus ? Theme.text : Theme.border
                }
                Icon {
                    anchors.left: parent.left
                    anchors.leftMargin: 16
                    anchors.verticalCenter: parent.verticalCenter
                    name: "radio"
                    size: 18
                    color: Theme.textMuted
                }
            }
            PillButton {
                text: "Tune in"
                enabled: stationField.text.trim() !== ""
                opacity: enabled ? 1 : 0.5
                onClicked: page.tuneTo(stationField.text)
            }
            Item { Layout.fillWidth: true }
        }
        Text {
            visible: page.typedError !== ""
            Layout.topMargin: -8
            Layout.fillWidth: true
            wrapMode: Text.WordWrap
            text: page.typedError
            color: Theme.danger
            font.pixelSize: Theme.fontBody
        }

        // The listener's own stations, each removable
        ColumnLayout {
            visible: page.saved.length > 0
            Layout.fillWidth: true
            spacing: 8
            Text {
                text: "YOUR STATIONS"
                color: Theme.textFaint
                font.pixelSize: Theme.fontSmall - 1
                font.weight: Font.DemiBold
                font.letterSpacing: 1.2
            }
            Flow {
                Layout.fillWidth: true
                spacing: 8
                Repeater {
                    model: page.saved
                    delegate: PillButton {
                        id: savedPill
                        required property var modelData
                        readonly property bool current: page.same(page.station, modelData.id)
                        width: implicitWidth + 20
                        rightPadding: 20
                        text: modelData.name
                        checkedStyle: current
                        ToolTip.visible: hovered && !removeButton.hovered
                        ToolTip.text: modelData.about
                        ToolTip.delay: 500
                        onClicked: page.load(modelData.id, false)
                        IconButton {
                            id: removeButton
                            anchors.right: parent.right
                            anchors.rightMargin: 4
                            anchors.verticalCenter: parent.verticalCenter
                            iconName: "close"
                            iconSize: 12
                            color: savedPill.current ? Theme.background : Theme.textMuted
                            hoverColor: savedPill.current ? Theme.background : Theme.text
                            tip: "Remove " + savedPill.modelData.name + " from your stations"
                            onClicked: page.removeStation(savedPill.modelData.id)
                        }
                    }
                }
            }
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
                readonly property bool pause: Player.context === "r/" + page.station && Player.current !== null && Player.busy
                iconName: pause ? "pause" : "play"
                iconSize: 24
                tip: pause ? "Pause" : "Play"
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
