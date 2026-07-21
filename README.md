<div align="center">
  <img src="static/logo@2x.gif" width="144" alt="Archiplayer logo">

  # Archiplayer

  **Decades of freeform radio, filed beautifully.**

  A local-first desktop browser and player for the [WFMU](https://wfmu.org/) archives.<br>
  Rust + Tauri 2 + Svelte 5 · No login · No cloud · Open source

  [Website](../../deployments/github-pages) · [Latest release](../../releases/latest) · [Build from source](#build-from-source) · [Report a problem](../../issues/new/choose)
</div>

---

Archiplayer turns WFMU’s enormous public archive into a personal desktop library. Browse current and defunct shows, search DJs and cached playlists, play directly from track timestamps, keep favourites, download episodes, and inspect your listening history. All personal data stays in a local SQLite database.

> [!IMPORTANT]
> Release builds are currently unsigned. Windows SmartScreen and macOS Gatekeeper will therefore show a warning. Verify that the file came from this repository and compare its SHA-256 hash with `SHA256SUMS.txt`, or [build from source](#build-from-source).

## Download

| Platform | Artifact | Support |
|---|---|---|
| Windows | NSIS `.exe` | Windows 10+, x64 |
| macOS | Universal `.dmg` | Intel and Apple Silicon |
| Linux | `.AppImage` or `.deb` | x64 Linux / Debian-based distributions |

Download the matching file from the [latest release](../../releases/latest).

### Opening an unsigned build

- **Windows:** open the installer, choose **More info**, verify the source, then choose **Run anyway**.
- **macOS:** move Archiplayer to Applications, Control-click it, choose **Open**, then confirm once.
- **Linux AppImage:** make it executable with `chmod +x Archiplayer*.AppImage`, then run it.

## What it does

- Browse roughly 580 current and defunct WFMU shows with an A–Z index.
- Search shows and DJs immediately; search tracks from every playlist already cached locally.
- Play a show, episode, or individual song from its playlist timestamp.
- Resume episodes and maintain a queue from a persistent player.
- Favourite shows, episodes, and tracks.
- Download broadcasts for offline listening to a directory you choose.
- Track listening time and export favourites, listens, and stats as CSV.

## Build from source

You need [Node.js 20 or newer](https://nodejs.org/), npm, [Rust via rustup](https://rustup.rs/), and Git.

Clone the repository, then run the common setup:

```sh
git clone https://github.com/peqod/Archiplayer.git
cd Archiplayer
npm ci
```

### Windows (x64)

1. Install **Visual Studio Build Tools** with the **Desktop development with C++** workload. It must include MSVC v143, a Windows 10/11 SDK, and WebView2 (WebView2 ships with Windows 11).
2. Install Rust from [rustup.rs](https://rustup.rs/). The MSVC toolchain is pinned in `src-tauri/rust-toolchain.toml`, so rustup installs `stable-x86_64-pc-windows-msvc` automatically on the first `cargo` run. No manual `rustup default` is needed.
3. Install JS dependencies: `npm ci`.
4. **Run in development:** `npm run dev:windows`. This dot-sources `build-env.ps1` to load `link.exe` plus the Windows SDK, then starts `tauri dev`. To reuse your current shell instead, run `. .\build-env.ps1` once, then `npm run tauri dev`.
5. **Build the installer:** `npm run build:windows`. The NSIS installer is copied to the repository root as `Archiplayer_<version>_x64-setup.exe` (and remains available under `src-tauri/target/release/bundle/nsis/`).

> If `cargo` reports a file lock ("used by another process"), stop a running `archiplayer.exe` first (`taskkill /F /IM archiplayer.exe`), then rebuild.

### macOS and Linux

Build-from-source steps for macOS (universal `.dmg`) and Linux (AppImage / deb) are in progress, tracked in issue [#1](https://github.com/peqod/Archiplayer/issues/1) ([PRD](docs/prd/cross-platform-build-instructions.md)). The Download table above still applies to published releases.

## Develop and test

After installing the prerequisites for your OS:

```sh
npm ci
npm run tauri dev
```

On Windows, use the helper that discovers Visual Studio Build Tools and loads `link.exe` plus the Windows SDK into the shell:

```powershell
npm run dev:windows
```

If you prefer to keep using the current PowerShell session, run `. .\build-env.ps1` once and then use the ordinary Tauri commands.

Run the deterministic checks before opening a pull request:

```sh
npm run check
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

There is also an ignored smoke test that reads the live WFMU site. Use it sparingly:

```sh
cargo test --manifest-path src-tauri/Cargo.toml --test live_smoke -- --ignored --nocapture
```

## How it works

```text
Svelte interface
      │ Tauri commands and events
      ▼
Rust application ── SQLite library, favourites, history and settings
      │
      ├── polite cache-first requests to public WFMU playlist pages
      ├── direct archive audio playback
      └── user-selected offline download directory
```

WFMU does not expose an official archive API. Archiplayer parses the public KenzoDB pages on demand, limits those page requests to one per second, and caches results rather than crawling the site.

| Data | Public source |
|---|---|
| Show catalogue | `wfmu.org/playlists/` |
| Episodes | `wfmu.org/playlists/{show}` |
| Playlist | `wfmu.org/playlists/shows/{episode}` |
| Audio | WFMU archive player → direct archive media |

Saved HTML fixtures protect the parsers from accidental regressions. When WFMU changes its markup, update the fixtures and parser tests together.

### Local data

Tauri chooses the operating system’s application-data directory for `org.archiplayer.app`. The directory contains `library.db` and, unless changed in Profile, downloaded episodes. Nothing is synchronized or uploaded by Archiplayer.

## Releases

Pull requests and main-branch pushes run frontend checks, Rust tests, and native no-bundle builds on Windows, macOS, and Ubuntu. A semantic version tag such as `v0.2.0` creates a **draft** GitHub Release with:

- Windows x64 NSIS installer
- macOS universal DMG
- Linux x64 AppImage and DEB
- `SHA256SUMS.txt`

Review and smoke-test all artifacts before publishing the draft. Keep the version in `package.json`, `src-tauri/Cargo.toml`, and `src-tauri/tauri.conf.json` aligned with the tag.

## Contributing

Bug reports, parser fixtures, accessibility improvements, design work, and platform testing are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md) before sending a change. Please do not run broad crawls against WFMU while developing.

Funding is intentionally not configured in this repository yet. It will be designed as one project-wide channel shared with the broader **@ontodesign** portfolio rather than as an Archiplayer-only tip link.

## Respect WFMU

WFMU is a listener-supported, non-commercial freeform station. Archiplayer is an independent project and is not affiliated with WFMU. If the archive gives you something good, please [donate directly to WFMU](https://pledge.wfmu.org/donate).

## License

[MIT](LICENSE)
