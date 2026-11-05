import QtQuick
import QtQuick.Layouts
import Earshot

// A responsive grid: as many columns as fit. Put a Repeater inside and size
// each card to `cardWidth`; `limit` is how many cards fit in `maxRows` rows.
// Album shelves follow the list view setting: the same cards become rows,
// in one or more columns.
Grid {
    id: shelf
    property int minCardWidth: 168
    property int minRowWidth: 380
    property int spacing: 24
    property int maxRows: -1
    property bool albums: true          // false for genre, era and station tiles
    readonly property bool listMode: albums && Api.albumList

    // How many covers fit across; `limit` stays the same in list view.
    readonly property int gridColumns: Math.max(1, Math.floor((width + spacing) / (minCardWidth + spacing)))
    readonly property int fitColumns: listMode ? Math.max(1, Math.floor((width + spacing) / (minRowWidth + spacing))) : gridColumns
    readonly property real cardWidth: (width - (fitColumns - 1) * spacing) / fitColumns
    readonly property int limit: maxRows > 0 ? gridColumns * maxRows : 100000

    function take(list) {
        return (list || []).slice(0, limit)
    }

    Layout.fillWidth: true
    columns: fitColumns
    columnSpacing: spacing
    rowSpacing: listMode ? 8 : spacing + 8
}
