# PRD: macOS & Linux build-from-source instructions

## Problem
The README "Build from source" section now covers only Windows (MSVC). macOS and
Linux contributors have no verified local build path after the Windows toolchain
was standardized on MSVC and the old cross-platform steps were stubbed out.

## Goal
Verified, step-by-step build-from-source instructions for macOS (universal) and
Linux (Ubuntu/Debian x64), matching the clarity of the Windows section.

## Requirements
- **macOS:** Xcode Command Line Tools; `rustup target add aarch64-apple-darwin x86_64-apple-darwin`; `npm run tauri build -- --target universal-apple-darwin --bundles dmg`; document the output path. Verify on Apple Silicon and Intel.
- **Linux:** WebKitGTK 4.1 plus Tauri packaging deps; `npm run tauri build -- --bundles appimage,deb`; document the output path. Verify on Ubuntu 22.04+/Debian.
- **Toolchain pin:** `src-tauri/rust-toolchain.toml` pins a Windows-only channel (`stable-x86_64-pc-windows-msvc`), which is invalid on macOS/Linux hosts and will break their builds. Decide: switch the pin to plain `stable` (host-inferred) or use host-conditional config. This must be resolved for cross-platform builds.
- Replace the README "in progress" stub with the real steps and drop the link to this PRD.

## Acceptance criteria
- A fresh clone builds a runnable artifact on each OS following only the README.
- CI (if present) is green on Windows, macOS, and Linux.
- The toolchain pin does not force a Windows-only toolchain on non-Windows hosts.

## Status
Tracked as [issue #1](https://github.com/peqod/Archiplayer/issues/1).
