pragma Singleton
import QtQuick
import QtCore
import Earshot

// Turns the backend's request ids into callbacks.
QtObject {
    id: api

    property var pending: ({})
    property var homeCache: null
    property var radioCache: ({})
    property string lastSearch: ""
    property string lastStation: "listentothis"
    property var bandcampTags: null
    property var forYouCache: null
    // Tags seen on Bandcamp albums opened this session, for the tag filter.
    property var seenBandcampTags: []
    function rememberBandcampTags(list) {
        const known = {}
        for (const tag of seenBandcampTags) known[tag.slug] = true
        const added = (list || []).filter(tag => !known[tag.slug])
        if (added.length > 0)
            seenBandcampTags = seenBandcampTags.concat(added)
    }

    // Albums shown as a list instead of a grid of covers (remembered).
    property bool albumList: false
    // The now-playing card when a song starts: "always", "background" (only
    // while Earshot isn't the window in front) or "never" (the default).
    property string nowPlayingCard: "never"
    // Notices ("Added to playlist", errors) as the desktop's notifications
    // instead of inside the window. Off by default.
    property bool desktopNotices: false
    // Songs marked explicit are left out of song lists and never played.
    property bool hideExplicit: false
    // YouTube searches list music (songs, music videos, albums) before other videos.
    property bool preferMusic: true
    property Settings view: Settings {
        category: "view"
        property alias albumList: api.albumList
        property alias songCard: api.nowPlayingCard
        property alias desktopNotices: api.desktopNotices
        property alias hideExplicit: api.hideExplicit
        property alias preferMusic: api.preferMusic
    }

    // The biggest versions of a cover the service has, biggest first (the art
    // viewer tries each in turn): YouTube's maxres thumbnail, Bandcamp's
    // 1200-pixel art, YouTube Music's and Deezer's 1200/1000-pixel covers…
    function bigArt(url) {
        const sizes = []
        const youtube = /^(https?:\/\/i\d?\.ytimg\.com\/vi(?:_webp)?\/[^/]+\/)[^/?]+/.exec(url)
        if (youtube)
            for (const name of ["maxresdefault.jpg", "sddefault.jpg", "hqdefault.jpg"])
                sizes.push(youtube[1].replace("/vi_webp/", "/vi/") + name)
        else if (/bcbits\.com\/img\/.+_\d+\.jpg$/.test(url))
            sizes.push(url.replace(/_\d+\.jpg$/, "_10.jpg"))
        else if (/sndcdn\.com/.test(url))
            sizes.push(url.replace(/-(large|t\d+x\d+|crop|small|badge|tiny|mini)\./, "-t500x500."))
        else if (/dzcdn\.net\/images\//.test(url))
            sizes.push(url.replace(/\/\d+x\d+-/, "/1000x1000-"))
        else if (/googleusercontent\.com|ggpht\.com/.test(url))
            sizes.push(url.replace(/=w\d+-h\d+/, "=w1200-h1200").replace(/=s\d+/, "=s1200"))
        else if (/thumbnailer\.mixcloud\.com\/unsafe\/\d+x\d+\//.test(url))
            sizes.push(url.replace(/\/unsafe\/\d+x\d+\//, "/unsafe/1000x1000/"))
        sizes.push(url)
        return sizes
    }

    // Where a song lives on the web: its YouTube video (at its chapter), or its
    // page on Bandcamp, SoundCloud, Mixcloud or the Internet Archive.
    function songPage(track) {
        if (!track)
            return ""
        if (track.pageUrl)
            return track.pageUrl
        const video = track.youtubeId || (Player.found[track.key] && Player.found[track.key][0])
        if (video)
            return "https://www.youtube.com/watch?v=" + video + (track.start > 0 ? "&t=" + Math.floor(track.start) + "s" : "")
        if (track.key && track.key.indexOf("dz:") === 0)
            return "https://www.deezer.com/track/" + track.key.substring(3)
        return ""
    }
    function albumPage(album) {
        if (!album)
            return ""
        if (album.pageUrl)
            return album.pageUrl
        const id = String(album.id || "")
        if (id.indexOf("yt:pl:") === 0)
            return "https://www.youtube.com/playlist?list=" + id.substring(6)
        if (id.indexOf("yt:v:") === 0)
            return "https://www.youtube.com/watch?v=" + id.substring(5)
        if (/^\d+$/.test(id))
            return "https://www.deezer.com/album/" + id
        return ""
    }
    // Where a song plays from (the site's name and logo), for every song row.
    // Catalogue songs (and Reddit's) play from YouTube.
    function source(track) {
        const services = {
            bandcamp: { name: "Bandcamp", logo: "source-bandcamp" },
            soundcloud: { name: "SoundCloud", logo: "source-soundcloud" },
            mixcloud: { name: "Mixcloud", logo: "source-mixcloud" },
            archive: { name: "Internet Archive", logo: "source-archive" }
        }
        return (track && services[track.service]) || { name: "YouTube", logo: "source-youtube" }
    }

    // "YouTube", "Bandcamp"… for "Open on …".
    function siteName(url) {
        const sites = [["youtube.com", "YouTube"], ["soundcloud.com", "SoundCloud"], ["bandcamp.com", "Bandcamp"],
                       ["mixcloud.com", "Mixcloud"], ["archive.org", "the Internet Archive"], ["deezer.com", "Deezer"]]
        for (const [host, name] of sites)
            if (url.indexOf(host) >= 0)
                return name
        return "the web"
    }
    // Opens a page in the system's web browser.
    function openWeb(url) {
        if (url)
            Qt.openUrlExternally(url)
    }

    // Fetches an album's songs and plays them from the top.
    function playAlbum(id, title, shuffle) {
        Nav.notify("Loading " + (title || "album") + "…")
        call(Backend.album(id), function (album) {
            const route = { page: "album", args: { id: id } }
            if (shuffle)
                Player.shuffleTracks(album.tracks, album.title, route)
            else
                Player.playTracks(album.tracks, 0, album.title, route)
        }, function (message) { Nav.notify(message) })
    }

    function call(requestId, onReply, onError) {
        pending[requestId] = { ok: onReply, fail: onError }
        return requestId
    }

    function cancel(requestId) {
        delete pending[requestId]
    }

    property Connections connections: Connections {
        target: Backend
        function onReply(request, json) {
            const handler = api.pending[request]
            if (!handler)
                return
            delete api.pending[request]
            let data = null
            try {
                data = JSON.parse(json)
            } catch (error) {
                if (handler.fail) handler.fail("Unexpected answer")
                return
            }
            // A page may have been closed before its answer arrived.
            try {
                if (handler.ok) handler.ok(data)
            } catch (error) {
                if (!(error instanceof TypeError))
                    throw error
            }
        }
        function onFailed(request, message) {
            const handler = api.pending[request]
            if (!handler)
                return
            delete api.pending[request]
            try {
                if (handler.fail) handler.fail(message)
            } catch (error) {
                if (!(error instanceof TypeError))
                    throw error
            }
        }
    }

    function playlists() {
        return JSON.parse(Backend.playlists())
    }

    function playlist(id) {
        return JSON.parse(Backend.playlist(id))
    }
}
