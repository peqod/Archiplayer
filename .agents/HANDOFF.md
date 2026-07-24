# Archiplayer handoff

Updated: 2026-07-24 (Europe/Warsaw)

## Current state

- Branch: `audit-wave1`
- HEAD: `ec08485` (`fixing handling`)
- WP3 hardening and verification are complete.
- The working tree contains intentional, uncommitted changes in:
  - `src-tauri/src/commands.rs`
  - `src-tauri/src/downloads.rs`
  - `src/routes/live/[id]/+page.svelte`

## WP3 changes after the previous commits

- Added cancellation-safe cleanup for the in-process active-download set.
- Removed both partial and renamed destination files when download finalization fails.
- Kept episode IDs at the end of download filenames even when long metadata is truncated.
- Expanded CSV formula-injection protection to tab, carriage-return, and newline prefixes.
- Added regression tests for active-download cleanup and long filename collision resistance.
- Removed stale `.ghost` selectors; Svelte checks are warning-free.

## Verification completed

- `npm run check` — passed with 0 errors and 0 warnings.
- `npm test` — passed.
- `npm run build` — passed as part of the native build.
- `cargo fmt --manifest-path src-tauri/Cargo.toml --check` — passed.
- `cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings` — passed.
- `cargo test --manifest-path src-tauri/Cargo.toml --locked` — 42 passed; the 2 networked live-smoke tests remained ignored as intended.
- `. .\build-env.ps1; npm run tauri build -- --no-bundle` — passed.
- Windows release binary: `src-tauri/target/release/archiplayer.exe` (17,958,400 bytes).
- `git diff --check` — passed.

## Remaining note

Tauri warns that the bundle identifier `org.archiplayer.app` ends in `.app`. It was deliberately
left unchanged because changing the identifier can move the application-data directory and needs
an explicit migration plan for existing user libraries.

## Next action

Review and commit the three WP3 source changes plus this handoff. Do not rebuild or change the
bundle identifier unless specifically requested.
