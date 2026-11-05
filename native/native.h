#pragma once

#include <QtCore/QString>

#include "rust/cxx.h"

// Prepares Qt WebEngine, which hosts YouTube's player. Must run before the
// application object is created.
void earshotInitWebEngine();

// Creates the application, loads the QML window and runs until Earshot quits;
// returns the exit code. A full QApplication (not just QGuiApplication), so the
// tray icon and its menu can fall back to Qt Widgets where the desktop has no
// native ones (Windows, or GNOME without a tray extension).
int earshotRun(const rust::Vec<rust::String>& args, const QString& desktopId, const QString& icon, const QString& qml);
