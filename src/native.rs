//! Small desktop-integration helpers that cxx-qt-lib doesn't cover.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("earshot/native.h");
        #[cxx_name = "earshotInitWebEngine"]
        fn init_web_engine();

        #[cxx_name = "earshotRun"]
        fn run(args: &Vec<String>, desktop_id: &QString, icon: &QString, qml: &QString) -> i32;

        #[cxx_name = "earshotReloadKWinRules"]
        fn reload_kwin_rules();
    }
}

pub use ffi::{init_web_engine, reload_kwin_rules, run};
