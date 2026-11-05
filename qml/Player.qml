pragma Singleton
import QtQuick
import QtCore
import Earshot

// Playback: the queue, shuffle and repeat, driving YouTube's own player
// (YouTubeEngine, created by the main window and handed over as `engine`).
QtObject {
    id: player

    property var engine: null
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
    // The YouTube videos that may play the current track, best first; the
    // next one is tried if YouTube won't play a video inside the app.
    property var candidates: []
    property int candidate: 0
    // Track key -> videos found earlier, so upcoming songs start instantly.
    property var found: ({})
    property bool started: false   // the current track has begun playing
    property bool wantPlaying: false
    // A new song has begun, including the next chapter of a video that's already playing.
    signal trackStarted(var track)

    // A track can be a chapter of a longer video: it starts and ends inside it.
    readonly property real trackStart: current && current.start ? current.start : 0
    readonly property real trackEnd: current && current.end ? current.end : 0
    readonly property string loadedVideo: engine ? engine.videoId : ""

    readonly property bool playing: engine !== null && engine.playing
    readonly property bool seekable: true
    readonly property real position: engine ? Math.max(0, engine.position - trackStart) : 0
    readonly property real length: trackEnd > 0 ? trackEnd - trackStart
                                  : engine && engine.duration > 0 ? engine.duration - trackStart
                                  : (current ? current.duration : 0)
    readonly property bool hasNext: index + 1 < queue.length || (repeat === 1 && queue.length > 0)
    readonly property bool hasPrevious: index > 0 || position > 3

    property Settings settings: Settings {
        category: "player"
        property bool shuffle: false
        property int repeat: 0
        property real volume: 0.8
    }

    // The waveform: six bar heights (0..1, bass first) measured from what the
    // speakers are playing, refreshed about 30 times a second while music plays.
    property var levels: [0, 0, 0, 0, 0, 0]
    onPlayingChanged: {
        Backend.setWaveformActive(playing)
        if (!playing)
            levels = [0, 0, 0, 0, 0, 0]
    }
    property Timer levelTimer: Timer {
        interval: 33
        repeat: true
        running: player.playing
        onTriggered: player.levels = Backend.waveformLevels().split(",").map(Number)
    }

    onShuffleChanged: settings.shuffle = shuffle
    onRepeatChanged: settings.repeat = repeat
    onVolumeChanged: {
        settings.volume = volume
        applyVolume()
    }
    onMutedChanged: applyVolume()
    onEngineChanged: applyVolume()

    function applyVolume() {
        if (engine)
            engine.setVolume(muted ? 0 : Math.pow(volume, 2) * 100)
    }

    property Connections engineEvents: Connections {
        target: player.engine
        function onPlayerStateChanged() {
            if (player.engine.playing && player.current && !player.started) {
                player.started = true
                player.loading = false
                player.failures = 0
                player.trackStarted(player.current)
                const video = player.engine.videoId
                if (!player.current.youtubeId && !player.current.service)
                    Backend.rememberVideo(player.current.key, video)
                player.prefetch()
            }
        }
        // A chapter ends before the video does: move on at its end time.
        function onPositionChanged() {
            if (player.trackEnd > 0 && player.started && player.engine.position >= player.trackEnd - 0.2)
                player.next(true)
        }
        function onEnded() {
            player.next(true)
        }
        function onFailed(code, message) {
            console.warn("Player error", code, player.engine.videoId, message)
            if (!player.current)
                return
            // A Bandcamp link may have expired: fetch a fresh one once.
            if (player.current.service === "bandcamp" && player.streams[player.current.key]
                    && player.streamRetried !== player.current.key) {
                player.streamRetried = player.current.key
                player.forgetStream(player.current.key)
                player.resolveCurrent()
                return
            }
            // Another upload of the same song may be allowed to play here.
            if (!player.current.service && player.candidate + 1 < player.candidates.length) {
                player.candidate += 1
                player.engine.load(player.candidates[player.candidate], player.trackStart)
                return
            }
            player.fail("Couldn't play \"" + player.current.title + "\": " + message)
        }
    }

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

    // Where the playing music came from (the album, playlist, chart, station
    // or search page), so the player bar can take you back there.
    property var contextRoute: null

    function playTracks(tracks, start, contextName, route) {
        let list = copy(tracks)
        if (list.length === 0)
            return
        contextRoute = route || { page: Nav.current, args: Object.assign({}, Nav.currentArgs) }
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

    function shuffleTracks(tracks, contextName, route) {
        shuffle = true
        const list = copy(tracks)
        playTracks(list, Math.floor(Math.random() * list.length), contextName, route)
    }

    // Open the album, playlist or page the music is playing from.
    function openSource() {
        if (contextRoute && contextRoute.page)
            Nav.open(contextRoute.page, contextRoute.args)
        else if (current && current.albumId)
            Nav.album(current.albumId)
        else
            Nav.open("queue")
    }

    function jump(i) {
        if (i < 0 || i >= queue.length)
            return
        index = i
        resolveCurrent()
    }

    function startVideos(videos) {
        candidates = videos
        candidate = 0
        engine.load(videos[0], trackStart)
    }

    function resolveCurrent() {
        const track = current
        if (!track || !engine)
            return
        error = ""
        wantPlaying = true
        // Another chapter of the video that's already playing: just move there.
        if (track.youtubeId && track.youtubeId === loadedVideo && started) {
            if (Math.abs(engine.position - trackStart) > 1.5)
                engine.seek(trackStart)
            engine.play()
            trackStarted(track)
            prefetch()
            return
        }
        started = false
        loading = true
        if (pendingRequest >= 0)
            Api.cancel(pendingRequest)
        // SoundCloud songs play in SoundCloud's own player.
        if (track.service === "soundcloud") {
            engine.loadSoundCloud(track.soundcloudId, trackStart)
            return
        }
        // Mixcloud shows play in Mixcloud's own player.
        if (track.service === "mixcloud") {
            engine.loadMixcloud(track.mixcloudKey, trackStart)
            return
        }
        // The Internet Archive's files stream straight from archive.org.
        if (track.service === "archive") {
            engine.loadAudio(track.streamUrl, trackStart)
            return
        }
        // Bandcamp songs stream from Bandcamp, with a link fetched just before.
        if (track.service === "bandcamp") {
            if (streams[track.key]) {
                engine.loadAudio(streams[track.key].url, trackStart)
                return
            }
            engine.stop()
            pendingRequest = Api.call(Backend.stream(JSON.stringify(track)), function (result) {
                pendingRequest = -1
                rememberStream(result)
                if (current && result.key === current.key)
                    engine.loadAudio(result.url, trackStart)
            }, function (message) {
                pendingRequest = -1
                if (current && current.key === track.key)
                    fail(message)
            })
            return
        }
        // YouTube tracks already know their video; others were often looked up ahead of time.
        const known = track.youtubeId ? [track.youtubeId] : found[track.key]
        if (known && known.length > 0) {
            startVideos(known)
            return
        }
        engine.stop()
        pendingRequest = Api.call(Backend.resolve(JSON.stringify(track)), function (result) {
            pendingRequest = -1
            remember(result)
            if (!current || result.key !== current.key)
                return
            startVideos(result.videos)
        }, function (message) {
            pendingRequest = -1
            if (current && current.key === track.key)
                fail(message)
        })
    }

    function remember(result) {
        const map = Object.assign({}, found)
        map[result.key] = result.videos
        found = map
    }

    // Look up the next songs' videos while this one plays, so they start at once.
    function prefetch() {
        for (let i = index + 1; i <= index + 2 && i < queue.length; ++i)
            lookAhead(queue[i])
    }

    // Find a track's video ahead of time (songs you hover, the first song of an album).
    function lookAhead(track) {
        if (!track || track.youtubeId || ["soundcloud", "mixcloud", "archive"].indexOf(track.service) >= 0)
            return
        if (track.service === "bandcamp") {
            if (!streams[track.key])
                Api.call(Backend.stream(JSON.stringify(track)), rememberStream)
            return
        }
        if (!found[track.key])
            Api.call(Backend.resolve(JSON.stringify(track)), remember)
    }

    // Bandcamp songs' audio links, kept for an hour (Bandcamp's expire after about a day).
    property var streams: ({})
    property string streamRetried: ""
    function rememberStream(result) {
        const map = Object.assign({}, streams)
        map[result.key] = { url: result.url, at: Date.now() }
        streams = map
        streamExpiry.restart()
    }
    function forgetStream(key) {
        const map = Object.assign({}, streams)
        delete map[key]
        streams = map
    }
    property Timer streamExpiry: Timer {
        interval: 60 * 1000
        repeat: true
        onTriggered: {
            const map = {}
            for (const key in player.streams)
                if (Date.now() - player.streams[key].at < 3600 * 1000)
                    map[key] = player.streams[key]
            player.streams = map
        }
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
        if (!current || !engine)
            return
        if (playing) {
            wantPlaying = false
            engine.pause()
        } else if (!started || error !== "") {
            resolveCurrent()
        } else {
            wantPlaying = true
            engine.play()
        }
    }

    function next(automatic) {
        if (queue.length === 0)
            return
        if (automatic && repeat === 2) {
            seek(0)
            engine.play()
            return
        }
        if (index + 1 < queue.length)
            jump(index + 1)
        else if (repeat === 1)
            jump(0)
        else if (automatic) {
            engine.pause()
            loading = false
        }
    }

    function previous() {
        if (position > 3 || index <= 0) {
            seek(0)
            return
        }
        jump(index - 1)
    }

    // Seconds from the start of the current track (which may be a chapter).
    function seek(seconds) {
        if (engine)
            engine.seek(trackStart + Math.max(0, seconds))
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
