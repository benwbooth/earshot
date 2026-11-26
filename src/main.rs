mod backend;
mod kde;
mod native;


use cxx_qt_lib::QString;

fn main() {
    // A dark, flat look by default regardless of the desktop's Qt style.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        // SAFETY: set before any other thread exists.
        set_qt_env("QT_QUICK_CONTROLS_STYLE", "Basic");
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

/// Sets an environment variable Qt reads while starting, noting what it was
/// (as `__APP_ORIG_<name>`: empty = unset, else "=value") so
/// `restore_launch_env` puts it back once the window is loaded: programs this
/// app starts get the desktop's own. (QT_QUICK_CONTROLS_STYLE=Basic leaking
/// from our apps made KDE apps opened from them draw plain light controls,
/// 2026-11-25.)
fn set_qt_env(name: &str, value: impl AsRef<std::ffi::OsStr>) {
    let note = format!("__APP_ORIG_{name}");
    if std::env::var_os(&note).is_none() {
        let was = std::env::var_os(name)
            .map(|v| {
                let mut s = std::ffi::OsString::from("=");
                s.push(v);
                s
            })
            .unwrap_or_default();
        #[allow(unused_unsafe)]
        unsafe { std::env::set_var(&note, was) };
    }
    #[allow(unused_unsafe)]
    unsafe { std::env::set_var(name, value) };
}

/// Puts back what `set_qt_env` (or the C++ start-up) changed, once the main
/// QML has loaded: Qt has read them all by then.
#[allow(dead_code)]
fn restore_launch_env() {
    for (k, v) in std::env::vars_os() {
        let Some(name) = k.to_str().and_then(|k| k.strip_prefix("__APP_ORIG_")).map(str::to_owned) else { continue };
        let v = v.to_string_lossy().into_owned();
        #[allow(unused_unsafe)]
        unsafe {
            match v.strip_prefix('=') {
                Some(old) => std::env::set_var(&name, old),
                None => std::env::remove_var(&name),
            }
            std::env::remove_var(&k);
        }
    }
}
