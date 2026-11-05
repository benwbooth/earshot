#pragma once

#include <QtCore/QString>

// Sets the icon the desktop shows for Earshot's window (taskbar, alt-tab).
void earshotSetWindowIcon(const QString& resource);

// Prepares Qt WebEngine, which hosts YouTube's player. Must run before the
// application object is created.
void earshotInitWebEngine();
