//! Small desktop-integration helpers that cxx-qt-lib doesn't cover.

#[cxx_qt::bridge]
pub mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!("earshot/native.h");
        #[cxx_name = "earshotSetWindowIcon"]
        fn set_window_icon(resource: &QString);

        #[cxx_name = "earshotInitWebEngine"]
        fn init_web_engine();
    }
}

pub use ffi::{init_web_engine, set_window_icon};
