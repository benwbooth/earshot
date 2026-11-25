import QtQuick

// The window's minimize, maximize/restore and close buttons: the shared
// ChromeButtons (hearth/shared/app-menus), drawn as arbor draws Breeze's on
// its title bars, the same in all of Ben's apps.
ChromeButtons {
    // (Kept for the callers: one look everywhere now.)
    property string style: "kde"
    property real barHeight: 56
    fg: Theme.text
    bg: Theme.sidebar
}
