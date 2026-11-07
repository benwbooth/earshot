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
- **Now-playing card**: a small card in the corner when a song starts. Choose always, only when Earshot isn't the
  window in front (the default) or never, from the settings menu (the gear in the top bar, or Ctrl+,).
- Long titles in the player bar and mini player glide sideways instead of making them taller.
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
- **Made for you**: Discover Weekly (new songs like the ones you play, every Monday), three Daily Mixes (your top
  artists plus similar songs, every day), Release Radar (new albums from your artists) and Albums for you (by related
  artists). Built from a listening history kept only on your computer; a song counts after 30 seconds.
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
- Your playlists, liked songs, saved albums and listening history live in an SQLite database,
  `~/.local/share/earshot/earshot.db`. The app and the web version's server can use it at the same time: each change
  is one transaction that takes SQLite's write lock first, readers never wait (write-ahead log), and each notices the
  other's changes within a couple of seconds. A `library.json` from earlier versions is imported once and kept as
  `library.json.imported`.

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

## Web version

The same Earshot in a browser, on computers and phones (on phones the sidebar becomes tabs along the bottom). Its
interface is Rust too, written with [Leptos](https://leptos.dev) and compiled to WebAssembly, with no JavaScript of
its own; a small server answers it with the same Rust code the app uses (`crates/core`).

- **From the app**: Settings (the gear, or Ctrl+,) › **Open the web version** starts it on this computer at
  http://127.0.0.1:8790/ and opens it in your browser. It shares the app's playlists, liked songs and listening
  history, and changes show up in both straight away.
- **On its own**: `nix run github:benwbooth/earshot#server`, then open http://127.0.0.1:8790/. Set
  `EARSHOT_LISTEN=0.0.0.0:8790` to reach it from phones on your network.
- **Docker**: `nix build github:benwbooth/earshot#docker && docker load < result` gives the image `earshot:latest`.
  It listens on port 8790 and keeps its library in `/data`:
  ```sh
  docker run -d -p 8788:8790 -v /srv/earshot:/data -v /run/user/1000/pipewire-0:/run/pipewire/pipewire-0 earshot:latest
  ```
  (the PipeWire socket is optional; it lets the waveform hear the speakers, below).

Songs play through each site's own player, as in the app. A video whose owner blocks embedded players still plays,
on any device: Earshot's server plays it on youtube.com's own page in a hidden, silent Chromium of its own and
streams the sound to the page as MP3 (`/api/relay`; the page says when a song plays this way). Ads never come
through: the server sends nothing while one shows and clicks Skip as soon as YouTube offers it. Starting, and
seeking to a part YouTube hasn't loaded yet, take a few seconds. On a server whose address YouTube distrusts
without a sign-in, its page stops about 45 seconds after each start or seek, and the song moves on as when it
won't play. Without Chromium on the server (the Nix package and the Docker image bring their own; elsewhere set
`EARSHOT_CHROMIUM` or put `chromium` on PATH) such videos move on to another upload instead. The waveform is
always measured, never made up:

- Bandcamp and Internet Archive songs are measured in the browser (their audio passes through Earshot's server,
  which those sites require for a page to measure it), and so are videos streamed from youtube.com's page.
- YouTube and SoundCloud play in those sites' own players, which a page can't hear, so the server fetches the
  song's audio itself, measures it, and the page plays those levels back in time with the song, on any device.
  YouTube sometimes sends only the first minute or so without a sign-in; after that the bars rest.
- Mixcloud scrambles its audio against downloading, so it isn't measured. Only then does the page fall back to
  the speakers of the computer running the server (the PipeWire socket above); otherwise the bars rest.
- When the bars rest, their tooltip says why.

## Build and run (Nix)

```sh
nix run github:benwbooth/earshot          # or, from a checkout:
nix build && ./result/bin/earshot
```

Development:

```sh
nix develop
cargo run
cargo test --workspace --exclude earshot-web
# the web version: build its interface, then serve it
scripts/build-web.sh target/web
EARSHOT_WEB_DIR=target/web cargo run -p earshot-server
```

The code is a Cargo workspace: the Qt app at the top, `crates/core` (the music services and library, shared by
both), `crates/server` (the web version's server) and `crates/web` (its Leptos interface).

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
