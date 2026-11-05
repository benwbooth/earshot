import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// Loading spinner, error with retry, or an empty-state message.
ColumnLayout {
    id: status
    property bool loading: false
    property string error: ""
    property string empty: ""
    property string emptyIcon: "music"
    signal retry()

    visible: loading || error !== "" || empty !== ""
    Layout.fillWidth: true
    Layout.topMargin: 40
    spacing: 14

    BusyIndicator {
        visible: status.loading
        running: visible
        Layout.alignment: Qt.AlignHCenter
        implicitWidth: 40
        implicitHeight: 40
        palette.dark: Theme.accent
    }
    Icon {
        visible: !status.loading && (status.error !== "" || status.empty !== "")
        Layout.alignment: Qt.AlignHCenter
        name: status.error !== "" ? "close" : status.emptyIcon
        size: 40
        color: Theme.textFaint
    }
    Text {
        visible: !status.loading && (status.error !== "" || status.empty !== "")
        Layout.alignment: Qt.AlignHCenter
        Layout.maximumWidth: 460
        horizontalAlignment: Text.AlignHCenter
        wrapMode: Text.WordWrap
        text: status.error !== "" ? status.error : status.empty
        color: Theme.textMuted
        font.pixelSize: Theme.fontBody + 1
    }
    PillButton {
        visible: !status.loading && status.error !== ""
        Layout.alignment: Qt.AlignHCenter
        text: "Try again"
        onClicked: status.retry()
    }
}
