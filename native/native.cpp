#include "earshot/native.h"

#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>
#include <QtWebEngineQuick/qtwebenginequickglobal.h>

void earshotSetWindowIcon(const QString& resource)
{
    QGuiApplication::setWindowIcon(QIcon(resource));
}

void earshotInitWebEngine()
{
    // Let YouTube's player start music without a click inside the page.
    qputenv("QTWEBENGINE_CHROMIUM_FLAGS",
            qgetenv("QTWEBENGINE_CHROMIUM_FLAGS") + " --autoplay-policy=no-user-gesture-required");
    QtWebEngineQuick::initialize();
}
