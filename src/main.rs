mod backend;
mod catalog;
mod library;
mod native;
mod net;
mod radio;
mod youtube;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

fn main() {
    // A dark, flat look by default regardless of the desktop's Qt style.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        // SAFETY: set before any other thread exists.
        unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic") };
    }

    let mut app = QGuiApplication::new();
    QGuiApplication::set_desktop_file_name(&QString::from("earshot"));
    native::set_window_icon(&QString::from(":/qt/qml/Earshot/qml/icons/earshot-app.svg"));
    if let Some(mut app) = app.as_mut() {
        app.as_mut().set_application_name(&QString::from("Earshot"));
        app.as_mut().set_organization_name(&QString::from("Earshot"));
    }

    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/Earshot/qml/Main.qml"));
    }

    let code = app.as_mut().map(|app| app.exec()).unwrap_or(1);
    // Tear Qt down (audio output included) before the process exits;
    // exiting with them alive crashes in the audio backend's cleanup.
    drop(engine);
    drop(app);
    std::process::exit(code);
}
