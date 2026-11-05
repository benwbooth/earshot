import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Browse Bandcamp by genre and tag, or look up any tag.
Flickable {
    id: page
    readonly property string pageName: "bandcamp"
    property var args: ({})
    property var tags: Api.bandcampTags
    property bool loading: false
    property string error: ""

    readonly property var tints: ["#ff5f7e", "#7a5cff", "#3db8ff", "#3ddc97", "#ff8a3d", "#f5c542", "#e05cff", "#5c8aff"]

    function load() {
        if (tags)
            return
        loading = true
        error = ""
        Api.call(Backend.bandcampTags(), function (result) {
            Api.bandcampTags = result
            page.tags = result
            page.loading = false
        }, function (message) {
            page.loading = false
            page.error = message
        })
    }
    function subgenres(parent) {
        return tags ? tags.subgenres.filter(s => s.parent === parent) : []
    }
    function openTag(text) {
        const words = text.trim()
        if (words !== "")
            Nav.open("bandcamp-tag", { tag: words, label: words.toLowerCase() })
    }

    Component.onCompleted: load()

    contentWidth: width
    contentHeight: content.implicitHeight + 64
    clip: true
    boundsBehavior: Flickable.StopAtBounds
    ScrollBar.vertical: ScrollBar {}
    KineticScroll { view: page }

    Rectangle {
        width: parent.width
        height: 340
        gradient: Gradient {
            GradientStop { position: 0; color: Qt.alpha("#1da0c3", 0.4) }
            GradientStop { position: 1; color: "transparent" }
        }
    }

    ColumnLayout {
        id: content
        x: Theme.pagePadding
        y: 56
        width: page.width - Theme.pagePadding * 2
        spacing: 20

        Text {
            text: "BANDCAMP"
            color: Theme.textMuted
            font.pixelSize: Theme.fontSmall
            font.weight: Font.DemiBold
            font.letterSpacing: 1.5
        }
        Text {
            Layout.fillWidth: true
            text: "Browse by tag"
            color: Theme.text
            font.pixelSize: 48
            font.weight: Font.Black
            font.letterSpacing: -1
            wrapMode: Text.Wrap
        }
        Text {
            Layout.fillWidth: true
            text: "Best-selling, new and random albums from Bandcamp's artists and labels for any genre or tag. Songs stream from Bandcamp itself."
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody + 1
            wrapMode: Text.Wrap
        }

        // Any tag at all, not only the genres below.
        TextField {
            id: tagField
            Layout.fillWidth: true
            Layout.maximumWidth: 520
            implicitHeight: 44
            leftPadding: 44
            placeholderText: "Any tag: chiptune, vaporwave, koichi sugiyama…"
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.fontBody + 1
            selectByMouse: true
            onAccepted: page.openTag(text)
            background: Rectangle {
                radius: height / 2
                color: tagField.activeFocus ? Theme.surfaceRaised : Theme.surface
                border.color: tagField.activeFocus ? Theme.text : Theme.border
            }
            Icon {
                anchors.left: parent.left
                anchors.leftMargin: 16
                anchors.verticalCenter: parent.verticalCenter
                name: "tag"
                size: 18
                color: Theme.textMuted
            }
        }

        StatusView {
            loading: page.loading
            error: page.error
            onRetry: page.load()
        }

        SectionHeader { visible: !!page.tags; title: "Genres" }
        GridLayout {
            visible: !!page.tags
            Layout.fillWidth: true
            columns: Math.max(1, Math.floor(width / 240))
            columnSpacing: 16
            rowSpacing: 16
            Repeater {
                model: page.tags ? page.tags.genres : []
                delegate: TileCard {
                    required property var modelData
                    required property int index
                    Layout.fillWidth: true
                    title: modelData.label
                    subtitle: page.subgenres(modelData.slug).length > 0 ? page.subgenres(modelData.slug).length + " styles" : ""
                    tint: page.tints[index % page.tints.length]
                    onClicked: Nav.open("bandcamp-tag", { tag: modelData.slug, label: modelData.label })
                }
            }
        }

        Repeater {
            model: page.tags ? page.tags.genres : []
            delegate: ColumnLayout {
                required property var modelData
                Layout.fillWidth: true
                visible: page.subgenres(modelData.slug).length > 0
                spacing: 10
                SectionHeader { title: modelData.label }
                TagChips {
                    prefix: ""
                    tags: page.subgenres(modelData.slug)
                }
            }
        }
    }
}
