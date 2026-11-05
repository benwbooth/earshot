import QtQuick
import org.kde.layershell as LayerShell

// Loaded only on Wayland when KDE's layer-shell module is installed: pins the
// now-playing card to the bottom-right corner, above other windows, without
// taking keyboard focus.
QtObject {
    required property var target
    readonly property var surface: target.LayerShell.Window

    Component.onCompleted: {
        surface.anchors = LayerShell.Window.AnchorBottom | LayerShell.Window.AnchorRight
        surface.layer = LayerShell.Window.LayerOverlay
        surface.exclusionZone = 0
        surface.keyboardInteractivity = LayerShell.Window.KeyboardInteractivityNone
        surface.activateOnShow = false
        surface.scope = "earshot-now-playing"
        surface.margins = Qt.binding(() => ({ left: 0, top: 0, right: target.margin, bottom: target.margin }))
    }
}
