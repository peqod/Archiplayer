# Archiplayer handoff

Updated: 2026-07-24 (Europe/Warsaw) — WP4

## Current state

- Branch: `audit-wave1`
- HEAD: `1cdbfb5` (`fixing handling`)
- WP3 is committed and complete.
- WP4 frontend lifecycle, async correctness, and accessibility work is complete but uncommitted.

## WP4 scope and changes

- Added a small generation-token guard for async UI work and regression coverage for it.
- Prevented stale catalog, search, show-detail, playlist, profile, and download-list responses
  from overwriting newer state or updating destroyed routes.
- Prevented a playlist request from reopening an episode after the user closes it.
- Paused the old archive immediately while a newly selected archive resolves.
- Sanitized persisted and incoming volume values, restored the correct pre-mute volume, and
  handled media `play()` rejections from track navigation.
- Cleaned up the root responsive media-query listener and made live-page polling wait for the
  active request instead of superseding it.
- Added confirmation before deleting an offline download.
- Replaced the episode playlist pseudo-button with a semantic button and added missing labels,
  pressed/expanded/current states, live status/error semantics, a skip link, visible keyboard
  focus, and reduced-motion handling.
- Kept home actions visible on touch devices and made search results and show rows fit narrow
  expanded windows.

## Files changed

- `package.json`
- `src/lib/CatalogNav.svelte`
- `src/lib/TrackRow.svelte`
- `src/lib/player.svelte.ts`
- `src/lib/request-gate.ts` (new)
- `src/lib/volume.ts` (new)
- `src/routes/+layout.svelte`
- `src/routes/+page.svelte`
- `src/routes/live/[id]/+page.svelte`
- `src/routes/profile/+page.svelte`
- `src/routes/show/[id]/+page.svelte`
- `tests/frontend-guards.test.mjs` (new)
- `.agents/HANDOFF.md`

## Verification completed

- `npm run check` — passed with 0 errors and 0 warnings.
- `npm test` — 18 tests passed, including the new request-generation and volume guards.
- `npm run build` — passed.
- `git diff --check` — passed.

No Rust sources or Tauri configuration changed in WP4, so the native/Rust suite was not repeated.

## Remaining note

Tauri warns that the bundle identifier `org.archiplayer.app` ends in `.app`. It remains unchanged
because changing it can move the application-data directory and needs an explicit migration plan
for existing user libraries.

## Next action

Review and commit the WP4 changes plus this handoff. A native no-bundle rebuild is optional because
WP4 only changes frontend code; do not change the bundle identifier without a migration plan.
