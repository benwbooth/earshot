# Developing Earshot

Earshot is a Rust + Qt Quick app ([cxx-qt](https://github.com/KDAB/cxx-qt)) with a web version whose interface is
Rust too ([Leptos](https://leptos.dev), compiled to WebAssembly).

## Layout

A Cargo workspace:

- the Qt app at the top (`src/`, `qml/`, `native/`);
- `crates/core`: the music services and the library, shared by the app and the server;
- `crates/server`: the web version's server;
- `crates/web`: the web version's Leptos interface.

## Build and run (Nix)

```sh
nix run github:benwbooth/earshot          # or, from a checkout:
nix build && ./result/bin/earshot
```

Day to day:

```sh
./dev.sh          # rebuilds and restarts the app on every save (watchexec)
./dev.sh web      # the same for the web version, at http://127.0.0.1:8791/
./dev.sh all      # both
```

`dev.sh` keeps one copy running: starting it stops an earlier `dev.sh`, any running Earshot and any earlier dev web
server first, so opening Earshot again just raises the dev build.

By hand:

```sh
nix develop
cargo run
cargo test --workspace --exclude earshot-web
scripts/build-web.sh target/web                     # the web version's interface
EARSHOT_WEB_DIR=target/web cargo run -p earshot-server
```

Without Nix you need Rust and Qt 6.8+ (Base, Declarative, Svg, WebEngine), plus PipeWire and ALSA development files
on Linux; point `QMAKE` at Qt 6's `qmake` and run `cargo build --release`. On Wayland/KDE the now-playing card uses
`layer-shell-qt` when it's installed.

Environment variables:

- `EARSHOT_DATA_DIR`: where the library is kept (default `~/.local/share/earshot`). Setting it also lets this copy
  run next to another Earshot.
- `EARSHOT_WINDOW_BUTTONS`: `kde`, `gnome`, `windows` or `mac`, to force a window-button style.
- `EARSHOT_LISTEN`: the web version's address (default `127.0.0.1:8790`).
- `EARSHOT_WEB_DIR`: the web interface's files, for the server.
- `EARSHOT_CHROMIUM`: the Chromium the server uses for videos kept on youtube.com.

Testing aids: `--page=album:123`, `--search=text`, `--play-track=JSON`, `--test-notification` and
`--screenshot=file.png` (see the end of `qml/Main.qml`).

## How it works

- Catalogue data and artwork come from Deezer's public API (no account or key needed).
- Search, playlists and the chapter timestamps of long videos come from YouTube's own web API. Playlists and
  full-length videos open as albums, with long videos split into tracks at their chapters.
- Songs play in YouTube's embedded player, hosted out of sight in Qt WebEngine. Videos whose owners block embedding
  play on youtube.com's own watch page instead, hidden the same way. Nothing is downloaded. For catalogue songs
  Earshot picks the best-matching video and looks up the next songs ahead of time.
- Bandcamp songs stream from Bandcamp; SoundCloud and Mixcloud play in their own embedded players; Internet Archive
  MP3s stream from archive.org.
- The waveform is measured from what the speakers play, with [cpal](https://github.com/RustAudio/cpal): loopback on
  Windows and macOS 14.6+, the PipeWire speaker monitor on Linux.
- The library is an SQLite database, `~/.local/share/earshot/earshot.db`. The app and the web server can use it at
  once: each change is one transaction that takes the write lock first, readers never wait (write-ahead log), and
  each notices the other's changes within a couple of seconds.

### The web version

A small server answers the Leptos interface with the same code the app uses. Songs play through each site's own
player. A video whose owner blocks embedding plays on youtube.com's page in a hidden, silent Chromium on the server,
which streams the sound to the page as MP3 (`/api/relay`), skipping ads. Without Chromium on the server, such videos
move on to another upload.

The waveform is always measured, never made up: Bandcamp, Internet Archive and relayed songs are measured in the
browser; YouTube and SoundCloud songs are fetched and measured by the server and played back in time; Mixcloud
scrambles its audio, so it falls back to the server's speakers (a PipeWire socket mounted into the container) or
rests.

## Releases

`.github/workflows/release.yml` builds everything only when a GitHub release is published:

```sh
gh release create v0.1.0 --generate-notes
```

It adds the Windows installer and portable zip, the macOS disk image, the AppImage, the Flatpak bundle and repository
to the release, updates the Homebrew cask in `Casks/`, and pushes the server image to
`ghcr.io/benwbooth/earshot`. CI (`ci.yml`) and the project page (`pages.yml`) only run by hand or from a release.
`FLATPAK_GPG_PRIVATE_KEY` (optional secret) signs the Flatpak repository.
