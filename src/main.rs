mod backend;
mod kde;
mod native;
mod visualizer;

use cxx_qt_lib::QString;

fn main() {
    // A dark, flat look by default regardless of the desktop's Qt style.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        // SAFETY: set before any other thread exists.
        unsafe { std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic") };
    }

    // YouTube's player runs in Qt WebEngine, which has to be set up first.
    native::init_web_engine();

    // Matches the .desktop file, so the desktop shows Earshot's icon and name.
    let desktop_id = std::env::var("FLATPAK_ID").unwrap_or_else(|_| "earshot".to_owned());
    let args: Vec<String> = std::env::args().collect();
    // Qt (audio output included) is torn down inside run(), before the process
    // exits; exiting with it alive crashes in the audio backend's cleanup.
    let code = native::run(
        &args,
        &QString::from(desktop_id.as_str()),
        &QString::from(":/qt/qml/Earshot/qml/icons/earshot-app.svg"),
        &QString::from("qrc:/qt/qml/Earshot/qml/Main.qml"),
    );
    std::process::exit(code);
}
