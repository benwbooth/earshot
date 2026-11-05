import QtQuick
import QtQuick.Layouts
import Earshot

// An album (or artist, or playlist) as a card: artwork, title and a line below.
// In list view it's a row instead: small artwork with the text beside it.
Item {
    id: card
    property string title
    property string subtitle
    property string image
    property string seed: title
    property bool round: false
    property bool playable: true
    property string placeholderIcon: "album"
    readonly property bool listMode: Api.albumList
    readonly property int thumb: 56

    signal clicked()
    signal playClicked()

    implicitHeight: listMode ? Math.max(thumb + 16, titleText.implicitHeight + 4 + subtitleText.implicitHeight + 16)
                             : art.height + 12 + titleText.implicitHeight + 4 + subtitleText.implicitHeight + 8

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: card.clicked() }

    Rectangle {
        anchors.fill: parent
        anchors.margins: card.listMode ? -4 : -10
        radius: Theme.radius + 2
        color: hover.hovered ? Theme.surfaceHover : "transparent"
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    Cover {
        id: art
        x: card.listMode ? 8 : 0
        y: card.listMode ? (card.height - height) / 2 : 0
        width: card.listMode ? card.thumb : parent.width
        height: width
        source: card.image
        seed: card.seed
        radius: card.listMode ? 6 : Theme.radius
        round: card.round
        placeholderIcon: card.placeholderIcon
    }

    IconButton {
        id: playButton
        visible: card.playable
        filled: true
        iconName: "play"
        iconSize: card.listMode ? 16 : 20
        x: card.listMode ? card.width - width - 8 : art.width - width - 10
        y: card.listMode ? (card.height - height) / 2 : art.height - height - (hover.hovered ? 10 : 2)
        opacity: hover.hovered ? 1 : 0
        Behavior on opacity { NumberAnimation { duration: 140 } }
        Behavior on y { enabled: !card.listMode; NumberAnimation { duration: 140 } }
        onClicked: card.playClicked()
    }

    readonly property real textX: listMode ? art.x + thumb + 14 : 0
    readonly property real textWidth: listMode ? width - textX - (playable ? playButton.width + 16 : 8) : width
    readonly property real textY: listMode ? (height - titleText.implicitHeight - 4 - subtitleText.implicitHeight) / 2 : art.height + 12

    Text {
        id: titleText
        x: card.textX
        y: card.textY
        width: card.textWidth
        text: card.title
        color: Theme.text
        font.pixelSize: Theme.fontBody + 1
        font.weight: Font.DemiBold
        wrapMode: Text.Wrap
        horizontalAlignment: card.round && !card.listMode ? Text.AlignHCenter : Text.AlignLeft
    }
    Text {
        id: subtitleText
        x: card.textX
        y: titleText.y + titleText.height + 4
        width: card.textWidth
        text: card.subtitle
        color: Theme.textMuted
        font.pixelSize: Theme.fontSmall + 1
        wrapMode: Text.Wrap
        horizontalAlignment: card.round && !card.listMode ? Text.AlignHCenter : Text.AlignLeft
    }
}
