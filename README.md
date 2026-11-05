# Earshot

A modern desktop music player written in Rust and Qt Quick ([cxx-qt](https://github.com/KDAB/cxx-qt)).
It's a fresh take on the ideas of the (no longer maintained) [Headset](https://github.com/headsetapp/headset-electron)
Electron app: popular music by genre and era, a radio made of songs people share on Reddit, and YouTube as the
music source. Earshot adds real **albums**, **artist pages** and **playlists**.

Project page: https://benwbooth.github.io/earshot/ (served by GitHub Pages from `docs/`).

![Home](docs/home.png)

| Album | Reddit radio |
|---|---|
| ![Album](docs/album.png) | ![Reddit radio](docs/radio.png) |

## Features

- **Home**: trending songs and albums, plus charts for 20+ genres and every decade from the 50s to the 2020s.
- **Search**: songs, albums and artists, with a top result, plus **YouTube**, **Bandcamp**, **SoundCloud**,
  **Mixcloud** and **Internet Archive** tabs for live sets, covers, independent releases, DJ mixes, concert
  recordings and anything not in the catalogue.
- **Browse each service** from the side pane: YouTube playlists, full albums and soundtracks by genre or mood,
  SoundCloud playlists and tracks, Mixcloud's popular shows, and the
  Internet Archive's Live Music Archive, 78 rpm records and netlabels, by genre or any tag.
- **Open on the web**: any song or album can be opened on YouTube, Bandcamp, SoundCloud, Mixcloud, the Internet
  Archive or Deezer in your browser.
- **Grid or list**: show albums as covers or as a compact list (the button in the top bar).
- Only one Earshot runs at a time: launching it again brings the open window forward (or out of the tray).
- Earshot reopens where you left it. On KDE Plasma (Wayland), where apps can't place their own windows, it adds a
  window rule once so KDE remembers the spot (System Settings › Window Rules › "Earshot: remember where its window
  was").
- **Bandcamp**: browse Bandcamp's genres and sub-genres, or any tag, sorted by best-selling, new arrivals or at
  random; every Bandcamp album lists its tags, and searches suggest matching tags.
- **Albums**: full track lists with artwork, release date, label and running time. Play, shuffle, add the whole
  album to the queue or a playlist, or save it to **Your albums**.
- **Artists**: popular songs and the complete discography (albums, singles and EPs).
- **Reddit radio**: about 50 music subreddits (r/listentothis, r/indieheads, r/jazz, r/synthwave…) in groups. Songs
  play in the order the community ranks them; save any station as a playlist.
- **Playlists**: create, rename, delete, reorder (Move up / Move down), filter, and add songs from any list with the
  `···` menu. **Liked songs** collects everything you heart. Import a YouTube, YouTube Music, SoundCloud or
  Mixcloud playlist, a Bandcamp album or an Internet Archive item by link.
- **Queue**: play next, add to queue, remove, clear upcoming; shuffle and repeat (all / one).
- **Random radio** (the dice next to repeat): when the queue runs out, Earshot keeps playing songs like the last
  ones, from the same service where it has suggestions (SoundCloud's related tracks, Mixcloud's similar shows,
  Bandcamp's recommended albums, the Internet Archive's other recordings by the artist) and YouTube's own mix
  otherwise.
- Keyboard: Space play/pause, Ctrl+←/→ previous/next, Ctrl+F search, Ctrl+N new playlist, Ctrl+S shuffle,
  Ctrl+R repeat, Alt+← back, and the media keys while the window has focus.

## How it works

- Catalogue data and artwork come from Deezer's public API (no account or key needed).
- Search, playlists and the chapter timestamps of long videos come straight from YouTube's own web API, in
  youtube.com's order. Playlists and full-length videos (soundtracks, full albums) open as albums, with long videos
  split into tracks at their chapters, or at the song list with times in their description.
- Songs play in YouTube's own embedded player, hosted out of sight in Qt WebEngine (as the original Headset did).
  Videos whose owners block embedded players play on youtube.com's own watch page instead, hidden the same way.
  Nothing is downloaded and no external programs are needed. For catalogue songs Earshot picks the best-matching
  video (official audio of the right length) and looks up the next songs ahead of time, so they start at once.
- Bandcamp albums, songs and tags come from bandcamp.com's own pages and search; songs stream from Bandcamp
  itself, as on its website. SoundCloud songs and playlists come from the API soundcloud.com's web app uses and play
  in SoundCloud's own embedded player, hidden like YouTube's. Previews of paid-only SoundCloud tracks are left out.
- Mixcloud shows come from Mixcloud's public API and play in Mixcloud's own embedded player. Internet Archive items
  open as albums and their MP3s stream straight from archive.org.
- The waveform is measured from what your speakers play, with the [cpal](https://github.com/RustAudio/cpal) audio
  library: loopback recording on Windows and macOS 14.6+, the PipeWire speaker monitor on Linux.
- Your playlists, liked songs and saved albums live in `~/.local/share/earshot/library.json`.

## Install

Downloads are on the [releases page](https://github.com/benwbooth/earshot/releases/latest).

- **Windows**: `Earshot-…-windows-x64-setup.exe` installs Earshot; `…-windows-x64-portable.zip` runs from any
  folder without installing.
- **macOS** (Apple silicon, macOS 13+): open `Earshot-…-macos-arm64.dmg` and drag Earshot to Applications, or use
  Homebrew:
  ```sh
  brew tap benwbooth/earshot https://github.com/benwbooth/earshot
  brew install --cask earshot
  ```
  Earshot isn't notarized by Apple; if macOS won't open the dmg's copy, run
  `xattr -dr com.apple.quarantine /Applications/Earshot.app` once (Homebrew does this for you).
- **Linux**:
  - Flatpak, from Earshot's own Flatpak repository (updates with `flatpak update`):
    ```sh
    flatpak install --user https://benwbooth.github.io/earshot/flatpak/earshot.flatpakref
    ```
    or download the single-file bundle `Earshot-…-x86_64.flatpak` and `flatpak install --user` it.
  - AppImage: download `Earshot-…-x86_64.AppImage`, make it executable and run it.
  - Nix: `nix run github:benwbooth/earshot`.

New versions are built by GitHub Actions (`.github/workflows/release.yml`) whenever a `v*` tag is pushed.

## Build and run (Nix)

```sh
nix run github:benwbooth/earshot          # or, from a checkout:
nix build && ./result/bin/earshot
```

Development:

```sh
nix develop
cargo run
cargo test
```

Without Nix you need Rust and Qt 6.8+ (Base, Declarative, Svg, WebEngine), plus PipeWire and ALSA development files
on Linux; point `QMAKE` at Qt 6's `qmake` and run `cargo build --release`. On Wayland/KDE the now-playing card uses
`layer-shell-qt` when it's installed.

Environment variables: `EARSHOT_DATA_DIR` (where the library is kept), `EARSHOT_WINDOW_BUTTONS`
(`kde`, `gnome`, `windows` or `mac`, to force a window-button style).

## Notes

- Reddit lets apps without an account load about one station a minute. Earshot keeps each station's list on disk,
  shows the saved list straight away, and loads the new one by itself when Reddit allows (a countdown shows how long).
- Earshot is an independent project and is not affiliated with Headset, Deezer, YouTube or Reddit.

## License

MIT
