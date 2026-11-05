import QtQuick
import QtWebEngine

// Plays music through YouTube's own embedded player, kept out of sight (as
// Headset did). Nothing is downloaded: YouTube streams it like on youtube.com.
Item {
    id: engine

    property bool ready: false
    property int playerState: -1    // YouTube's states: -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued
    property real position: 0       // seconds
    property real duration: 0       // seconds
    property string videoId: ""
    readonly property bool playing: playerState === 1
    readonly property bool buffering: playerState === 3 || playerState === -1

    signal ended()
    signal failed(int code, string message)

    // A small real view (not hidden), since browsers pause media in hidden pages.
    width: 160
    height: 90
    opacity: 0.01
    enabled: false

    function load(id, start) {
        videoId = id
        position = start || 0
        duration = 0
        playerState = -1
        call("earshotLoad(" + JSON.stringify(id) + "," + (start || 0) + ")")
    }
    function play() { call("player && player.playVideo()") }
    function pause() { call("player && player.pauseVideo()") }
    function stop() {
        videoId = ""
        call("player && player.stopVideo()")
        playerState = -1
    }
    function seek(seconds) {
        position = seconds
        call("player && player.seekTo(" + seconds + ", true)")
    }
    function setVolume(percent) { call("earshotVolume(" + Math.round(percent) + ")") }

    function call(script) {
        if (view.loadProgress === 100)
            view.runJavaScript(script)
        else
            pendingScripts.push(script)
    }
    property var pendingScripts: []

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

    // YouTube's player reports nothing on its own, so ask it a few times a second.
    Timer {
        interval: 250
        repeat: true
        running: view.loadProgress === 100
        onTriggered: view.runJavaScript("earshotStatus()", function (result) {
            if (!result)
                return
            const status = JSON.parse(result)
            engine.ready = status.ready
            if (!status.ready)
                return
            if (engine.videoId !== "") {
                engine.playerState = status.state
                engine.position = status.time
                if (status.duration > 0)
                    engine.duration = status.duration
            }
            if (status.error)
                engine.failed(status.error, engine.describe(status.error))
            else if (status.ended && engine.videoId !== "")
                engine.ended()
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
