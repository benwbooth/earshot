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
    // The last song looked for on SoundCloud and Bandcamp after YouTube wouldn't play it.
    property string elsewhereTried: ""
    // Track key -> videos found earlier, so upcoming songs start instantly.
    property var found: ({})
    property bool started: false   // the current track has begun playing
    property bool wantPlaying: false
    // A new song has begun, including the next chapter of a video that's already playing.
    signal trackStarted(var track)

    // Listening history (for Made for you): a song counts once it has played
    // 30 seconds, or half of a shorter one, so skipped songs don't.
    property var unrecordedPlay: null
    onTrackStarted: track => unrecordedPlay = track
    function recordIfListened() {
        if (!unrecordedPlay || !current || unrecordedPlay.key !== current.key)
            return
        if (position < Math.min(30, Math.max(5, length / 2)))
            return
        const played = engine ? engine.videoId : ""
        Backend.recordPlay(JSON.stringify(unrecordedPlay), played.indexOf(":") < 0 ? played : "")
        unrecordedPlay = null
    }

    // A track can be a chapter of a longer video: it starts and ends inside it.
    readonly property real trackStart: current && current.start ? current.start : 0
    readonly property real trackEnd: current && current.end ? current.end : 0
    readonly property string loadedVideo: engine ? engine.videoId : ""

    readonly property bool playing: engine !== null && engine.playing
    // Music playing or on its way: a song starting or buffering, or the next
    // one about to start after one failed. A page's pink play button shows
    // pause meanwhile (a click stops it), so it doesn't flicker to play.
    readonly property bool busy: playing || loading || skipTimer.running || (engine !== null && engine.playerState === 3)
    readonly property bool seekable: true
    // Seconds into the current song to start from when it next loads: where it
    // was when Earshot last quit, until play is pressed.
    property real resumeAt: 0
    readonly property real position: resumeAt > 0 && !started ? resumeAt
                                     : engine ? Math.max(0, engine.position - trackStart) : 0
    readonly property real length: trackEnd > 0 ? trackEnd - trackStart
                                  : engine && engine.duration > 0 ? engine.duration - trackStart
                                  : (current ? current.duration : 0)
    readonly property bool hasNext: index + 1 < queue.length || (repeat === 1 && queue.length > 0) || (radio && current !== null)

    // Random radio: when the queue runs out, keep going with songs like the
    // last ones (YouTube's own mix for the song playing), added a batch at a time.
    property bool radio: settings.radio
    property bool radioLoading: false
    property bool radioWaiting: false   // the queue ended while more songs were on their way
    onRadioChanged: {
        settings.radio = radio
        if (radio && current && index >= queue.length - 2)
            extendRadio()
    }
    readonly property bool hasPrevious: index > 0 || position > 3

    property Settings settings: Settings {
        category: "player"
        property bool shuffle: false
        property int repeat: 0
        property real volume: 0.8
        property bool radio: false
        property bool leveling: false
        // The queue, the song in it and how far it had played, so the next
        // launch carries on where this one left off.
        property string session: ""
    }

    // Saved when the queue or song changes, every few seconds while playing,
    // and on quit (saveSession()).
    function saveSession() {
        if (restoring || closed || !engine)
            return
        if (!current) {
            settings.session = ""
            return
        }
        settings.session = JSON.stringify({
            queue: queue,
            originalQueue: originalQueue,
            index: index,
            context: context,
            contextRoute: contextRoute,
            position: Math.floor(position)
        })
    }
    property bool restoring: false
    // Saved for the last time as Earshot quits, before its player goes away.
    property bool closed: false
    function closeSession() {
        saveSession()
        closed = true
    }
    property Connections quitting: Connections {
        target: Qt.application
        function onAboutToQuit() { player.closeSession() }
    }
    property Timer sessionTimer: Timer {
        interval: 5000
        repeat: true
        running: player.playing
        onTriggered: player.saveSession()
    }
    property Timer sessionSoon: Timer {
        interval: 300
        onTriggered: player.saveSession()
    }
    onQueueChanged: sessionSoon.restart()
    onIndexChanged: sessionSoon.restart()
    // Picks the last session back up, paused at the same place: play carries on from there.
    function restoreSession() {
        if (!settings.session || current)
            return
        let saved
        try {
            saved = JSON.parse(settings.session)
        } catch (e) {
            return
        }
        const list = (saved.queue || []).filter(t => t && t.key)
        if (list.length === 0 || !(saved.index >= 0 && saved.index < list.length))
            return
        restoring = true
        originalQueue = (saved.originalQueue || list).filter(t => t && t.key)
        context = saved.context || ""
        contextRoute = saved.contextRoute || null
        queue = list
        index = saved.index
        started = false
        loading = false
        wantPlaying = false
        resumeAt = Math.max(0, saved.position || 0)
        restoring = false
    }
    // Later, once the settings have been read.
    Component.onCompleted: Qt.callLater(restoreSession)

    function extendRadio() {
        if (!radio || radioLoading || !current)
            return
        const seed = current
        // The video playing (or found) for it, so the mix starts from exactly this song.
        const playingVideo = engine && engine.videoId && engine.videoId.indexOf(":") < 0 ? engine.videoId : ""
        const video = seed.youtubeId || (found[seed.key] && found[seed.key][0]) || (seed.service ? "" : playingVideo)
        radioLoading = true
        Api.call(Backend.radioMore(JSON.stringify(seed), video), function (result) {
            radioLoading = false
            const seen = {}
            for (const track of queue) {
                seen[track.key] = true
                if (track.youtubeId) seen["yt:" + track.youtubeId] = true
            }
            const added = result.tracks.filter(track => !seen[track.key] && allowed(track)).slice(0, 15)
            if (added.length > 0) {
                queue = queue.concat(added)
                originalQueue = originalQueue.concat(added)
            }
            if (radioWaiting) {
                radioWaiting = false
                if (index + 1 < queue.length)
                    jump(index + 1)
            }
        }, function (message) {
            radioLoading = false
            if (radioWaiting) {
                radioWaiting = false
                Nav.notify("Random radio: " + message)
            }
        })
    }

    // The waveform: six bar heights (0..1, bass first) measured from what the
    // speakers are playing, refreshed about 30 times a second while music plays.
    property var levels: [0, 0, 0, 0, 0, 0]
    onPlayingChanged: {
        saveSession()
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
            engine.setVolume(muted ? 0 : Math.min(100, Math.pow(volume, 2) * 100 * Math.pow(10, levelGain / 20)))
    }

    // Volume leveling (Settings): each song is turned up or down (in decibels,
    // as Backend's "loudness" works out from how loud it is) so songs sound
    // about as loud as each other. Quiet songs can only be turned up as far
    // as the volume slider leaves room.
    property bool leveling: settings.leveling
    property real levelGain: 0      // what the player is given now
    property real levelTarget: 0    // what the song playing should have
    property string levelFor: ""    // the video or stream it's for
    onLevelingChanged: {
        settings.leveling = leveling
        updateLevel()
    }
    onLevelGainChanged: applyVolume()
    function updateLevel() {
        const id = engine ? engine.videoId : ""
        if (!leveling || !id) {
            levelFor = ""
            levelTarget = 0
            if (!leveling)
                levelRamp.start()
            return
        }
        if (id === levelFor)
            return
        levelFor = id
        const track = current
        Api.call(Backend.call("loudness", JSON.stringify({ id: id, track: track })), function (result) {
            if (!leveling || !engine || engine.videoId !== id)
                return
            console.info("Volume leveling:", id, result.lufs === null ? "loudness unknown" : result.lufs + " LUFS (" + result.how + ")", "->", result.gain, "dB")
            levelTarget = result.gain
            // Not heard yet: straight away; otherwise eased in.
            if (!started || position < 1.5)
                levelGain = levelTarget
            else
                levelRamp.start()
        }, function () {})
    }
    property Timer levelRamp: Timer {
        interval: 50
        repeat: true
        onTriggered: {
            const step = 0.4
            const gap = player.levelTarget - player.levelGain
            if (Math.abs(gap) <= step) {
                player.levelGain = player.levelTarget
                stop()
            } else {
                player.levelGain += gap > 0 ? step : -step
            }
        }
    }
    // Measures songs coming up ahead of time, so they start at the right volume.
    function warmLevel(track) {
        if (!leveling || !track)
            return
        let id = ""
        if (track.service === "soundcloud" && track.soundcloudId)
            id = "soundcloud:" + track.soundcloudId
        else if (track.service === "bandcamp" && streams[track.key])
            id = "audio:" + streams[track.key].url
        else if (track.streamUrl)
            id = "audio:" + track.streamUrl
        else if (!track.service)
            id = track.youtubeId || (found[track.key] && found[track.key][0]) || ""
        if (id)
            Api.call(Backend.call("loudness", JSON.stringify({ id: id, track: track })), function () {}, function () {})
    }

    property Connections engineEvents: Connections {
        target: player.engine
        function onPlayerStateChanged() {
            if (player.engine.playing && player.current && !player.started) {
                // Paused before it began (see halt()): the player may start anyway.
                if (!player.loading) {
                    player.engine.pause()
                    return
                }
                player.started = true
                player.resumeAt = 0
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
        function onVideoIdChanged() { player.updateLevel() }
        function onPositionChanged() {
            player.recordIfListened()
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
            // A SoundCloud song that won't play (encrypted, which this app's
            // browser can't decode, or SoundCloud's player refused it): find
            // the same song on YouTube instead, as for a catalogue song (and
            // from there, on Bandcamp).
            if (player.current.service === "soundcloud") {
                player.playOffSoundCloud()
                return
            }
            // Another upload of the same song may be allowed to play here.
            if (!player.current.service && player.candidate + 1 < player.candidates.length) {
                player.candidate += 1
                player.engine.load(player.candidates[player.candidate], player.trackStart + player.resumeAt)
                return
            }
            // Only a short message if nothing would play it; the reason goes to the log.
            console.warn("Couldn't play " + player.current.title + ": " + message)
            const failure = "Couldn't play \"" + player.current.title + "\"."
            // No upload will play: look for the same song on SoundCloud or Bandcamp.
            const track = player.current
            if (!track.service && !(track.start > 0) && player.elsewhereTried !== track.key) {
                player.elsewhereTried = track.key
                player.loading = true
                const at = player.index
                player.pendingRequest = Api.call(Backend.call("elsewhere", JSON.stringify({ track: track })), function (song) {
                    player.pendingRequest = -1
                    if (!player.current || player.current.key !== track.key || player.index !== at)
                        return
                    if (!song) {
                        player.fail(failure)
                        return
                    }
                    const q = player.queue.slice()
                    q[at] = song
                    player.queue = q
                    player.resolveCurrent()
                }, function () {
                    player.pendingRequest = -1
                    if (player.current && player.current.key === track.key)
                        player.fail(failure)
                })
                return
            }
            player.fail(failure)
        }
    }

    // Whether the song playing was started from these songs, so a page's big
    // play button shows pause and pauses (or carries on) instead of restarting.
    function playingFrom(contextName, tracks) {
        return current !== null && context === contextName && (tracks || []).some(t => t.key === current.key)
    }
    function playOrToggle(tracks, contextName, route) {
        if (playingFrom(contextName, tracks))
            toggle()
        else
            playTracks(tracks, 0, contextName, route)
    }

    // Whether the music playing was started from this page (an album or a
    // playlist, by its id).
    function playingRoute(pageName, id) {
        return current !== null && !!id && !!contextRoute && contextRoute.page === pageName
            && !!contextRoute.args && String(contextRoute.args.id) === String(id)
    }

    // An album card's play button: whether that album is what's playing
    // (started from its card or its page).
    function playingAlbum(id) {
        return playingRoute("album", id)
    }

    // Explicit songs are left out everywhere while "Hide explicit songs" is on.
    function allowed(track) {
        return !(Api.hideExplicit && track && track.explicit)
    }

    function copy(tracks) {
        return (tracks || []).filter(t => t && t.key && allowed(t)).map(t => Object.assign({}, t))
    }
    // Nothing to play because every song asked for is explicit and hidden.
    function noticeHidden(tracks) {
        if ((tracks || []).some(t => t && t.key))
            Nav.notify("Explicit songs are hidden (see Settings)")
    }

    // Turning on "Hide explicit songs" takes them out of the queue; if one is
    // playing, the next song that isn't explicit plays instead.
    property Connections explicitSetting: Connections {
        target: Api
        function onHideExplicitChanged() {
            if (Api.hideExplicit)
                player.dropExplicit()
        }
    }
    function dropExplicit() {
        if (!queue.some(t => !allowed(t)))
            return
        const skipping = current !== null && !allowed(current)
        // Where the current song (or the next one allowed) ends up.
        const at = queue.slice(0, Math.max(0, index)).filter(allowed).length
        const kept = queue.filter(allowed)
        originalQueue = originalQueue.filter(allowed)
        if (!skipping) {
            queue = kept
            index = index < 0 ? -1 : at
            return
        }
        index = -1
        queue = kept
        if (at < kept.length) {
            jump(at)
        } else {
            if (engine)
                engine.stop()
            started = false
            loading = false
            wantPlaying = false
        }
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
        if (list.length === 0) {
            noticeHidden(tracks)
            return
        }
        contextRoute = route || { page: Nav.current, args: Object.assign({}, Nav.currentArgs) }
        // Counted among the songs kept, so a hidden explicit one starts the next one instead.
        start = (tracks || []).slice(0, Math.max(0, start || 0)).filter(t => t && t.key && allowed(t)).length
        start = Math.max(0, Math.min(start, list.length - 1))
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
        const list = copy(tracks)
        if (list.length === 0) {
            noticeHidden(tracks)
            return
        }
        shuffle = true
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
        // A hidden explicit song still in the queue is passed over.
        while (i >= 0 && i < queue.length && !allowed(queue[i]))
            i += i < index ? -1 : 1
        if (i < 0 || i >= queue.length)
            return
        index = i
        resumeAt = 0
        resolveCurrent()
    }

    function startVideos(videos) {
        candidates = videos
        candidate = 0
        engine.load(videos[0], trackStart + resumeAt)
    }

    function resolveCurrent() {
        const track = current
        if (!track || !engine)
            return
        error = ""
        wantPlaying = true
        skipTimer.stop()
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
        // The song left playing last time starts where it was.
        const from = trackStart + resumeAt
        // SoundCloud songs play in SoundCloud's own player, unless SoundCloud
        // only offers them encrypted and this computer has no Widevine module.
        if (track.service === "soundcloud") {
            if (track.encrypted && !engine.canDecrypt) {
                playOffSoundCloud()
                return
            }
            engine.loadSoundCloud(track.soundcloudId, from)
            // Songs saved before that was noted: ask while the player loads.
            if (track.encrypted === undefined && !engine.canDecrypt) {
                const at = index
                Api.call(Backend.call("soundcloudEncrypted", JSON.stringify({ id: track.soundcloudId })), function (encrypted) {
                    if (encrypted && !started && current && current.key === track.key && index === at)
                        playOffSoundCloud()
                })
            }
            return
        }
        // Mixcloud shows play in Mixcloud's own player.
        if (track.service === "mixcloud") {
            engine.loadMixcloud(track.mixcloudKey, from)
            return
        }
        // The Internet Archive's files stream straight from archive.org.
        if (track.service === "archive") {
            engine.loadAudio(track.streamUrl, from)
            return
        }
        // Bandcamp songs stream from Bandcamp, with a link fetched just before.
        if (track.service === "bandcamp") {
            if (streams[track.key]) {
                engine.loadAudio(streams[track.key].url, from)
                return
            }
            engine.stop()
            pendingRequest = Api.call(Backend.stream(JSON.stringify(track)), function (result) {
                pendingRequest = -1
                rememberStream(result)
                if (current && result.key === current.key)
                    engine.loadAudio(result.url, from)
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

    // The song playing, found on YouTube (and from there, Bandcamp) instead of
    // SoundCloud: kept under its own name and key, so likes and history stay.
    function playOffSoundCloud() {
        const song = Object.assign({}, current)
        delete song.service
        delete song.soundcloudId
        delete song.youtubeId
        delete song.encrypted
        const q = queue.slice()
        q[index] = song
        queue = q
        resolveCurrent()
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
        if (index + 1 < queue.length)
            warmLevel(queue[index + 1])
        // Random radio: fetch more songs before the queue runs out.
        if (radio && index >= queue.length - 2)
            extendRadio()
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
        } else if (loading || skipTimer.running) {
            halt()
        } else if (engine.playerState === 3) {
            wantPlaying = false
            engine.pause()
        } else if (!started || error !== "") {
            resolveCurrent()
        } else {
            wantPlaying = true
            engine.play()
        }
    }

    // Pause while a song is still starting (or about to): it doesn't start,
    // and playing again starts it afresh.
    function halt() {
        skipTimer.stop()
        if (pendingRequest >= 0)
            Api.cancel(pendingRequest)
        pendingRequest = -1
        radioWaiting = false
        loading = false
        started = false
        wantPlaying = false
        engine.pause()
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
        else if (radio) {
            // More songs are on their way (or about to be fetched): play the first when they come.
            radioWaiting = true
            loading = true
            extendRadio()
        } else if (automatic) {
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
        // Not loaded yet (left from last time): it'll start from there.
        if (current && !started && !loading) {
            resumeAt = Math.max(0, seconds)
            sessionSoon.restart()
            return
        }
        sessionSoon.restart()
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
        if (list.length === 0) {
            noticeHidden(tracks)
            return
        }
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
        if (list.length === 0) {
            noticeHidden(tracks)
            return
        }
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
