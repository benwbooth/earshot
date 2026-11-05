pragma Singleton
import QtQuick
import QtCore
import QtMultimedia
import Earshot

// Playback: the queue, shuffle and repeat, and the audio player itself.
QtObject {
    id: player

    property var queue: []
    property var originalQueue: []
    property int index: -1
    readonly property var current: index >= 0 && index < queue.length ? queue[index] : null
    property string context: ""
    property bool shuffle: settings.shuffle
    property int repeat: settings.repeat // 0 off, 1 all, 2 one
    property real volume: settings.volume
    property bool muted: false
    property bool loading: false
    property string error: ""
    property int failures: 0
    property int pendingRequest: -1
    property string pendingKey: ""
    property bool retried: false

    readonly property bool playing: media.playbackState === MediaPlayer.PlayingState
    readonly property real position: media.position / 1000
    readonly property real length: media.duration > 0 ? media.duration / 1000 : (current ? current.duration : 0)
    readonly property bool hasNext: index + 1 < queue.length || (repeat === 1 && queue.length > 0)
    readonly property bool hasPrevious: index > 0 || media.position > 3000

    property Settings settings: Settings {
        category: "player"
        property bool shuffle: false
        property int repeat: 0
        property real volume: 0.8
    }

    property AudioOutput output: AudioOutput {
        volume: player.muted ? 0 : Math.pow(player.volume, 2)
    }

    property MediaPlayer media: MediaPlayer {
        audioOutput: player.output
        onMediaStatusChanged: {
            if (mediaStatus === MediaPlayer.EndOfMedia)
                player.next(true)
            else if (mediaStatus === MediaPlayer.BufferedMedia || mediaStatus === MediaPlayer.LoadedMedia)
                player.loading = false
        }
        onErrorOccurred: (code, message) => {
            if (!player.current)
                return
            if (!player.retried) {
                // Stream links expire; look the song up once more.
                player.retried = true
                player.resolveCurrent(true)
                return
            }
            player.fail("Couldn't play \"" + player.current.title + "\": " + message)
        }
    }

    onShuffleChanged: settings.shuffle = shuffle
    onRepeatChanged: settings.repeat = repeat
    onVolumeChanged: settings.volume = volume

    function copy(tracks) {
        return (tracks || []).filter(t => t && t.key).map(t => Object.assign({}, t))
    }

    function shuffled(list) {
        const out = list.slice()
        for (let i = out.length - 1; i > 0; --i) {
            const j = Math.floor(Math.random() * (i + 1))
            const tmp = out[i]; out[i] = out[j]; out[j] = tmp
        }
        return out
    }

    function playTracks(tracks, start, contextName) {
        let list = copy(tracks)
        if (list.length === 0)
            return
        start = Math.max(0, Math.min(start || 0, list.length - 1))
        originalQueue = list.slice()
        if (shuffle) {
            const first = list.splice(start, 1)[0]
            list = [first].concat(shuffled(list))
            start = 0
        }
        context = contextName || ""
        queue = list
        failures = 0
        jump(start)
    }

    function shuffleTracks(tracks, contextName) {
        shuffle = true
        const list = copy(tracks)
        playTracks(list, Math.floor(Math.random() * list.length), contextName)
    }

    function jump(i) {
        if (i < 0 || i >= queue.length)
            return
        index = i
        retried = false
        resolveCurrent(false)
    }

    function resolveCurrent(force) {
        const track = current
        if (!track)
            return
        media.stop()
        error = ""
        loading = true
        if (pendingRequest >= 0)
            Api.cancel(pendingRequest)
        pendingKey = track.key
        pendingRequest = Api.call(Backend.resolve(JSON.stringify(track)), function (result) {
            pendingRequest = -1
            if (!current || result.key !== current.key)
                return
            failures = 0
            media.source = result.url
            media.play()
            prefetch()
        }, function (message) {
            pendingRequest = -1
            if (current && current.key === track.key)
                fail(message)
        })
    }

    function prefetch() {
        const upcoming = queue[index + 1]
        if (upcoming)
            Backend.resolve(JSON.stringify(upcoming))
    }

    function fail(message) {
        loading = false
        error = message
        Nav.notify(message)
        failures += 1
        if (failures < 3 && index + 1 < queue.length)
            skipTimer.restart()
    }

    property Timer skipTimer: Timer {
        interval: 1200
        onTriggered: player.next(true)
    }

    function toggle() {
        if (!current) {
            return
        }
        if (playing)
            media.pause()
        else if (media.source.toString() === "" || error !== "")
            resolveCurrent(false)
        else
            media.play()
    }

    function next(automatic) {
        if (queue.length === 0)
            return
        if (automatic && repeat === 2) {
            media.position = 0
            media.play()
            return
        }
        if (index + 1 < queue.length)
            jump(index + 1)
        else if (repeat === 1)
            jump(0)
        else if (automatic) {
            media.stop()
            loading = false
        }
    }

    function previous() {
        if (media.position > 3000 || index <= 0) {
            media.position = 0
            return
        }
        jump(index - 1)
    }

    function seek(seconds) {
        media.position = Math.max(0, seconds * 1000)
    }

    function setShuffle(on) {
        if (on === shuffle)
            return
        shuffle = on
        if (queue.length === 0)
            return
        const playingNow = current
        if (on) {
            originalQueue = queue.slice()
            const done = queue.slice(0, index + 1)
            queue = done.concat(shuffled(queue.slice(index + 1)))
        } else if (originalQueue.length > 0) {
            const keys = queue.map(t => t.key)
            const restored = originalQueue.filter(t => keys.indexOf(t.key) >= 0)
            queue = restored
            index = playingNow ? Math.max(0, restored.findIndex(t => t.key === playingNow.key)) : 0
        }
    }

    function cycleRepeat() {
        repeat = (repeat + 1) % 3
    }

    function playNext(tracks) {
        const list = copy(tracks)
        if (list.length === 0)
            return
        if (!current) {
            playTracks(list, 0, "")
            return
        }
        const q = queue.slice()
        q.splice(index + 1, 0, ...list)
        queue = q
        originalQueue = originalQueue.concat(list)
        Nav.notify(list.length === 1 ? "Playing \"" + list[0].title + "\" next" : list.length + " songs will play next")
    }

    function enqueue(tracks) {
        const list = copy(tracks)
        if (list.length === 0)
            return
        if (!current) {
            playTracks(list, 0, "")
            return
        }
        queue = queue.concat(list)
        originalQueue = originalQueue.concat(list)
        Nav.notify(list.length === 1 ? "Added \"" + list[0].title + "\" to the queue" : "Added " + list.length + " songs to the queue")
    }

    function removeAt(i) {
        if (i < 0 || i >= queue.length || i === index)
            return
        const q = queue.slice()
        const removed = q.splice(i, 1)[0]
        if (i < index)
            index -= 1
        queue = q
        originalQueue = originalQueue.filter(t => t.key !== removed.key)
    }

    function clearUpcoming() {
        queue = queue.slice(0, index + 1)
    }
}
