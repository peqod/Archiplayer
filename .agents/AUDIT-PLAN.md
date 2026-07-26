# Audit Plan — 2026-07-25

**Status 2026-07-26: all 7 findings fixed, uncommitted, not compiled.** Touched files:
`src-tauri/src/commands.rs` (1, 4, 5, 6 call sites), `src-tauri/src/db.rs` (2, 6),
`src-tauri/src/wfmu.rs` (3), `src-tauri/tauri.conf.json` (7). New tests: FTS backfill,
live-seq preservation, entity double-decode. Nothing built or run: user verifies.

Source: full-codebase audit (Rust backend, core frontend TS, layout + show routes, configs, CI) plus graphify knowledge graph (`graphify-out/graph.html`, 686 nodes / 1383 edges / 40 communities).

Overall verdict: codebase is solid. Guard patterns throughout (DownloadGuard drop-cleanup, `create_new` reservation, rename-then-commit), SSRF-validated audio URLs including every redirect, CSV formula-injection neutralized, no `{@html}` anywhere, tight CSP, minimal Tauri capabilities, generation tokens covering async UI races. Nothing blocking. Items below are ordered worst first.

## Findings

### 1. Panic on poisoned DB mutex — two spots bypass the safe helper
- **Where:** `src-tauri/src/commands.rs:662` (resolve_audio archive-id backfill) and `src-tauri/src/commands.rs:965` (record_listen).
- **What:** `state.db.lock().unwrap()` panics if the mutex is poisoned. Everywhere else uses the `state.db()` helper, which maps poison to `Err("database lock poisoned")`.
- **Impact:** one panic while the lock is held turns these paths into an app crash instead of a recoverable error.
- **Fix:** replace both call sites with `state.db()?`. Two-line change.
- **Priority:** fix.

### 2. FTS index never backfilled
- **Where:** `src-tauri/src/db.rs:151` (`Db::open` creates `tracks_fts` with IF NOT EXISTS); rows only enter the index via `sync_tracks` / `sync_provider_tracks`.
- **What:** a database created before FTS support existed, or a run where FTS table creation failed once, leaves existing tracks permanently absent from the search index.
- **Impact:** search silently misses old tracks.
- **Fix:** on open, when `fts` is true, compare `SELECT COUNT(*) FROM tracks_fts` against `SELECT COUNT(*) FROM tracks`; if the index is short, rebuild it from the tracks table.
- **Priority:** fix.

### 3. `decode_entities` double-decodes
- **Where:** `src-tauri/src/wfmu.rs:318`.
- **What:** `&amp;` is replaced first, so `&amp;lt;` becomes `&lt;` and then the later replace turns it into `<`. A literal `&lt;` in scraped text renders wrong.
- **Fix:** decode `&amp;` last.
- **Priority:** opportunistic. Cosmetic.

### 4. `set_download_dir` grants asset-protocol read on any string the renderer sends
- **Where:** `src-tauri/src/commands.rs:1002` (`allow_directory(&dir, true)`, recursive); same trust class: `export_csv` (`commands.rs:1239`) writes to an arbitrary `dest` path.
- **What:** no existence/type validation before granting scope. A compromised webview could grant itself recursive `asset:` read of `C:\`.
- **Impact:** low likelihood (CSP is strict, no `{@html}` in the app), but cheap to harden.
- **Fix:** verify the path exists and is a directory before granting; optionally reject roots like drive letters. For `export_csv`, require the destination's parent to exist.
- **Priority:** opportunistic hardening.

### 5. Non-FTS search fallback mangles queries
- **Where:** `src-tauri/src/commands.rs:880`.
- **What:** `_` (a LIKE single-char wildcard) is replaced with a space and `%` is stripped, so searching `mr_smith` finds nothing. Proper approach is a LIKE `ESCAPE` clause.
- **Impact:** only builds where FTS5 is unavailable.
- **Priority:** nit.

### 6. Live listening resets episode `seq` to 0
- **Where:** `get_live_status`, `src-tauri/src/commands.rs:330` (`upsert_episode(..., seq: 0)`).
- **What:** a real hosted episode discovered via live status gets `seq=0`, jumping it to the top of its show's episode ordering until the next show-page scrape rewrites seqs.
- **Impact:** transient ordering quirk in the show list.
- **Fix:** preserve existing seq on conflict when the caller passes no meaningful ordering (e.g. `seq=COALESCE(NULLIF(?7, 0), seq)` or a dedicated upsert for the live path).
- **Priority:** nit.

### 7. CSP wider than needed
- **Where:** `src-tauri/tauri.conf.json:23`.
- **What:** `connect-src`/`media-src` allow `https://*.amazonaws.com` (any S3 bucket). The backend only accepts `s3.amazonaws.com/arch.wfmu.org/`.
- **Fix:** narrow to `https://s3.amazonaws.com`.
- **Priority:** nit.

## Non-issues checked and cleared
- Std mutex held across `.await`: compiler-enforced impossible (`MutexGuard` is `!Send`); all guard scopes verified sync.
- Download pipeline races: `active_downloads` set + `create_new` `.part` reservation + destination re-check before rename cover concurrent and cross-process cases.
- XSS: no `{@html}` / `innerHTML` anywhere; Svelte text interpolation throughout; `script-src 'self'`.
- CSV injection: `neutralize_csv_cell` covers `= + - @ \t \r \n`, with tests.
- Audio URL SSRF: `validate_audio_url` enforces HTTPS, no credentials, port 443, exact host allowlist, S3 bucket path prefix; applied to initial URL and every redirect (custom redirect policy, max 10).
- Frontend async races: generation tokens (`PlaybackTransitions`, `LatestRequest`) gate every stale completion; timers cleaned up in effect/onMount teardown.
- Workflows: minimal permissions, version gate before publish, checksums computed from re-downloaded assets.

## Graph pointers (for future sessions)
- Knowledge graph exists at `graphify-out/graph.json`; query with `graphify query "<question>"` instead of rebuilding.
- Core abstractions by degree: `Db`, `$lib/api`, `AppState`, `Player`, `$lib/player.svelte`.
- Known graph health caveats: 64 dangling-endpoint edges (semantic chunk ids outside extraction) and ~155 collapsed parallel edges (AST multi-field refs merged, benign).
