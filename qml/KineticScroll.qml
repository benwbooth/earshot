import QtQuick

// Momentum scrolling for a Flickable. Each mouse-wheel notch adds speed that
// then glides to a stop; touchpad swipes follow the fingers and keep coasting
// after they lift. Dragging and touch flicks keep the Flickable's own physics.
WheelHandler {
    id: kinetic
    required property Flickable view

    property real notchSpeed: 1400        // px/s added per wheel notch
    property real maximumSpeed: 9000      // px/s
    property real friction: 4.5           // higher stops sooner
    property real velocity: 0             // px/s, positive scrolls down
    property double lastPixelTime: 0

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
        view.contentY = next
        return !hitEdge
    }

    function stop() {
        velocity = 0
        glide.stop()
    }

    onWheel: event => {
        if (event.modifiers & Qt.ControlModifier)
            return
        view.cancelFlick()
        if (event.pixelDelta.y !== 0) {
            // Touchpad: follow the fingers, remember the speed for the coast.
            glide.stop()
            const now = Date.now()
            const dt = lastPixelTime > 0 ? Math.max(0.004, (now - lastPixelTime) / 1000) : 0.016
            lastPixelTime = now
            const delta = -event.pixelDelta.y
            moveBy(delta)
            const instant = delta / dt
            velocity = Math.max(-maximumSpeed, Math.min(maximumSpeed, velocity * 0.6 + instant * 0.4))
            coastTimer.restart()
        } else if (event.angleDelta.y !== 0) {
            // Mouse wheel: each notch adds speed; reversing cancels the old glide.
            const steps = event.angleDelta.y / 120
            const push = -steps * notchSpeed
            if (velocity * push < 0)
                velocity = 0
            velocity = Math.max(-maximumSpeed, Math.min(maximumSpeed, velocity + push))
            glide.start()
        }
        event.accepted = true
    }

    // Fingers lifted: keep going with the last speed.
    property Timer coastTimer: Timer {
        interval: 60
        onTriggered: {
            kinetic.lastPixelTime = 0
            if (Math.abs(kinetic.velocity) > 200)
                glide.start()
            else
                kinetic.velocity = 0
        }
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
