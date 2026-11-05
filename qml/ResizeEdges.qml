import QtQuick

// Invisible strips along the window's edges and corners that resize it,
// since the window has no system frame of its own.
Item {
    id: edges
    required property var window
    property int grip: 6

    anchors.fill: parent
    visible: window.visibility !== Window.Maximized && window.visibility !== Window.FullScreen
    z: 1000

    component Edge: MouseArea {
        property int edges: 0
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton
        onPressed: edges.window.startSystemResize(this.edges)
    }

    Edge { edges: Qt.TopEdge; cursorShape: Qt.SizeVerCursor; x: edges.grip; width: parent.width - 2 * edges.grip; height: edges.grip }
    Edge { edges: Qt.BottomEdge; cursorShape: Qt.SizeVerCursor; x: edges.grip; y: parent.height - edges.grip; width: parent.width - 2 * edges.grip; height: edges.grip }
    Edge { edges: Qt.LeftEdge; cursorShape: Qt.SizeHorCursor; y: edges.grip; width: edges.grip; height: parent.height - 2 * edges.grip }
    Edge { edges: Qt.RightEdge; cursorShape: Qt.SizeHorCursor; x: parent.width - edges.grip; y: edges.grip; width: edges.grip; height: parent.height - 2 * edges.grip }
    Edge { edges: Qt.TopEdge | Qt.LeftEdge; cursorShape: Qt.SizeFDiagCursor; width: edges.grip * 2; height: edges.grip * 2 }
    Edge { edges: Qt.TopEdge | Qt.RightEdge; cursorShape: Qt.SizeBDiagCursor; x: parent.width - edges.grip * 2; width: edges.grip * 2; height: edges.grip * 2 }
    Edge { edges: Qt.BottomEdge | Qt.LeftEdge; cursorShape: Qt.SizeBDiagCursor; y: parent.height - edges.grip * 2; width: edges.grip * 2; height: edges.grip * 2 }
    Edge { edges: Qt.BottomEdge | Qt.RightEdge; cursorShape: Qt.SizeFDiagCursor; x: parent.width - edges.grip * 2; y: parent.height - edges.grip * 2; width: edges.grip * 2; height: edges.grip * 2 }
}
