import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// The unified title bar: app name, back, search and the window buttons, in
// place of the desktop's own title bar. Drag it to move the window and
// double-click it to maximize.
Rectangle {
    id: bar
    required property var window
    property bool canGoBack: false
    property alias searchField: search
    signal backClicked()

    implicitHeight: 56
    color: Theme.sidebar

    readonly property bool maximized: window.visibility === Window.Maximized

    function toggleMaximized() {
        if (maximized)
            window.showNormal()
        else
            window.showMaximized()
    }

    // Moving and maximizing the window from empty parts of the bar
    DragHandler {
        target: null
        grabPermissions: PointerHandler.CanTakeOverFromAnything
        onActiveChanged: if (active) bar.window.startSystemMove()
    }
    TapHandler {
        acceptedButtons: Qt.LeftButton
        onDoubleTapped: bar.toggleMaximized()
    }

    Rectangle { anchors.bottom: parent.bottom; width: parent.width; height: 1; color: Theme.border }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 16
        anchors.rightMargin: 8
        spacing: 12

        // Brand, as wide as the sidebar below it so the search lines up with the content
        RowLayout {
            Layout.preferredWidth: 248 - 16
            spacing: 10
            Image {
                source: Theme.icon("earshot-app")
                sourceSize: Qt.size(56, 56)
                Layout.preferredWidth: 28
                Layout.preferredHeight: 28
            }
            Text {
                text: "Earshot"
                color: Theme.text
                font.pixelSize: 18
                font.weight: Font.Bold
                font.letterSpacing: -0.3
            }
        }

        IconButton {
            iconName: "back"
            iconSize: 18
            enabled: bar.canGoBack
            color: Theme.text
            tip: "Back (Alt+Left)"
            onClicked: bar.backClicked()
        }
        IconButton {
            iconName: "home"
            iconSize: 18
            active: Nav.current === "home"
            tip: "Home"
            onClicked: Nav.open("home")
        }

        TextField {
            id: search
            Layout.fillWidth: true
            Layout.maximumWidth: 480
            implicitHeight: 38
            leftPadding: 40
            rightPadding: 34
            placeholderText: "What do you want to hear?"
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.fontBody + 1
            selectByMouse: true
            onTextEdited: Nav.search(text)
            onAccepted: Nav.search(text)
            Keys.onEscapePressed: { text = ""; focus = false }
            background: Rectangle {
                radius: height / 2
                color: search.activeFocus ? Theme.surfaceRaised : Theme.surface
                border.color: search.activeFocus ? Theme.text : (searchHover.hovered ? Theme.textFaint : Theme.border)
                border.width: search.activeFocus ? 2 : 1
                HoverHandler { id: searchHover }
            }
            Icon {
                anchors.left: parent.left
                anchors.leftMargin: 14
                anchors.verticalCenter: parent.verticalCenter
                name: "search"
                size: 17
                color: Theme.textMuted
            }
            IconButton {
                visible: search.text !== ""
                anchors.right: parent.right
                anchors.rightMargin: 4
                anchors.verticalCenter: parent.verticalCenter
                iconName: "close"
                iconSize: 14
                onClicked: { search.text = ""; Nav.search("") }
            }
        }

        Item { Layout.fillWidth: true }

        // Window buttons
        Repeater {
            model: [
                { icon: "minimize", tip: "Minimize" },
                { icon: bar.maximized ? "restore" : "maximize", tip: bar.maximized ? "Restore" : "Maximize" },
                { icon: "close", tip: "Close" },
            ]
            delegate: AbstractButton {
                id: winButton
                required property var modelData
                required property int index
                implicitWidth: 44
                implicitHeight: 36
                hoverEnabled: true
                focusPolicy: Qt.NoFocus
                ToolTip.visible: hovered
                ToolTip.text: modelData.tip
                ToolTip.delay: 700
                background: Rectangle {
                    radius: 8
                    color: !winButton.hovered ? "transparent" : (winButton.index === 2 ? "#e5484d" : Qt.rgba(1, 1, 1, 0.08))
                }
                contentItem: Item {
                    Icon {
                        anchors.centerIn: parent
                        name: winButton.modelData.icon
                        size: 16
                        color: winButton.hovered ? "#ffffff" : Theme.textMuted
                    }
                }
                onClicked: {
                    if (index === 0)
                        bar.window.showMinimized()
                    else if (index === 1)
                        bar.toggleMaximized()
                    else
                        bar.window.close()
                }
            }
        }
    }
}
