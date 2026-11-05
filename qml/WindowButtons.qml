import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Controls.impl
import Earshot

// Minimize, maximize and close, drawn the way the desktop draws them:
// "kde" (Breeze/Qt), "gnome" (Adwaita), "windows" (Windows 11) or "mac" (traffic lights).
Row {
    id: buttons
    required property var window
    property string style: "kde"
    property real barHeight: 56
    readonly property bool maximized: window.visibility === Window.Maximized

    function toggleMaximized() {
        if (maximized)
            window.showNormal()
        else
            window.showMaximized()
    }

    function act(which) {
        if (which === "minimize")
            window.showMinimized()
        else if (which === "maximize")
            toggleMaximized()
        else
            window.close()
    }

    spacing: style === "mac" ? 8 : style === "gnome" ? 12 : style === "windows" ? 0 : 6
    layoutDirection: Qt.LeftToRight

    readonly property var order: style === "mac" ? ["close", "minimize", "maximize"] : ["minimize", "maximize", "close"]

    HoverHandler { id: groupHover }

    Repeater {
        model: buttons.order
        delegate: AbstractButton {
            id: button
            required property string modelData
            readonly property string which: modelData
            readonly property bool isClose: which === "close"
            readonly property string themeIcon: which === "close" ? "window-close"
                                              : which === "minimize" ? "window-minimize"
                                              : buttons.maximized ? "window-restore" : "window-maximize"
            readonly property string ownIcon: which === "close" ? "close" : which === "minimize" ? "minimize"
                                            : buttons.maximized ? "restore" : "maximize"

            anchors.verticalCenter: parent.verticalCenter
            implicitWidth: buttons.style === "windows" ? 46 : buttons.style === "mac" ? 12 : buttons.style === "gnome" ? 24 : 28
            implicitHeight: buttons.style === "windows" ? buttons.barHeight : implicitWidth
            hoverEnabled: true
            focusPolicy: Qt.NoFocus
            onClicked: buttons.act(which)

            ToolTip.visible: hovered && buttons.style !== "mac"
            ToolTip.text: which === "close" ? "Close" : which === "minimize" ? "Minimize" : buttons.maximized ? "Restore" : "Maximize"
            ToolTip.delay: 700

            background: Rectangle {
                radius: buttons.style === "windows" ? 0 : width / 2
                color: {
                    if (buttons.style === "mac")
                        return button.isClose ? "#ff5f57" : button.which === "minimize" ? "#febc2e" : "#28c840"
                    if (buttons.style === "gnome")
                        return button.pressed ? Qt.rgba(1, 1, 1, 0.22) : button.hovered ? Qt.rgba(1, 1, 1, 0.16) : Qt.rgba(1, 1, 1, 0.08)
                    if (buttons.style === "windows")
                        return button.isClose && button.hovered ? "#c42b1c" : button.hovered ? Qt.rgba(1, 1, 1, 0.08) : "transparent"
                    // Breeze: a soft circle, red for close
                    return button.isClose && button.hovered ? "#da4453" : button.hovered ? Qt.rgba(1, 1, 1, 0.12) : "transparent"
                }
                border.width: buttons.style === "mac" ? 0.5 : 0
                border.color: Qt.rgba(0, 0, 0, 0.25)
            }

            contentItem: Item {
                // The desktop's own icon theme where it has these icons, ours otherwise.
                IconImage {
                    visible: buttons.style === "kde" || buttons.style === "gnome"
                    anchors.centerIn: parent
                    name: buttons.style === "gnome" ? button.themeIcon + "-symbolic" : button.themeIcon
                    source: Theme.icon(button.ownIcon)
                    sourceSize: Qt.size(16, 16)
                    color: button.isClose && button.hovered && buttons.style === "kde" ? "#ffffff" : Theme.text
                }
                Icon {
                    visible: buttons.style === "windows"
                    anchors.centerIn: parent
                    name: button.ownIcon
                    size: 14
                    color: button.isClose && button.hovered ? "#ffffff" : Theme.text
                }
                // Traffic-light glyphs show when the pointer is over the group.
                Text {
                    visible: buttons.style === "mac" && groupHover.hovered
                    anchors.centerIn: parent
                    text: button.isClose ? "×" : button.which === "minimize" ? "−" : "+"
                    color: Qt.rgba(0, 0, 0, 0.6)
                    font.pixelSize: 10
                    font.bold: true
                }
            }
        }
    }
}
