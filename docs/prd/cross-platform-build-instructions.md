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
- **Toolchain selection:** `src-tauri/rust-toolchain.toml` stays on host-inferred `stable`; the Windows helpers explicitly select `stable-x86_64-pc-windows-msvc`. This keeps macOS/Linux builds portable without letting a GNU-default Windows installation select the wrong linker.
- Replace the README "in progress" stub with the real steps and drop the link to this PRD.

## Acceptance criteria
- A fresh clone builds a runnable artifact on each OS following only the README.
- CI (if present) is green on Windows, macOS, and Linux.
- The toolchain pin does not force a Windows-only toolchain on non-Windows hosts.

## Status
Implemented in the README on `audit-wave1`. The Linux package list follows Tauri's official
prerequisites, and the bundle arguments match the repository's Ubuntu and macOS CI jobs. Closing
[issue #1](https://github.com/peqod/Archiplayer/issues/1) still requires fresh-clone smoke tests
of the resulting packages on Linux, Apple Silicon, and Intel hardware.
