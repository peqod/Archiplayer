# Archiplayer

Local-first desktop browser and player for the [WFMU](https://wfmu.org) archives.
Tauri v2 (Rust) + Svelte 5. No login, no cloud — everything lives in a local SQLite file.

> WFMU is a listener-supported, non-commercial freeform station.
> If this app gets use out of their archives, [donate to WFMU](https://pledge.wfmu.org/donate).

## Features

- **Browse** all ~580 shows (current + defunct) as an alphabetical tile grid with A–Z jump bar
- **Search** shows and DJs instantly; songs are searchable across every playlist you've viewed (lazy cache, FTS5)
- **Play** a whole show chronologically (tile hover → *Play all*), a single episode, or a song — clicking a song starts the episode at its timestamp
- **Player** with scrub (byte-range seeking), volume, queue, live now-playing song highlight
- **Favourites** for shows, episodes and songs — local profile, one click to play back
- **Offline**: download any episode (~180 MB for a 3-hour show); catalog and playlists browse from cache without network
- **Stats**: listening time per show, session counts, audition ranking
- **CSV export** of favourites, listening history and stats

## Data source

There is no official WFMU API. The app politely scrapes the public KenzoDB-generated pages
(1 request/second, cache-first, on-demand only — no crawling):

| What | Where |
|---|---|
| Show catalog | `wfmu.org/playlists/` |
| Episode list per show | `wfmu.org/playlists/{ID}` |
| Playlist per episode | `wfmu.org/playlists/shows/{episodeId}` |
| Audio URL | `wfmu.org/listen.m3u?show={ep}&archive={arch}` → direct MP3 on `mp3archives.wfmu.org` |

Parsers are tested against saved HTML fixtures in `src-tauri/tests/fixtures/`.
If WFMU changes their markup, update the fixtures and fix `src-tauri/src/wfmu.rs`.

## Development

Prereqs: Rust (MSVC toolchain + Windows SDK), Node 20+.

```sh
npm install
npm run tauri dev      # run the app
cd src-tauri && cargo test   # parser + unit tests
npm run tauri build    # release bundle
```

App data (SQLite DB + downloads): `%APPDATA%/org.archivebunker.two/`.
