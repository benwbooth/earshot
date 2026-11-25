use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=native");

    let module = QmlModule::new("Earshot").qml_files([
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/Api.qml").singleton(true),
        QmlFile::from("qml/Player.qml").singleton(true),
        QmlFile::from("qml/Nav.qml").singleton(true),
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Sidebar.qml"),
        QmlFile::from("qml/TopBar.qml"),
        QmlFile::from("qml/ResizeEdges.qml"),
        QmlFile::from("qml/KineticScroll.qml"),
        QmlFile::from("qml/MiniPlayer.qml"),
        QmlFile::from("qml/Waveform.qml"),
        QmlFile::from("qml/YouTubeEngine.qml"),
        QmlFile::from("qml/WindowButtons.qml"),
        QmlFile::from("qml/NowPlayingPopup.qml"),
        QmlFile::from("qml/PlayerBar.qml"),
        QmlFile::from("qml/ArtViewer.qml"),
        QmlFile::from("qml/Icon.qml"),
        QmlFile::from("qml/IconButton.qml"),
        QmlFile::from("qml/Cover.qml"),
        QmlFile::from("qml/AlbumCard.qml"),
        QmlFile::from("qml/TileCard.qml"),
        QmlFile::from("qml/TrackRow.qml"),
        QmlFile::from("qml/TrackList.qml"),
        QmlFile::from("qml/TrackMenu.qml"),
        QmlFile::from("qml/PillButton.qml"),
        QmlFile::from("qml/SectionHeader.qml"),
        QmlFile::from("qml/HorizontalShelf.qml"),
        QmlFile::from("qml/StatusView.qml"),
        QmlFile::from("qml/Spinner.qml"),
        QmlFile::from("qml/BrowseSections.qml"),
        QmlFile::from("qml/TextPrompt.qml"),
        QmlFile::from("qml/SeekSlider.qml"),
        QmlFile::from("qml/HomePage.qml"),
        QmlFile::from("qml/SearchPage.qml"),
        QmlFile::from("qml/RadioPage.qml"),
        QmlFile::from("qml/ChartPage.qml"),
        QmlFile::from("qml/AlbumPage.qml"),
        QmlFile::from("qml/ArtistPage.qml"),
        QmlFile::from("qml/PlaylistPage.qml"),
        QmlFile::from("qml/HistoryPage.qml"),
        QmlFile::from("qml/LibraryAlbumsPage.qml"),
        QmlFile::from("qml/QueuePage.qml"),
        QmlFile::from("qml/TagChips.qml"),
        QmlFile::from("qml/BandcampPage.qml"),
        QmlFile::from("qml/BandcampTagPage.qml"),
        QmlFile::from("qml/ServicePage.qml"),
        QmlFile::from("qml/Marquee.qml"),
        QmlFile::from("qml/ForYouPage.qml"),
        QmlFile::from("qml/AppMenu.qml"),
        QmlFile::from("qml/MacComboBox.qml"),
        QmlFile::from("qml/MacComboItem.qml"),
        QmlFile::from("qml/MacComboPopup.qml"),
        QmlFile::from("qml/MacMenuBackground.qml"),
        QmlFile::from("qml/MacMenuIcon.qml"),
        QmlFile::from("qml/MacMenuItem.qml"),
        QmlFile::from("qml/MacMenu.qml"),
        QmlFile::from("qml/MacMenuSeparator.qml"),
        QmlFile::from("qml/MacMenuStyle.qml"),
    ]);

    let icons: Vec<String> = std::fs::read_dir("qml/icons")
        .expect("qml/icons")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .map(|path| path.to_string_lossy().into_owned())
        .chain([
            // Loaded on demand: needs KDE's layer-shell module, which only exists on Linux/Wayland.
            "qml/NotificationLayerShell.qml".to_owned(),
        ])
        .collect();

    let mut builder = CxxQtBuilder::new_qml_module(module)
        .files(["src/backend.rs", "src/native.rs"])
        .crate_include_root(Some("native".to_owned()))
        .cpp_file("native/native.cpp")
        // The menus in the desktop's global menu bar (needs Qt's private QPA headers).
        .cpp_file("native/global_menu.h")
        .cpp_file("native/global_menu.cpp")
        .qrc_resources(icons)
        .qt_module("Network")
        .qt_module("Quick")
        .qt_module("Widgets")
        .qt_module("WebEngineQuick");
    // D-Bus: telling KDE's window manager to reload Earshot's window rule.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("linux") {
        builder = builder.qt_module("DBus");
    }
    let private_headers = qt_private_headers();
    // SAFETY: only adds include folders and a define for Earshot's own C++.
    let builder = unsafe {
        builder.cc_builder(|cc| {
            cc.includes(&private_headers);
            // Shown in About Earshot.
            cc.define("EARSHOT_VERSION", Some(format!("{:?}", env!("CARGO_PKG_VERSION")).as_str()));
        })
    };
    builder.build();

    // The icon Windows shows for earshot.exe in Explorer and the taskbar.
    #[cfg(windows)]
    winresource::WindowsResource::new()
        .set_icon("packaging/earshot.ico")
        .compile()
        .expect("Windows icon resource");
}

// Qt's private QtGui/QtCore headers (qpa/qplatformmenu.h and the like), which
// native/global_menu.cpp builds the desktop's global menu bar with: in the
// include folder, or inside the frameworks on macOS.
fn qt_private_headers() -> Vec<std::path::PathBuf> {
    let qmakes = match std::env::var("QMAKE") {
        Ok(qmake) => vec![qmake],
        Err(_) => vec!["qmake6".to_owned(), "qmake".to_owned()],
    };
    for qmake in qmakes {
        let query = |key: &str| {
            std::process::Command::new(&qmake)
                .args(["-query", key])
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
        };
        let Some(version) = query("QT_VERSION") else { continue };
        let mut roots = Vec::new();
        if let Some(headers) = query("QT_INSTALL_HEADERS") {
            let headers = std::path::PathBuf::from(headers);
            roots.push((headers.join("QtGui"), headers.join("QtCore")));
        }
        if let Some(libs) = query("QT_INSTALL_LIBS") {
            let libs = std::path::PathBuf::from(libs);
            roots.push((libs.join("QtGui.framework/Headers"), libs.join("QtCore.framework/Headers")));
        }
        for (gui, core) in roots {
            let gui = gui.join(&version);
            let core = core.join(&version);
            if gui.join("QtGui/qpa/qplatformmenu.h").is_file() {
                return vec![gui.join("QtGui"), gui, core.join("QtCore"), core];
            }
        }
    }
    println!("cargo:warning=Qt's private headers (qpa/qplatformmenu.h) weren't found; the global menu bar won't build");
    Vec::new()
}
