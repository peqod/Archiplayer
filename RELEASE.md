# Releasing Archiplayer

Standard operating procedure for cutting a release. Releases are driven by **pushing a git tag**; GitHub Actions does the building, checksumming, and drafting. This document is the single source of truth for the process.

## Principles

- **The tag is the trigger.** Pushing `vX.Y.Z` starts `.github/workflows/release.yml`. Nothing else needs doing to start a build.
- **CI owns the release and `SHA256SUMS.txt`.** The workflow builds all three platforms and attaches the checksums. You do not build or hash locally for a release.
- **Never run `gh release create`.** It creates the tag *and* pre-publishes a release, colliding with what CI produces. (This is exactly how the v0.3.0 release got cut Windows-only by hand and had to be backfilled by CI.) The only correct trigger is a tag push.
- **The release starts as a draft.** It is not visible on the website until you publish it (the site reads `releases/latest`, which excludes drafts).

## Prerequisites

- Windows with **PowerShell 7** (`pwsh`).
- **Node 20** and **Rust stable** (matches CI).
- **GitHub CLI** authenticated with push rights: `gh auth login`, verify `gh auth status`.
- A clean `main` that is level with `origin/main`.
- Visual Studio Build Tools (Desktop C++) + a Windows SDK are only needed for the optional local smoke build, not for the release itself.

## Cut a release — one command

```powershell
pwsh -File scripts/release.ps1 -Version 0.4.0
```

This will, in order: check preconditions → bump every version file → run the version gate → commit `release: v0.4.0` → push `main` → **prompt for confirmation** → create and push the `v0.4.0` tag (starts CI) → watch the run → verify all five assets landed → publish the draft.

**Switches**

| Switch | Effect |
|---|---|
| `-Version X.Y.Z` | Required. Semantic version without the leading `v`. |
| `-WhatIf` | Dry run. Prints every mutating step, changes nothing. Run this first if unsure. |
| `-NoPublish` | Push the tag and watch CI, but leave the release as a draft for manual review. |
| `-NoWatch` | Push the tag and exit immediately (does not watch or publish). |
| `-Yes` | Skip the interactive confirmation before the tag push. |

The confirmation prompt before the tag push is the point of no return — after it, the tag is public and CI is building.

## Cut a release — fully manual equivalent

Use this when the script is unavailable or you want to understand each step. Replace `0.4.0` with the new version.

```bash
# 1. Bump the version in every manifest (keep them identical):
#    package.json, package-lock.json (root + packages[""]),
#    src-tauri/tauri.conf.json, src-tauri/Cargo.toml,
#    src-tauri/Cargo.lock (the `name = "archiplayer"` entry only)
npm version 0.4.0 --no-git-tag-version --allow-same-version   # handles package.json + lock
#    then edit tauri.conf.json, Cargo.toml, Cargo.lock by hand

# 2. Gate: all versions must agree and match the tag
node scripts/verify-release-version.mjs v0.4.0

# 3. Commit exactly the version files
git add package.json package-lock.json src-tauri/tauri.conf.json src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "release: v0.4.0"

# 4. Push main (does NOT trigger CI)
git push origin main

# 5. Tag and push the tag (THIS triggers CI)
git tag -a v0.4.0 -m "Archiplayer v0.4.0"
git push origin v0.4.0

# 6. Watch, verify, publish
gh run watch $(gh run list --workflow release.yml --limit 1 --json databaseId --jq '.[0].databaseId') --exit-status
gh release view v0.4.0 --json assets --jq '.assets[].name'   # expect 5 assets
gh release edit v0.4.0 --draft=false
```

## How CI works

`.github/workflows/release.yml`:

- **Trigger:** `push` on tags matching `v[0-9]+.[0-9]+.[0-9]+`.
- **`publish` job:** a 3-platform matrix (`fail-fast: false`) — `windows-latest` (`nsis`), `ubuntu-22.04` (`appimage,deb`), `macos-latest` (universal `dmg`). Each runs the version gate, then `tauri-apps/tauri-action` with `releaseDraft: true`, creating/filling a **draft** release named `Archiplayer <tag>`.
- **`checksums` job (`needs: publish`):** downloads every asset, runs `sha256sum` into `SHA256SUMS.txt`, and uploads it. Because it *needs* `publish`, **if any single platform leg fails, checksums is skipped** and the draft is left partial — do not publish it.

Expected assets on a complete release (5):

```
Archiplayer_<ver>_x64-setup.exe     (Windows)
Archiplayer_<ver>_universal.dmg     (macOS)
Archiplayer_<ver>_amd64.AppImage    (Linux)
Archiplayer_<ver>_amd64.deb         (Linux)
SHA256SUMS.txt
```

**Asset-name contract:** the website (`site/site.js`) matches assets by regex — Windows `/setup.*\.exe$|\.msi$/i`, macOS `/\.dmg$/i`, Linux `/\.AppImage$|\.deb$/i`, checksums `/SHA256SUMS/i`. Do not rename these outputs.

## Verify and publish

```powershell
gh release view v0.4.0 --json assets --jq '.assets[].name'   # confirm 5 assets
gh release edit v0.4.0 --draft=false                          # publish
```

After publishing, the site (which reads `releases/latest`) updates on the next load: the download buttons and the `SHA256SUMS.txt` link resolve to the new assets. **Nothing on the site changes while the release is a draft** — that is expected, not a failure.

## Local smoke build (optional, not a release)

Before tagging, you can build a Windows installer locally to sanity-check:

```powershell
npm run build:windows
```

This loads the MSVC/SDK environment and copies an unsigned `Archiplayer_<version>_x64-setup.exe` to the repo root. It is a smoke test only — it is git-ignored and is **not** how releases are produced.

## Post-release manual actions

- Submit the Windows installer to **VirusTotal**; put the permalink into the release notes and the site's `data-vt-todo` link (`site/index.html`).
- Announce per the launch plan.

## Reproducibility — honest

The bundles are **not** byte-for-byte reproducible: NSIS, `.dmg`, and `.AppImage` embed build timestamps and packaging metadata, so two builds of the same commit differ in bytes. Reproducibility here means a **repeatable process** with pinned inputs — `package-lock.json`, `Cargo.lock` (`--locked` in CI), Node 20, Rust stable, and pinned action/toolchain versions. Builds are functionally equivalent, not bit-identical. Byte-identical reproducibility would require code signing removal, deterministic packagers, and pinned SDKs, which are out of scope.

## Optional: pruning old releases

Automation never deletes releases. To prune manually (for example, to remove an old release with an outdated binary), delete the release but **keep the tag**:

```powershell
gh release delete v0.2.0 --yes        # keeps the git tag (no --cleanup-tag)
```

Tags are permanent history; do not pass `--cleanup-tag`.

## Troubleshooting

**Version-gate mismatch** — `verify-release-version.mjs` names the file that drifted (`package`, `tauri`, `cargo`, or `cargo-lock`). Fix that file to match and re-run the gate. `package-lock.json` is intentionally not gated.

**A platform leg failed** — the `publish` job failed, so `checksums` was skipped: the draft has partial assets and no `SHA256SUMS.txt`. Do **not** publish it. Fix the build, then re-tag (below).

**Re-pushing a tag** (to rebuild the same version) — `tauri-action` reuses any existing release for the tag, so delete the draft first:

```powershell
gh release delete v0.4.0 --yes
git push origin :refs/tags/v0.4.0     # delete remote tag
git tag -d v0.4.0                     # delete local tag
# re-tag and push
git tag -a v0.4.0 -m "Archiplayer v0.4.0"
git push origin v0.4.0
```

Note: deleting a tag deletes neither its prior Actions runs nor the release. A tag can accumulate multiple runs — check `gh run list --workflow release.yml` if you see duplicates.

**Remote-only tag** — if `release.ps1` reports the tag exists on the remote but not locally, a previous release (or a stray `gh release create`) created it there. Reconcile before proceeding: `git fetch --tags`, inspect with `git ls-remote --tags origin`.
