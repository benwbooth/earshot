pragma Singleton
import QtQuick

QtObject {
    readonly property color background: "#0d0f14"
    readonly property color sidebar: "#090b0f"
    readonly property color surface: "#151821"
    readonly property color surfaceRaised: "#1c2030"
    readonly property color surfaceHover: "#232838"
    readonly property color border: "#262b3a"
    readonly property color text: "#f2f3f7"
    readonly property color textMuted: "#9aa1b5"
    readonly property color textFaint: "#636a80"
    readonly property color accent: "#ff5f7e"
    readonly property color accentHover: "#ff7a93"
    readonly property color accent2: "#7a5cff"
    readonly property color success: "#3ddc97"
    readonly property color danger: "#ff6b6b"

    readonly property string fontFamily: "Inter, Noto Sans, Cantarell, sans-serif"
    readonly property int fontSmall: 12
    readonly property int fontBody: 14
    readonly property int fontLarge: 16
    readonly property int fontTitle: 22
    readonly property int fontHero: 40

    readonly property int radius: 10
    readonly property int radiusLarge: 16
    readonly property int pagePadding: 32
    readonly property int gap: 16

    function duration(seconds) {
        seconds = Math.max(0, Math.floor(seconds || 0))
        const h = Math.floor(seconds / 3600)
        const m = Math.floor((seconds % 3600) / 60)
        const s = seconds % 60
        const ss = s < 10 ? "0" + s : "" + s
        if (h > 0)
            return h + ":" + (m < 10 ? "0" + m : m) + ":" + ss
        return m + ":" + ss
    }

    function longDuration(seconds) {
        const minutes = Math.round((seconds || 0) / 60)
        if (minutes < 60)
            return minutes + " min"
        return Math.floor(minutes / 60) + " hr " + (minutes % 60) + " min"
    }

    function icon(name) {
        return "qrc:/qt/qml/Earshot/qml/icons/" + name + ".svg"
    }
}
