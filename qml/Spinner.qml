import QtQuick

// The loading spinner, as in the web version: a faint ring with a quarter of
// it in the accent colour, going round.
Item {
    id: spinner
    property bool running: visible
    property color color: Theme.accent
    property real thickness: 3

    implicitWidth: 40
    implicitHeight: 40

    Rectangle {
        anchors.fill: parent
        radius: width / 2
        color: "transparent"
        border.width: spinner.thickness
        border.color: Qt.alpha(spinner.color, 0.2)
    }

    Canvas {
        id: arc
        anchors.fill: parent
        onPaint: {
            const ctx = getContext("2d")
            ctx.reset()
            ctx.lineWidth = spinner.thickness
            ctx.strokeStyle = spinner.color
            ctx.beginPath()
            ctx.arc(width / 2, height / 2, (Math.min(width, height) - spinner.thickness) / 2, -0.75 * Math.PI, -0.25 * Math.PI)
            ctx.stroke()
        }
        onWidthChanged: requestPaint()
        onHeightChanged: requestPaint()

        RotationAnimator on rotation {
            from: 0
            to: 360
            duration: 800
            loops: Animation.Infinite
            running: spinner.running
        }
    }

    onColorChanged: arc.requestPaint()
    onThicknessChanged: arc.requestPaint()
}
