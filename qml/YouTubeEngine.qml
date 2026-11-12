import QtQuick
import QtCore
import QtWebEngine

// Plays music through YouTube's own embedded player, kept out of sight (as
// Headset did). Nothing is downloaded: YouTube streams it like on youtube.com.
// Some owners don't allow their videos in embedded players; those play on
// youtube.com's own watch page instead, hidden the same way. The same hidden
// page also plays Bandcamp's audio streams and SoundCloud's embedded player.
Item {
    id: engine

    property bool ready: false
    property int playerState: -1    // YouTube's states: -1 unstarted, 0 ended, 1 playing, 2 paused, 3 buffering, 5 cued
    property real position: 0       // seconds
    property real duration: 0       // seconds
    property string videoId: ""
    property bool canDecrypt: false // encrypted songs can play (SoundCloud's labels' releases)
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

    // The Mixcloud show found unable to load its audio, and when the current one was loaded.
    property string unreachable: ""
    property double loadedAt: 0

    function reset(id, start) {
        unreachable = ""
        loadedAt = Date.now()
        videoId = id
        position = start || 0
        duration = 0
        playerState = -1
    }
    function load(id, start) {
        reset(id, start)
        if (isWatchOnly(id)) {
            openWatchPage(id, start)
            return
        }
        leaveWatchPage()
        call("earshotLoad(" + JSON.stringify(id) + "," + (start || 0) + ")")
    }
    // A plain audio link (Bandcamp's streams).
    function loadAudio(url, start) {
        reset("audio:" + url, start)
        leaveWatchPage()
        call("earshotLoadAudio(" + JSON.stringify(url) + "," + (start || 0) + ")")
    }
    // A song in SoundCloud's own embedded player.
    function loadSoundCloud(trackId, start) {
        reset("soundcloud:" + trackId, start)
        leaveWatchPage()
        call("earshotLoadSoundCloud(" + JSON.stringify(String(trackId)) + "," + (start || 0) + ")")
    }
    // A show in Mixcloud's own embedded player.
    function loadMixcloud(key, start) {
        reset("mixcloud:" + key, start)
        leaveWatchPage()
        call("earshotLoadMixcloud(" + JSON.stringify(key) + "," + (start || 0) + ")")
    }
    function play() { watchMode ? watchCall("p.playVideo()") : call("earshotPlay()") }
    function pause() { watchMode ? watchCall("p.pauseVideo()") : call("earshotPause()") }
    function stop() {
        videoId = ""
        leaveWatchPage()
        call("earshotStop()")
        playerState = -1
    }
    function seek(seconds) {
        position = seconds
        if (watchMode)
            watchCall("p.seekTo(" + seconds + ", true)")
        else
            call("earshotSeek(" + seconds + ")")
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
        call("earshotStop()")
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

    // Some Mixcloud shows' audio is a plain http link to another site, which
    // the player can't fetch from inside a secure page: ask for it over https.
    readonly property string html: '<!doctype html><html><head><meta charset="utf-8">'
        + '<meta http-equiv="Content-Security-Policy" content="upgrade-insecure-requests">'
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
        // Which player has the current song: "yt" YouTube, "audio" a plain stream (Bandcamp), "sc" SoundCloud.
        + 'var mode="yt";'
        // Whether encrypted songs can play here (Google's Widevine module was found).
        + 'var drm=false;try{navigator.requestMediaKeySystemAccess("com.widevine.alpha",[{initDataTypes:["cenc"],'
        + '  audioCapabilities:[{contentType:"audio/mp4; codecs=\\"mp4a.40.2\\""}]}]).then(function(){drm=true;},function(){});}catch(e){}'
        + 'function quiet(next){'
        + '  if(next!=="yt"&&ready)player.stopVideo();'
        + '  if(next!=="audio"&&audio.src){audio.pause();audio.removeAttribute("src");audio.load();}'
        + '  if(next!=="sc"&&sc)sc.pause();'
        + '  if(next!=="mc"&&mc)mc.pause();'
        + '  mode=next;ended=false;lastError=0;}'
        + 'function earshotLoad(id,start){quiet("yt");if(!ready){pending={id:id,start:start};return;}'
        + '  player.loadVideoById({videoId:id,startSeconds:start||0,suggestedQuality:"small"});}'
        // Plain audio
        + 'var audio=new Audio();audio.preload="auto";'
        + 'audio.addEventListener("ended",function(){if(mode==="audio")ended=true;});'
        + 'audio.addEventListener("error",function(){if(mode==="audio"&&audio.src)lastError=1001;});'
        + 'function earshotLoadAudio(url,start){quiet("audio");audio.src=url;audio.volume=volume/100;'
        + '  if(start)audio.currentTime=start;audio.play().catch(function(){});}'
        // SoundCloud's widget, made the first time a SoundCloud song plays
        + 'var sc=null,scState=-1,scPos=0,scDur=0,scStart=0;'
        + 'function scUrl(id){return "https://api.soundcloud.com/tracks/"+id;}'
        + 'function scBegin(){sc.setVolume(volume);sc.getDuration(function(d){scDur=d;});if(scStart>0)sc.seekTo(scStart*1000);sc.play();}'
        + 'var scLoaded=0;'
        + 'function earshotLoadSoundCloud(id,start){quiet("sc");scState=-1;scPos=(start||0)*1000;scDur=0;scStart=start||0;scLoaded=Date.now();'
        + '  if(typeof SC==="undefined"){lastError=1002;return;}'
        + '  if(!sc){var f=document.createElement("iframe");f.allow="autoplay; encrypted-media";f.style.cssText="position:fixed;left:0;top:0;width:160px;height:90px;border:0";'
        + '    f.src="https://w.soundcloud.com/player/?url="+encodeURIComponent(scUrl(id))+"&auto_play=true&visual=false";'
        + '    document.body.appendChild(f);sc=SC.Widget(f);var E=SC.Widget.Events;'
        + '    sc.bind(E.READY,scBegin);'
        + '    sc.bind(E.PLAY,function(){scState=1;sc.getDuration(function(d){scDur=d;});});'
        + '    sc.bind(E.PAUSE,function(){if(scState!==0)scState=2;});'
        + '    sc.bind(E.PLAY_PROGRESS,function(e){scPos=e.currentPosition;if(scState<1)scState=1;});'
        + '    sc.bind(E.FINISH,function(){scState=0;if(mode==="sc")ended=true;});'
        + '    sc.bind(E.ERROR,function(){if(mode==="sc")lastError=1002;});'
        + '  }else{sc.load(scUrl(id),{auto_play:true,visual:false,callback:scBegin});}}'
        // Mixcloud's widget, made the first time a Mixcloud show plays
        + 'var mc=null,mcFrame=null,mcKey="",mcState=-1,mcPos=0,mcDur=0,mcStart=0,mcLoaded=0;'
        + 'function mcBegin(){if(mode!=="mc"||!mc)return;if(mcStart>0)mc.seek(mcStart);mc.play();try{if(mc.setVolume)mc.setVolume(volume/100);}catch(e){}}'
        + 'function earshotLoadMixcloud(key,start){quiet("mc");mcState=-1;mcPos=start||0;mcDur=0;mcStart=start||0;mcLoaded=Date.now();mcKey=key;'
        + '  if(typeof Mixcloud==="undefined"){lastError=1003;return;}'
        + '  if(!mcFrame){var first=key;mcFrame=document.createElement("iframe");mcFrame.allow="autoplay; encrypted-media";mcFrame.style.cssText="position:fixed;left:0;top:0;width:160px;height:90px;border:0";'
        + '    mcFrame.src="https://player-widget.mixcloud.com/?hide_cover=1&mini=1&autoplay=1&feed="+encodeURIComponent(key);'
        + '    mcFrame.onload=function(){mcFrame.onload=null;var w=Mixcloud.PlayerWidget(mcFrame);'
        + '    w.ready.then(function(){mc=w;var E=w.events;'
        + '      E.progress.on(function(p,d){mcPos=p;if(d)mcDur=d;if(mode==="mc"&&mcState!==2&&mcState!==0)mcState=1;});'
        + '      E.play.on(function(){mcState=1;});'
        + '      E.pause.on(function(){if(mcState!==0)mcState=2;});'
        + '      E.buffering.on(function(){if(mcState!==2)mcState=3;});'
        + '      E.ended.on(function(){mcState=0;if(mode==="mc")ended=true;});'
        + '      E.error.on(function(e){if(mode==="mc")lastError=1003;});'
        + '      if(mcKey!==first)mc.load(mcKey,true).then(mcBegin);else mcBegin();});};document.body.appendChild(mcFrame);'
        + '  }else if(mc){mc.load(key,true).then(mcBegin);}}'
        // The same controls for whichever player is in use
        + 'function earshotPlay(){if(mode==="audio")audio.play().catch(function(){});else if(mode==="sc"){if(sc)sc.play();}else if(mode==="mc"){if(mc)mc.play();}else if(ready)player.playVideo();}'
        + 'function earshotPause(){if(mode==="audio")audio.pause();else if(mode==="sc"){if(sc)sc.pause();}else if(mode==="mc"){if(mc)mc.pause();}else if(ready)player.pauseVideo();}'
        + 'function earshotSeek(s){if(mode==="audio")audio.currentTime=s;else if(mode==="sc"){if(sc){sc.seekTo(s*1000);scPos=s*1000;}}else if(mode==="mc"){if(mc){mc.seek(s);mcPos=s;}}else if(ready)player.seekTo(s,true);}'
        + 'function earshotStop(){var was=mode;quiet("none");mode=was;}'
        + 'function earshotVolume(v){volume=v;if(ready)player.setVolume(v);audio.volume=v/100;if(sc)sc.setVolume(v);try{if(mc&&mc.setVolume)mc.setVolume(v/100);}catch(e){}}'
        + 'function earshotStatus(){var s;'
        + '  if(mode==="audio")s={ready:true,state:audio.ended?0:audio.paused?2:audio.readyState<3?3:1,time:audio.currentTime||0,duration:isFinite(audio.duration)?audio.duration:0};'
        + '  else if(mode==="sc"){s={ready:true,state:scState,time:scPos/1000,duration:scDur/1000};'
        + '    if(scState<1&&Date.now()-scLoaded>30000){scLoaded=Date.now();lastError=1002;}}'
        + '  else if(mode==="mc"){s={ready:true,state:mcState,time:mcPos,duration:mcDur};'
        + '    if(mcState<1&&mcState!==2&&Date.now()-mcLoaded>30000){mcLoaded=Date.now();lastError=1003;}}'
        + '  else if(!ready)return JSON.stringify({ready:false,drm:drm});'
        + '  else s={ready:true,state:player.getPlayerState(),time:player.getCurrentTime()||0,duration:player.getDuration()||0};'
        + '  s.drm=drm;s.ended=ended;s.error=lastError;ended=false;lastError=0;return JSON.stringify(s);}'
        + '</script>'
        + '<script src="https://www.youtube.com/iframe_api"></script>'
        + '<script src="https://w.soundcloud.com/player/api.js"></script>'
        + '<script src="https://widget.mixcloud.com/media/js/widgetApi.js"></script>'
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
        // A Mixcloud show whose audio is kept on another site that won't send
        // it can never start: say so at once instead of waiting 30 seconds.
        // (Handling this signal stops Qt logging the page's messages itself.)
        onJavaScriptConsoleMessage: (level, message, line, source) => {
            console.log("js:", message)
            // (The first moment may still bring messages about the show before.)
            if (engine.videoId.startsWith("mixcloud:") && engine.videoId !== engine.unreachable
                    && Date.now() - engine.loadedAt > 1000
                    && engine.playerState !== 1 && engine.playerState !== 2
                    && /requested an insecure resource|blocked by CORS policy/.test(message)) {
                engine.unreachable = engine.videoId
                // Mixcloud's player keeps retrying: start a fresh one for the next show.
                engine.call("earshotStop();mcLoaded=Infinity;if(mcFrame){mcFrame.remove();mcFrame=null;mc=null;}")
                engine.failed(1004, engine.describe(1004))
            }
        }
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
            canDecrypt = status.drm === true
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
        case 1001: return "Bandcamp's audio stopped loading."
        case 1002: return "SoundCloud's player couldn't play this song."
        case 1003: return "Mixcloud's player couldn't play this show."
        case 1004: return "Mixcloud's player can't load this show: its audio is kept on another site, which won't send it."
        default: return "YouTube's player stopped (error " + code + ")."
        }
    }
}
