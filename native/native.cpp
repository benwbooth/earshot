#include "earshot/native.h"

#include <string>
#include <vector>

#include <QtCore/QUrl>
#include <QtGui/QIcon>
#include <QtQml/QQmlApplicationEngine>
#include <QtWebEngineQuick/qtwebenginequickglobal.h>
#include <QtWidgets/QApplication>
#ifdef Q_OS_LINUX
#include <QtDBus/QDBusConnection>
#include <QtDBus/QDBusMessage>
#endif

void earshotReloadKWinRules()
{
#ifdef Q_OS_LINUX
    QDBusConnection::sessionBus().send(QDBusMessage::createMethodCall(
        QStringLiteral("org.kde.KWin"), QStringLiteral("/KWin"), QStringLiteral("org.kde.KWin"), QStringLiteral("reconfigure")));
#endif
}

void earshotInitWebEngine()
{
    // Let YouTube's player start music without a click inside the page.
    qputenv("QTWEBENGINE_CHROMIUM_FLAGS",
            qgetenv("QTWEBENGINE_CHROMIUM_FLAGS") + " --autoplay-policy=no-user-gesture-required");
    QtWebEngineQuick::initialize();
}

int earshotRun(const rust::Vec<rust::String>& args, const QString& desktopId, const QString& icon, const QString& qml)
{
    // Qt keeps pointers to argc and argv for the application's whole life.
    static std::vector<std::string> storage;
    static std::vector<char*> argv;
    static int argc = 0;
    for (const auto& arg : args)
        storage.emplace_back(arg);
    for (auto& arg : storage)
        argv.push_back(arg.data());
    argv.push_back(nullptr);
    argc = int(storage.size());

    QApplication app(argc, argv.data());
    QApplication::setApplicationName(QStringLiteral("Earshot"));
    QApplication::setOrganizationName(QStringLiteral("Earshot"));
    QGuiApplication::setDesktopFileName(desktopId);
    QGuiApplication::setWindowIcon(QIcon(icon));

    // Declared after the application, so it's torn down first.
    QQmlApplicationEngine engine;
    engine.load(QUrl(qml));
    if (engine.rootObjects().isEmpty())
        return 1;
    return app.exec();
}
