import QtQuick
import QtQuick.Layouts
import Earshot

// Bandcamp tags as chips; each opens the tag's albums.
Flow {
    id: chips
    property var tags: []      // [{ slug, label }]
    property string prefix: "#"

    Layout.fillWidth: true
    spacing: 8
    visible: tags.length > 0

    Repeater {
        model: chips.tags
        delegate: PillButton {
            required property var modelData
            text: chips.prefix + modelData.label
            onClicked: Nav.open("bandcamp-tag", { tag: modelData.slug, label: modelData.label })
        }
    }
}
