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
- **Search**: songs, albums and artists, with a top result; a **YouTube** tab finds live sets, covers and anything
  not in the catalogue.
- **Albums**: full track lists with artwork, release date, label and running time. Play, shuffle, add the whole
  album to the queue or a playlist, or save it to **Your albums**.
- **Artists**: popular songs and the complete discography (albums, singles and EPs).
- **Reddit radio**: about 50 music subreddits (r/listentothis, r/indieheads, r/jazz, r/synthwave…) in groups. Songs
  play in the order the community ranks them; save any station as a playlist.
- **Playlists**: create, rename, delete, reorder (Move up / Move down), filter, and add songs from any list with the
  `···` menu. **Liked songs** collects everything you heart. Import a YouTube or YouTube Music playlist by link.
- **Queue**: play next, add to queue, remove, clear upcoming; shuffle and repeat (all / one).
- Keyboard: Space play/pause, Ctrl+←/→ previous/next, Ctrl+F search, Ctrl+N new playlist, Ctrl+S shuffle,
  Ctrl+R repeat, Alt+← back, and the media keys while the window has focus.

## How it works

- Catalogue data and artwork come from Deezer's public API (no account or key needed).
- When a song plays, Earshot finds the best-matching YouTube video (preferring official audio of the right length)
  and downloads its audio with [yt-dlp](https://github.com/yt-dlp/yt-dlp) into a cache (`~/.cache/earshot/audio`,
  capped at 3 GB). The next song in the queue downloads in the background so playback continues without a gap.
- Your playlists, liked songs and saved albums live in `~/.local/share/earshot/library.json`.

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

Without Nix you need Rust, Qt 6.8+ (Base, Declarative, Multimedia, Svg) and `yt-dlp` on your `PATH`; point `QMAKE`
at Qt 6's `qmake` and run `cargo build --release`.

Environment variables: `EARSHOT_YTDLP` (path to yt-dlp), `EARSHOT_DATA_DIR`, `EARSHOT_CACHE_DIR`.

## Notes

- Reddit sometimes rate-limits requests from apps; if a station won't load, wait a minute and press Refresh.
- Earshot is an independent project and is not affiliated with Headset, Deezer, YouTube or Reddit.

## License

MIT
