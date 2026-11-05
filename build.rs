use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    println!("cargo:rerun-if-changed=qml");
    println!("cargo:rerun-if-changed=build.rs");

    let module = QmlModule::new("Earshot").qml_files([
        QmlFile::from("qml/Theme.qml").singleton(true),
        QmlFile::from("qml/Api.qml").singleton(true),
        QmlFile::from("qml/Player.qml").singleton(true),
        QmlFile::from("qml/Nav.qml").singleton(true),
        QmlFile::from("qml/Main.qml"),
        QmlFile::from("qml/Sidebar.qml"),
        QmlFile::from("qml/PlayerBar.qml"),
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
        QmlFile::from("qml/TextPrompt.qml"),
        QmlFile::from("qml/SeekSlider.qml"),
        QmlFile::from("qml/HomePage.qml"),
        QmlFile::from("qml/SearchPage.qml"),
        QmlFile::from("qml/RadioPage.qml"),
        QmlFile::from("qml/ChartPage.qml"),
        QmlFile::from("qml/AlbumPage.qml"),
        QmlFile::from("qml/ArtistPage.qml"),
        QmlFile::from("qml/PlaylistPage.qml"),
        QmlFile::from("qml/LibraryAlbumsPage.qml"),
        QmlFile::from("qml/QueuePage.qml"),
    ]);

    let icons: Vec<String> = std::fs::read_dir("qml/icons")
        .expect("qml/icons")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .map(|path| path.to_string_lossy().into_owned())
        .collect();

    CxxQtBuilder::new_qml_module(module)
        .files(["src/backend.rs"])
        .qrc_resources(icons)
        .qt_module("Network")
        .qt_module("Quick")
        .build();
}
