import QtQuick

// Strips along the window's edges and corners that resize it, since the
// window has no system frame of its own: the shared ChromeResizeEdges
// (hearth/shared/app-menus).
ChromeResizeEdges {
    anchors.fill: parent
    z: 1000
}
