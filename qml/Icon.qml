import QtQuick
import QtQuick.Effects
import Earshot

Item {
    id: root
    property string name
    property color color: Theme.text
    property int size: 20

    implicitWidth: size
    implicitHeight: size

    Image {
        id: image
        anchors.fill: parent
        source: root.name ? Theme.icon(root.name) : ""
        sourceSize: Qt.size(root.size * 2, root.size * 2)
        smooth: true
        visible: false
    }

    MultiEffect {
        anchors.fill: parent
        source: image
        colorization: 1.0
        colorizationColor: root.color
        brightness: 1.0
    }
}
