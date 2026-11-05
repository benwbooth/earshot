import QtQuick
import QtQuick.Controls.Basic
import QtQuick.Layouts
import Earshot

// A small dialog asking for one line of text (playlist names, links).
Popup {
    id: prompt
    property string title
    property string message: ""
    property string placeholder: ""
    property string acceptText: "Save"
    property alias value: field.text
    signal accepted(string value)

    function ask(initial) {
        field.text = initial || ""
        open()
        field.forceActiveFocus()
        field.selectAll()
    }

    modal: true
    focus: true
    anchors.centerIn: Overlay.overlay
    width: 420
    padding: 24
    closePolicy: Popup.CloseOnEscape | Popup.CloseOnPressOutside

    Overlay.modal: Rectangle { color: Qt.rgba(0, 0, 0, 0.55) }

    background: Rectangle {
        color: Theme.surfaceRaised
        radius: Theme.radiusLarge
        border.color: Theme.border
    }

    enter: Transition {
        NumberAnimation { property: "opacity"; from: 0; to: 1; duration: 140 }
        NumberAnimation { property: "scale"; from: 0.96; to: 1; duration: 140; easing.type: Easing.OutCubic }
    }

    contentItem: ColumnLayout {
        spacing: 16
        Text {
            text: prompt.title
            color: Theme.text
            font.pixelSize: Theme.fontTitle - 2
            font.weight: Font.Bold
        }
        Text {
            visible: prompt.message !== ""
            Layout.fillWidth: true
            text: prompt.message
            wrapMode: Text.WordWrap
            color: Theme.textMuted
            font.pixelSize: Theme.fontBody
        }
        TextField {
            id: field
            Layout.fillWidth: true
            placeholderText: prompt.placeholder
            placeholderTextColor: Theme.textFaint
            color: Theme.text
            font.pixelSize: Theme.fontBody + 1
            selectByMouse: true
            leftPadding: 14
            implicitHeight: 42
            background: Rectangle {
                radius: 8
                color: Theme.background
                border.color: field.activeFocus ? Theme.accent : Theme.border
                border.width: field.activeFocus ? 2 : 1
            }
            onAccepted: save.clicked()
        }
        RowLayout {
            Layout.alignment: Qt.AlignRight
            spacing: 10
            PillButton { text: "Cancel"; onClicked: prompt.close() }
            PillButton {
                id: save
                text: prompt.acceptText
                primary: true
                implicitHeight: 36
                enabled: field.text.trim() !== ""
                onClicked: {
                    prompt.accepted(field.text.trim())
                    prompt.close()
                }
            }
        }
    }
}
