import QtQuick
import QtCore
import QtWebEngine

// Plays music through YouTube's own embedded player, kept out of sight (as
// Headset did). Nothing is downloaded: YouTube streams it like on youtube.com.
// Some owners don't allow their videos in embedded players; those play on
// youtube.com's own watch page instead, hidden the same way.
Item {
    id: engine

    property bool ready: false
    property int playerState: -1    // YouTube's states: -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued
    property real position: 0       // seconds
    property real duration: 0       // seconds
    property string videoId: ""
    readonly property bool playing: playerState === 1
    readonly property bool buffering: playerState === 3 || playerState === -1

    // The current video is playing on youtube.com's watch page, not the embedded player.
    property bool watchMode: false
    property bool watchAd: false     // YouTube is showing an ad before the song
    property int volume: 80
    property double watchOpened: 0
    property int watchState: -1

    signal ended()
    signal failed(int code, string message)

    // A small real view (not hidden), since browsers pause media in hidden pages.
    width: 160
    height: 90
    opacity: 0.01
    enabled: false

    // Videos already known to refuse embedding go straight to the watch page.
    property Settings memory: Settings {
        category: "youtube"
        property string watchOnly: "[]"
    }
    function isWatchOnly(id) { return JSON.parse(memory.watchOnly).indexOf(id) >= 0 }
    function rememberWatchOnly(id) {
        const list = JSON.parse(memory.watchOnly).filter(known => known !== id)
        list.unshift(id)
        memory.watchOnly = JSON.stringify(list.slice(0, 500))
    }

    function load(id, start) {
        videoId = id
        position = start || 0
        duration = 0
        playerState = -1
        if (isWatchOnly(id)) {
            openWatchPage(id, start)
            return
        }
        leaveWatchPage()
        call("earshotLoad(" + JSON.stringify(id) + "," + (start || 0) + ")")
    }
    function play() { watchMode ? watchCall("p.playVideo()") : call("player && player.playVideo()") }
    function pause() { watchMode ? watchCall("p.pauseVideo()") : call("player && player.pauseVideo()") }
    function stop() {
        videoId = ""
        leaveWatchPage()
        call("player && player.stopVideo()")
        playerState = -1
    }
    function seek(seconds) {
        position = seconds
        if (watchMode)
            watchCall("p.seekTo(" + seconds + ", true)")
        else
            call("player && player.seekTo(" + seconds + ", true)")
    }
    function setVolume(percent) {
        volume = Math.round(percent)
        call("earshotVolume(" + volume + ")")
        watchCall("p.unMute(); p.setVolume(" + volume + ")")
    }

    function call(script) {
        if (view.loadProgress === 100)
            view.runJavaScript(script)
        else
            pendingScripts.push(script)
    }
    property var pendingScripts: []

    // youtube.com's watch page: its player element has the same controls as the embedded one.
    function openWatchPage(id, start) {
        call("player && player.stopVideo()")
        watchMode = true
        watchAd = false
        watchState = -1
        watchOpened = Date.now()
        const at = start > 0 ? "&t=" + Math.floor(start) + "s" : ""
        watchView.url = "https://www.youtube.com/watch?v=" + id + at
    }
    function leaveWatchPage() {
        if (!watchMode)
            return
        watchMode = false
        watchAd = false
        watchView.url = "about:blank"
    }
    function watchCall(script) {
        if (watchMode)
            watchView.runJavaScript("(function(){var p=document.getElementById('movie_player');if(p&&p.playVideo){" + script + "}})()")
    }

    readonly property string html: '<!doctype html><html><head><meta charset="utf-8">'
        + '<style>html,body{margin:0;background:#000;overflow:hidden}</style></head><body>'
        + '<div id="player"></div>'
        + '<script>'
        + 'var player=null, ready=false, pending=null, volume=80, ended=false, lastError=0;'
        + 'function onYouTubeIframeAPIReady(){'
        + '  player=new YT.Player("player",{width:"160",height:"90",'
        + '    playerVars:{autoplay:1,controls:0,disablekb:1,fs:0,playsinline:1,rel:0,iv_load_policy:3,origin:location.origin},'
        + '    events:{'
        + '      onReady:function(){ready=true;player.setVolume(volume);if(pending){earshotLoad(pending.id,pending.start);pending=null;}},'
        + '      onStateChange:function(e){if(e.data===0)ended=true;},'
        + '      onError:function(e){lastError=e.data;}'
        + '    }});'
        + '}'
        + 'function earshotLoad(id,start){if(!ready){pending={id:id,start:start};return;}ended=false;lastError=0;'
        + '  player.loadVideoById({videoId:id,startSeconds:start||0,suggestedQuality:"small"});}'
        + 'function earshotVolume(v){volume=v;if(ready)player.setVolume(v);}'
        + 'function earshotStatus(){if(!ready)return JSON.stringify({ready:false});'
        + '  var s={ready:true,state:player.getPlayerState(),time:player.getCurrentTime()||0,duration:player.getDuration()||0,ended:ended,error:lastError};'
        + '  ended=false;lastError=0;return JSON.stringify(s);}'
        + '</script>'
        + '<script src="https://www.youtube.com/iframe_api"></script>'
        + '</body></html>'

    // What the watch page's player is doing; ads count as still loading.
    readonly property string watchStatusScript: "(function(){"
        + "var p=document.getElementById('movie_player');"
        + "var error=document.querySelector('.ytp-error-content-wrap-reason, .ytp-error');"
        + "if(error&&error.offsetParent!==null&&error.innerText.trim())return JSON.stringify({error:error.innerText.trim().split('\\n')[0]});"
        + "if(!p||!p.getPlayerState)return JSON.stringify({ready:false});"
        + "var ad=p.classList.contains('ad-showing');"
        + "return JSON.stringify({ready:true,ad:ad,state:p.getPlayerState(),time:ad?-1:p.getCurrentTime(),duration:ad?0:p.getDuration(),muted:p.isMuted()});"
        + "})()"

    WebEngineView {
        id: view
        anchors.fill: parent
        settings.playbackRequiresUserGesture: false
        settings.showScrollBars: false
        // YouTube's embedded player needs to know which site it's on.
        Component.onCompleted: loadHtml(engine.html, "https://earshot.app/")
        onLoadingChanged: request => {
            if (request.status === WebEngineView.LoadSucceededStatus) {
                const scripts = engine.pendingScripts
                engine.pendingScripts = []
                for (const script of scripts)
                    view.runJavaScript(script)
            }
        }
    }

    WebEngineView {
        id: watchView
        anchors.fill: parent
        visible: engine.watchMode
        settings.playbackRequiresUserGesture: false
        settings.showScrollBars: false
        // Its own profile, so youtube.com's cookies (consent, settings) are kept between runs.
        // (WebEngineProfilePrototype, Qt's newer way to make one, crashes here as of Qt 6.12.)
        profile: WebEngineProfile {
            storageName: "youtube"
            offTheRecord: false
            // Present as the Chrome it is, without the "QtWebEngine" tag.
            Component.onCompleted: httpUserAgent = httpUserAgent.replace(/ QtWebEngine\/\S+/, "")
        }
        url: "about:blank"
    }

    // YouTube's player reports nothing on its own, so ask it a few times a second.
    Timer {
        interval: 250
        repeat: true
        running: view.loadProgress === 100
        onTriggered: engine.watchMode ? engine.pollWatchPage() : engine.pollEmbedded()
    }

    function pollEmbedded() {
        view.runJavaScript("earshotStatus()", function (result) {
            if (!result || watchMode)
                return
            const status = JSON.parse(result)
            ready = status.ready
            if (!status.ready)
                return
            if (videoId !== "") {
                playerState = status.state
                position = status.time
                if (status.duration > 0)
                    duration = status.duration
            }
            if (status.error === 101 || status.error === 150 || status.error === 153) {
                // Not allowed in embedded players: play it on youtube.com's page instead.
                console.info("Playing", videoId, "on youtube.com's watch page; its owner blocks embedding")
                rememberWatchOnly(videoId)
                openWatchPage(videoId, position)
            } else if (status.error) {
                failed(status.error, describe(status.error))
            } else if (status.ended && videoId !== "") {
                ended()
            }
        })
    }

    function pollWatchPage() {
        const id = videoId
        watchView.runJavaScript(watchStatusScript, function (result) {
            if (!watchMode || id !== videoId || !result)
                return
            const status = JSON.parse(result)
            if (status.error) {
                leaveWatchPage()
                failed(-1, "YouTube won't play this video here: " + status.error)
                return
            }
            if (!status.ready || status.state === -1) {
                if (Date.now() - watchOpened > 30000) {
                    leaveWatchPage()
                    failed(-2, "youtube.com's page didn't start this video.")
                }
                return
            }
            watchAd = status.ad
            if (status.ad) {
                playerState = 3
                return
            }
            if (status.muted)
                watchCall("p.unMute(); p.setVolume(" + volume + ")")
            playerState = status.state
            position = status.time
            if (status.duration > 0)
                duration = status.duration
            if (status.state === 0 && watchState !== 0) {
                // Don't let youtube.com move on to a video of its choosing.
                watchCall("p.pauseVideo()")
                ended()
            }
            if (watchState === -1)
                watchCall("p.unMute(); p.setVolume(" + volume + ")")
            watchState = status.state
        })
    }

    function describe(code) {
        switch (code) {
        case 2: return "YouTube didn't recognise this video."
        case 5: return "YouTube's player couldn't play this video."
        case 100: return "This video was removed or made private."
        case 101:
        case 150:
        case 153: return "This video's owner doesn't allow it to play outside youtube.com."
        default: return "YouTube's player stopped (error " + code + ")."
        }
    }
}
