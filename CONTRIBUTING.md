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
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --locked
```

The live smoke test is ignored by default because it performs network requests. Only run it when working on the WFMU integration, and do not loop it.

## Pull requests

Explain the user-visible result, note which platforms you tested, and attach before/after images for interface changes. Keep unrelated refactors separate. Database changes must include a migration path from an existing library rather than requiring users to delete local data.

