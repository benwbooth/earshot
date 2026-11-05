import QtQuick
import Earshot

// A coloured tile for a genre, era or radio station.
Rectangle {
    id: tile
    property string title
    property string subtitle
    property string image
    property color tint: Theme.accent2
    signal clicked()

    implicitHeight: 96
    radius: Theme.radius
    clip: true
    gradient: Gradient {
        orientation: Gradient.Horizontal
        GradientStop { position: 0; color: Qt.darker(tile.tint, hover.hovered ? 1.5 : 1.9) }
        GradientStop { position: 1; color: Qt.darker(tile.tint, hover.hovered ? 2.4 : 3.0) }
    }

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: tile.clicked() }

    Cover {
        visible: tile.image !== ""
        width: tile.height * 0.82
        height: width
        anchors.right: parent.right
        anchors.rightMargin: -width * 0.18
        anchors.bottom: parent.bottom
        anchors.bottomMargin: -width * 0.12
        rotation: 18
        radius: 8
        source: tile.image
        seed: tile.title
    }

    Column {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.margins: 16
        anchors.right: parent.right
        anchors.rightMargin: tile.image !== "" ? tile.height * 0.7 : 16
        spacing: 4
        Text {
            width: parent.width
            text: tile.title
            color: "#ffffff"
            font.pixelSize: Theme.fontLarge + 2
            font.weight: Font.Bold
            elide: Text.ElideRight
        }
        Text {
            width: parent.width
            visible: text !== ""
            text: tile.subtitle
            color: Qt.rgba(1, 1, 1, 0.75)
            font.pixelSize: Theme.fontSmall + 1
            wrapMode: Text.WordWrap
            maximumLineCount: 2
            elide: Text.ElideRight
        }
    }
}
