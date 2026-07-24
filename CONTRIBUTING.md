# Contributing to Archiplayer

Thank you for helping make the WFMU archive easier to explore.

## Before you start

- Search existing issues before opening a new one.
- Keep requests to WFMU cache-first and limited to the existing global one-request-per-second policy.
- Never commit personal databases, downloaded broadcasts, credentials, or generated build directories.
- For parser changes, add or update a minimal saved HTML fixture and its deterministic test.

## Local checks

Install the prerequisites in the README, then run:

```sh
npm ci
npm run check
npm test
npm run build
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --locked --all-targets --all-features -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

On Windows, dot-source `. .\build-env.ps1` before the Cargo commands so they use
the required MSVC toolchain and SDK environment.

The live smoke test is ignored by default because it performs network requests. Only run it when working on the WFMU integration, and do not loop it.

## Pull requests

Explain the user-visible result, note which platforms you tested, and attach before/after images for interface changes. Keep unrelated refactors separate. Database changes must include a migration path from an existing library rather than requiring users to delete local data.
