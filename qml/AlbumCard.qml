import QtQuick
import QtQuick.Layouts
import Earshot

// An album (or artist, or playlist) as a card: artwork, title and a line below.
Item {
    id: card
    property string title
    property string subtitle
    property string image
    property string seed: title
    property bool round: false
    property bool playable: true
    property string placeholderIcon: "album"

    signal clicked()
    signal playClicked()

    implicitHeight: art.height + 12 + titleText.implicitHeight + 4 + subtitleText.implicitHeight + 8

    HoverHandler { id: hover; cursorShape: Qt.PointingHandCursor }
    TapHandler { onTapped: card.clicked() }

    Rectangle {
        anchors.fill: parent
        anchors.margins: -10
        radius: Theme.radius + 2
        color: hover.hovered ? Theme.surfaceHover : "transparent"
        Behavior on color { ColorAnimation { duration: 120 } }
    }

    Cover {
        id: art
        width: parent.width
        height: width
        source: card.image
        seed: card.seed
        radius: Theme.radius
        round: card.round
        placeholderIcon: card.placeholderIcon
    }

    IconButton {
        visible: card.playable
        filled: true
        iconName: "play"
        iconSize: 20
        anchors.right: art.right
        anchors.bottom: art.bottom
        anchors.margins: 10
        opacity: hover.hovered ? 1 : 0
        anchors.bottomMargin: hover.hovered ? 10 : 2
        Behavior on opacity { NumberAnimation { duration: 140 } }
        Behavior on anchors.bottomMargin { NumberAnimation { duration: 140 } }
        onClicked: card.playClicked()
    }

    Text {
        id: titleText
        anchors.top: art.bottom
        anchors.topMargin: 12
        width: parent.width
        text: card.title
        color: Theme.text
        font.pixelSize: Theme.fontBody + 1
        font.weight: Font.DemiBold
        elide: Text.ElideRight
        horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
    }
    Text {
        id: subtitleText
        anchors.top: titleText.bottom
        anchors.topMargin: 4
        width: parent.width
        text: card.subtitle
        color: Theme.textMuted
        font.pixelSize: Theme.fontSmall + 1
        elide: Text.ElideRight
        horizontalAlignment: card.round ? Text.AlignHCenter : Text.AlignLeft
    }
}
