import QtQuick
import QtQuick.Controls.Basic

// Momentum scrolling for a Flickable. Each mouse-wheel notch adds speed that
// then glides to a stop, and spinning the wheel quickly builds up speed;
// touchpad swipes follow the fingers and keep coasting after they lift.
// Dragging and touch flicks keep the Flickable's own physics. The view's
// scroll bar shows while it moves and fades out shortly after.
WheelHandler {
    id: kinetic
    required property Flickable view

    property real notchSpeed: 1000        // px/s added per wheel notch
    property real maximumBoost: 3         // fast spins add up to this many times more
    property real maximumSpeed: 12000     // px/s
    property real friction: 3.2           // higher stops sooner
    property real velocity: 0             // px/s, positive scrolls down
    property real boost: 1
    property double lastNotchTime: 0
    property var swipe: []                // recent touchpad moves: [time ms, px]

    target: null
    orientation: Qt.Vertical
    acceptedDevices: PointerDevice.Mouse | PointerDevice.TouchPad

    function bounds() {
        const top = view.originY - view.topMargin
        return [top, Math.max(top, view.originY + view.contentHeight - view.height + view.bottomMargin)]
    }

    function moveBy(delta) {
        const [top, bottom] = bounds()
        const next = Math.max(top, Math.min(bottom, view.contentY + delta))
        const hitEdge = next !== view.contentY + delta
        if (next !== view.contentY)
            showBar()
        view.contentY = next
        return !hitEdge
    }

    // Flickable only lights up its scroll bar for its own drags and flicks.
    readonly property ScrollBar bar: view ? view.ScrollBar.vertical : null
    function showBar() {
        if (!bar)
            return
        bar.active = true
        barTimer.restart()
    }
    property Timer barTimer: Timer {
        interval: 600
        onTriggered: {
            const bar = kinetic.bar
            if (!bar)
                return
            if (kinetic.glide.running || bar.pressed || bar.hovered || kinetic.view.moving)
                restart()
            else
                bar.active = false
        }
    }

    function clamp(speed) {
        return Math.max(-maximumSpeed, Math.min(maximumSpeed, speed))
    }

    function stop() {
        velocity = 0
        glide.stop()
    }

    // Mouse wheel: each notch adds speed; quick notches add more; reversing
    // cancels the old glide.
    function wheelNotches(steps, now) {
        boost = now - lastNotchTime < 160 ? Math.min(maximumBoost, boost + 0.35) : 1
        lastNotchTime = now
        const push = -steps * notchSpeed * boost
        if (velocity * push < 0) {
            velocity = 0
            boost = 1
        }
        velocity = clamp(velocity + push)
        glide.start()
    }

    // Touchpad: follow the fingers and remember the moves of the last moment.
    function touchpadMove(pixels, now) {
        glide.stop()
        velocity = 0
        moveBy(pixels)
        swipe = swipe.filter(move => now - move[0] < 120).concat([[now, pixels]])
        coastTimer.restart()
    }

    // Fingers lifted: coast at the swipe's speed over its last ~120 ms, not
    // the slower speed of the very last move.
    function touchpadLifted() {
        const moves = swipe
        swipe = []
        if (moves.length < 2)
            return
        const span = Math.max(16, moves[moves.length - 1][0] - moves[0][0])
        const travelled = moves.slice(1).reduce((sum, move) => sum + move[1], 0)
        const speed = travelled / span * 1000
        if (Math.abs(speed) > 150) {
            velocity = clamp(speed)
            glide.start()
        }
    }

    onWheel: event => {
        if (event.modifiers & Qt.ControlModifier)
            return
        view.cancelFlick()
        if (event.pixelDelta.y !== 0)
            touchpadMove(-event.pixelDelta.y, Date.now())
        else if (event.angleDelta.y !== 0)
            wheelNotches(event.angleDelta.y / 120, Date.now())
        event.accepted = true
    }

    // No touchpad moves for a moment: the fingers have lifted.
    property Timer coastTimer: Timer {
        interval: 50
        onTriggered: kinetic.touchpadLifted()
    }

    property FrameAnimation glide: FrameAnimation {
        onTriggered: {
            const dt = Math.min(frameTime, 0.05)
            if (!kinetic.moveBy(kinetic.velocity * dt)) {
                kinetic.stop()
                return
            }
            kinetic.velocity *= Math.exp(-kinetic.friction * dt)
            if (Math.abs(kinetic.velocity) < 12)
                kinetic.stop()
        }
    }
}
