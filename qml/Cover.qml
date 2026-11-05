import QtQuick
import QtQuick.Effects
import Earshot

// Artwork with rounded corners and a soft gradient while it loads (or when there is none).
Item {
    id: root
    property string source
    property real radius: 8
    property string seed: ""
    property bool round: false
    property string placeholderIcon: "music"

    readonly property var swatches: [["#ff5f7e", "#7a5cff"], ["#3db8ff", "#3ddc97"], ["#ff8a3d", "#ff5f7e"], ["#7a5cff", "#3db8ff"], ["#f5c542", "#ff8a3d"], ["#3ddc97", "#7a5cff"]]
    readonly property var colors: {
        let h = 0
        const text = seed || source || ""
        for (let i = 0; i < text.length; ++i)
            h = (h * 31 + text.charCodeAt(i)) >>> 0
        return swatches[h % swatches.length]
    }

    Rectangle {
        id: placeholder
        anchors.fill: parent
        radius: root.round ? width / 2 : root.radius
        visible: image.status !== Image.Ready
        gradient: Gradient {
            orientation: Gradient.Vertical
            GradientStop { position: 0; color: Qt.darker(root.colors[0], 1.6) }
            GradientStop { position: 1; color: Qt.darker(root.colors[1], 2.2) }
        }
        Icon {
            anchors.centerIn: parent
            name: root.placeholderIcon
            size: Math.max(14, Math.min(parent.width, parent.height) * 0.32)
            color: Qt.rgba(1, 1, 1, 0.55)
        }
    }

    Image {
        id: image
        anchors.fill: parent
        source: root.source
        fillMode: Image.PreserveAspectCrop
        asynchronous: true
        cache: true
        sourceSize: Qt.size(Math.max(64, root.width * 2), Math.max(64, root.height * 2))
        visible: false
    }

    Rectangle {
        id: mask
        anchors.fill: parent
        radius: root.round ? width / 2 : root.radius
        visible: false
        layer.enabled: true
    }

    MultiEffect {
        anchors.fill: parent
        source: image
        visible: image.status === Image.Ready
        maskEnabled: true
        maskSource: mask
        maskThresholdMin: 0.5
        maskSpreadAtMin: 1.0
        opacity: visible ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 200 } }
    }
}
