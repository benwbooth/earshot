import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import QtCore
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
    // "kde", "gnome", "windows" or "mac"; anything else gets the Qt/KDE look.
    readonly property string buttonStyle: {
        const desktop = Backend.desktop()
        return ["gnome", "windows", "mac"].indexOf(desktop) >= 0 ? desktop : "kde"
    }

    // Search history, newest first, kept between runs.
    Settings {
        id: history
        category: "search"
        property string recent: "[]"
    }
    readonly property var historyItems: {
        try { return JSON.parse(history.recent) } catch (error) { return [] }
    }
    readonly property var matchingHistory: {
        const typed = search.text.trim().toLowerCase()
        return historyItems.filter(item => typed === "" || (item.toLowerCase().indexOf(typed) >= 0 && item.toLowerCase() !== typed)).slice(0, 12)
    }
    function addHistory(query) {
        query = (query || "").trim()
        if (query.length < 2)
            return
        const items = historyItems.filter(item => item.toLowerCase() !== query.toLowerCase())
        // A longer search replaces the half-typed one it grew from.
        const kept = items.filter(item => !query.toLowerCase().startsWith(item.toLowerCase()))
        history.recent = JSON.stringify([query].concat(kept).slice(0, 30))
    }
    function removeHistory(query) {
        history.recent = JSON.stringify(historyItems.filter(item => item !== query))
    }
    function clearHistory() {
        history.recent = "[]"
    }
    function pickHistory(query) {
        if (!query)
            return
        search.text = query
        Nav.search(query)
        addHistory(query)
        search.focus = false
    }

    function openSettings() {
        settingsMenu.popup(settingsButton, settingsButton.width - settingsMenu.width, settingsButton.height)
    }

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

        WindowButtons {
            visible: bar.buttonStyle === "mac"
            window: bar.window
            style: "mac"
            Layout.rightMargin: 8
        }

        // Brand, as wide as the sidebar below it so the search lines up with the content
        RowLayout {
            Layout.preferredWidth: 248 - 16 - (bar.buttonStyle === "mac" ? 60 : 0)
            Layout.fillWidth: false
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
            // Keeps the name next to the logo; the row's spare width goes after it.
            Item { Layout.fillWidth: true }
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
            onTextEdited: {
                Nav.search(text)
                remember.restart()
            }
            onAccepted: {
                Nav.search(text)
                bar.addHistory(text)
            }
            Keys.onEscapePressed: { text = ""; focus = false }
            Keys.onDownPressed: if (historyList.count > 0) historyList.forceActiveFocus()

            // A search typed and left alone for a moment counts as a search worth keeping.
            Timer {
                id: remember
                interval: 2000
                onTriggered: bar.addHistory(search.text)
            }

            // Recent searches, each with an ×, under the box while it has focus.
            Popup {
                id: historyPopup
                y: search.height + 6
                width: search.width
                padding: 6
                focus: false
                closePolicy: Popup.NoAutoClose
                visible: (search.activeFocus || historyList.activeFocus) && bar.matchingHistory.length > 0
                background: Rectangle {
                    color: Theme.surfaceRaised
                    radius: 12
                    border.color: Theme.border
                }
                contentItem: Column {
                    spacing: 2
                    Item {
                        width: parent.width
                        height: 30
                        Text {
                            anchors.left: parent.left
                            anchors.leftMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            text: "RECENT SEARCHES"
                            color: Theme.textFaint
                            font.pixelSize: Theme.fontSmall - 1
                            font.weight: Font.DemiBold
                            font.letterSpacing: 1
                        }
                        Text {
                            anchors.right: parent.right
                            anchors.rightMargin: 10
                            anchors.verticalCenter: parent.verticalCenter
                            text: "Clear all"
                            color: clearMouse.containsMouse ? Theme.text : Theme.textMuted
                            font.pixelSize: Theme.fontSmall + 1
                            font.weight: Font.DemiBold
                            MouseArea {
                                id: clearMouse
                                anchors.fill: parent
                                anchors.margins: -6
                                hoverEnabled: true
                                cursorShape: Qt.PointingHandCursor
                                onClicked: bar.clearHistory()
                            }
                        }
                    }
                    ListView {
                        id: historyList
                        width: parent.width
                        height: Math.min(contentHeight, 360)
                        clip: true
                        model: bar.matchingHistory
                        keyNavigationEnabled: true
                        Keys.onReturnPressed: bar.pickHistory(bar.matchingHistory[currentIndex])
                        Keys.onUpPressed: currentIndex > 0 ? decrementCurrentIndex() : search.forceActiveFocus()
                        Keys.onEscapePressed: search.forceActiveFocus()
                        delegate: Rectangle {
                            id: entry
                            required property string modelData
                            required property int index
                            width: ListView.view.width
                            height: 36
                            radius: 8
                            color: entryHover.hovered || (historyList.activeFocus && historyList.currentIndex === index) ? Theme.surfaceHover : "transparent"
                            HoverHandler { id: entryHover; cursorShape: Qt.PointingHandCursor }
                            TapHandler { onTapped: bar.pickHistory(entry.modelData) }
                            Icon {
                                id: clock
                                anchors.left: parent.left
                                anchors.leftMargin: 10
                                anchors.verticalCenter: parent.verticalCenter
                                name: "history"
                                size: 15
                                color: Theme.textMuted
                            }
                            Text {
                                anchors.left: clock.right
                                anchors.leftMargin: 10
                                anchors.right: removeButton.left
                                anchors.verticalCenter: parent.verticalCenter
                                text: entry.modelData
                                color: Theme.text
                                font.pixelSize: Theme.fontBody
                                wrapMode: Text.Wrap
                            }
                            IconButton {
                                id: removeButton
                                anchors.right: parent.right
                                anchors.rightMargin: 2
                                anchors.verticalCenter: parent.verticalCenter
                                iconName: "close"
                                iconSize: 13
                                tip: "Remove from history"
                                onClicked: bar.removeHistory(entry.modelData)
                            }
                        }
                    }
                }
            }
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

        // Albums as a grid of covers or as a list.
        IconButton {
            iconName: Api.albumList ? "grid" : "list"
            iconSize: 18
            tip: Api.albumList ? "Show albums as a grid" : "Show albums as a list"
            onClicked: Api.albumList = !Api.albumList
        }

        // Settings: when the now-playing card appears.
        IconButton {
            id: settingsButton
            iconName: "settings"
            iconSize: 18
            tip: "Settings"
            onClicked: bar.openSettings()
            Layout.rightMargin: 8
        }
        Menu {
            id: settingsMenu
            implicitWidth: 300
            topPadding: 6
            bottomPadding: 6
            background: Rectangle { color: Theme.surfaceRaised; radius: Theme.radius; border.color: Theme.border }
            delegate: MenuItem {
                id: settingItem
                implicitHeight: 34
                contentItem: RowLayout {
                    spacing: 8
                    Icon {
                        name: "check"
                        size: 14
                        color: Theme.accent
                        opacity: settingItem.checked ? 1 : 0
                    }
                    Text {
                        Layout.fillWidth: true
                        text: settingItem.text
                        color: settingItem.enabled ? Theme.text : Theme.textMuted
                        font.pixelSize: Theme.fontBody
                    }
                }
                background: Rectangle { color: settingItem.highlighted && settingItem.enabled ? Theme.surfaceHover : "transparent"; radius: 6; anchors.fill: parent; anchors.leftMargin: 6; anchors.rightMargin: 6 }
            }
            MenuItem { text: "Now-playing card when a song starts:"; enabled: false }
            MenuItem { text: "Always"; checkable: true; checked: Api.nowPlayingCard === "always"; onTriggered: Api.nowPlayingCard = "always" }
            MenuItem { text: "Only when Earshot isn't in front"; checkable: true; checked: Api.nowPlayingCard === "background"; onTriggered: Api.nowPlayingCard = "background" }
            MenuItem { text: "Never"; checkable: true; checked: Api.nowPlayingCard === "never"; onTriggered: Api.nowPlayingCard = "never" }
            MenuSeparator { contentItem: Rectangle { implicitHeight: 1; color: Theme.border } }
            // The same Earshot in a browser on this computer, sharing this window's playlists.
            MenuItem {
                text: "Open the web version"
                onTriggered: Api.call(Backend.openWebUi(), function (result) {
                    Api.openWeb(result.url)
                    Nav.notify("The web version is open in your browser at " + result.url)
                }, function (message) { Nav.notify(message) })
            }
        }

        // Window buttons in the desktop's own style (macOS puts them on the left).
        WindowButtons {
            visible: bar.buttonStyle !== "mac"
            window: bar.window
            style: bar.buttonStyle
            barHeight: bar.height
            Layout.rightMargin: bar.buttonStyle === "windows" ? -8 : 0
        }
    }
}
