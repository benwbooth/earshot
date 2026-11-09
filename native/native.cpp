#include "earshot/native.h"

#include <string>
#include <vector>

#include <QtCore/QDir>
#include <QtCore/QStandardPaths>
#include <QtCore/QUrl>
#include <QtGui/QFont>
#include <QtGui/QIcon>
#include <QtNetwork/QLocalServer>
#include <QtNetwork/QLocalSocket>
#include <QtQml/QQmlApplicationEngine>
#include <QtQuick/QQuickWindow>
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

// Where the running Earshot listens for "show yourself" from a second launch.
static QString instanceAddress()
{
#ifdef Q_OS_WIN
    return QStringLiteral("earshot-") + qEnvironmentVariable("USERNAME");
#else
    QString dir = QStandardPaths::writableLocation(QStandardPaths::RuntimeLocation);
    if (dir.isEmpty())
        return QDir::tempPath() + QStringLiteral("/earshot.socket");
    // The Flatpak's own runtime folder, which every Earshot uses: Flatpak
    // shares it with the rest of the desktop (at the same path), so a Flatpak
    // Earshot and an AppImage, Nix or development one find each other.
    dir += QStringLiteral("/app/io.github.benwbooth.Earshot");
    QDir().mkpath(dir);
    return dir + QStringLiteral("/earshot.socket");
#endif
}

// Whether this Earshot uses the usual library, and so is the only one: a run
// with EARSHOT_DATA_DIR pointing at another folder (tests, a second library)
// may run alongside.
static bool usualLibrary()
{
    const QString chosen = qEnvironmentVariable("EARSHOT_DATA_DIR");
    if (chosen.isEmpty())
        return true;
    const QString usual = QStandardPaths::writableLocation(QStandardPaths::GenericDataLocation) + QStringLiteral("/earshot");
    return QDir(chosen).canonicalPath() == QDir(usual).canonicalPath()
        || QDir::cleanPath(QDir(chosen).absolutePath()) == QDir::cleanPath(usual);
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
    // Draw text with the platform's own rasterizer (FreeType hinting and
    // subpixel settings from the desktop), not Qt Quick's distance fields,
    // which look soft at small sizes. Every window and Text item picks it up.
    QQuickWindow::setTextRenderType(QQuickWindow::NativeTextRendering);
    // Full hinting snaps every glyph to the pixel grid. With the desktop's
    // usual slight hinting, at fractional scales (130%) round letters like
    // e and o land half a pixel below the others, depending on where a line
    // of text falls. Plain windows inherit this from the application font;
    // Main.qml sets it on the main window, whose font comes from the style.
    QFont font = QGuiApplication::font();
    font.setHintingPreference(QFont::PreferFullHinting);
    QGuiApplication::setFont(font);

    // Only one Earshot at a time: a second launch asks the running one to show
    // its window, then quits.
    const bool single = usualLibrary();
    const QString address = instanceAddress();
    if (single) {
        QLocalSocket running;
        running.connectToServer(address);
        if (running.waitForConnected(500)) {
            // Pass on the desktop's permission to bring a window forward (Wayland).
            running.write("show " + qgetenv("XDG_ACTIVATION_TOKEN") + "\n");
            running.waitForBytesWritten(1000);
            running.disconnectFromServer();
            return 0;
        }
    }
    QLocalServer server;
    if (single) {
        QLocalServer::removeServer(address); // left over from a crash
        server.setSocketOptions(QLocalServer::UserAccessOption);
        server.listen(address);
    }

    // Declared after the application, so it's torn down first.
    QQmlApplicationEngine engine;
    engine.load(QUrl(qml));
    if (engine.rootObjects().isEmpty())
        return 1;

    QObject* window = engine.rootObjects().first();
    QObject::connect(&server, &QLocalServer::newConnection, &server, [&server, window]() {
        while (QLocalSocket* other = server.nextPendingConnection()) {
            QObject::connect(other, &QLocalSocket::disconnected, other, &QObject::deleteLater);
            QObject::connect(other, &QLocalSocket::readyRead, other, [other, window]() {
                const QByteArray message = other->readAll();
                const QByteArray token = message.startsWith("show ") ? message.mid(5).trimmed() : QByteArray();
                if (!token.isEmpty())
                    qputenv("XDG_ACTIVATION_TOKEN", token);
                QMetaObject::invokeMethod(window, "showWindow");
            });
        }
    });
    return app.exec();
}
