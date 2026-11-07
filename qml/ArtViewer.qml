import QtQuick
import Earshot

// Artwork shown large over the window (clicking a cover opens it): the
// biggest version the service has, closed by a click anywhere or Escape.
Rectangle {
    id: viewer
    anchors.fill: parent
    color: Qt.rgba(0, 0, 0, 0.82)
    visible: opacity > 0
    opacity: 0
    z: 100
    Behavior on opacity { NumberAnimation { duration: 150 } }

    property var candidates: []
    property int tried: 0

    function show(url) {
        if (!url)
            return
        candidates = Api.bigArt(url)
        tried = 0
        art.source = candidates[0]
        opacity = 1
        forceActiveFocus()
    }
    function close() {
        opacity = 0
    }

    Keys.onEscapePressed: close()
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onClicked: viewer.close()
        onWheel: wheel => wheel.accepted = true
    }

    Image {
        id: art
        anchors.centerIn: parent
        width: Math.min(parent.width - 96, parent.height - 96)
        height: width
        fillMode: Image.PreserveAspectFit
        asynchronous: true
        cache: true
        // A size the service doesn't have: try the next one down.
        onStatusChanged: {
            if (status === Image.Error && viewer.tried + 1 < viewer.candidates.length) {
                viewer.tried += 1
                source = viewer.candidates[viewer.tried]
            }
        }
    }
    Spinner {
        anchors.centerIn: parent
        visible: art.status === Image.Loading
    }

    IconButton {
        anchors.top: parent.top
        anchors.right: parent.right
        anchors.margins: 16
        iconName: "close"
        iconSize: 22
        color: Theme.text
        tip: "Close (Esc)"
        onClicked: viewer.close()
    }
}
