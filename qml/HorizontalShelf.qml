import QtQuick
import QtQuick.Layouts

// A responsive grid: as many columns as fit. Put a Repeater inside and size
// each card to `cardWidth`; `limit` is how many cards fit in `maxRows` rows.
Grid {
    id: shelf
    property int minCardWidth: 168
    property int spacing: 24
    property int maxRows: -1

    readonly property int fitColumns: Math.max(1, Math.floor((width + spacing) / (minCardWidth + spacing)))
    readonly property real cardWidth: (width - (fitColumns - 1) * spacing) / fitColumns
    readonly property int limit: maxRows > 0 ? fitColumns * maxRows : 100000

    function take(list) {
        return (list || []).slice(0, limit)
    }

    Layout.fillWidth: true
    columns: fitColumns
    columnSpacing: spacing
    rowSpacing: spacing + 8
}
