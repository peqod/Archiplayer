# Changelog

Notable changes per release. The section for a tag becomes that release's notes on GitHub, so
`scripts/extract-changelog.mjs` refuses to release a version that has no section here.

Format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow
[Semantic Versioning](https://semver.org/spec/v2.0.0.html). Releases before 0.4.0 predate this
file; their notes are on the [releases page](https://github.com/peqod/Archiplayer/releases).

## [Unreleased]

## [0.4.5] - 2026-07-31

### Added

- Long show and song titles that do not fit the player scroll past at a steady reading speed
  instead of being cut off.

### Fixed

- Episodes that once played and later stopped loading now recover on their own. WFMU retires
  its 128k mp3s after a few weeks and falls back to a permanent archive file, so a remembered
  address goes dead; playback and downloads now look the address up again and resume where
  they left off.
- System-wide shortcuts no longer capture Ctrl+Alt/AltGr character input on international
  keyboard layouts. Legacy Ctrl+Alt defaults migrate to Ctrl+Shift+F6 through F11, and the
  shortcut picker says which chords cannot be taken system-wide and why.
- Precision seeking now follows how wide the seek bar actually is, so it stays available in
  window shapes where the old viewport breakpoint switched it off.
- Switching away from a live stream fades it out instead of cutting, and pressing play on the
  source already loaded stays an immediate pause/resume rather than a delayed reload.
- The episode list no longer drifts after a window resize: the corrective scroll settles in
  the same frame rather than animating.

## [0.4.1] - 2026-07-27

### Fixed

- Archived WFMU shows now stop at their scheduled broadcast boundary instead of continuing into
  the next show, while allowing a 30-second grace period for closing jingles.
- Audition playback now fades in and out over 1.5 seconds at its boundaries for smoother starts
  and finishes.

## [0.4.0] - 2026-07-27

### Added

- **Keyboard shortcuts**, in the app and system-wide, set under Profile, Keyboard shortcuts.
  Defaults are `Space` play/pause, `J`/`K` seek, `,`/`.` previous/next, `M` mute, `F` favourite,
  `S` share, `R` random show; the system-wide tier repeats seek, mute, favourite, share and
  random on `Ctrl+Alt+`. A binding records the physical key, so it is the same key on every
  layout, and one switch turns the system-wide tier off. Transport is left to the OS media keys
  through `mediaSession`, which a global grab would otherwise starve.
- **Loupe scrubbing.** Press the seek bar to jump to that point; drag and a magnifier opens,
  gearing travel down to a quarter of a bar pixel so the seek resolution matches what the lens
  is showing.
- **A floating back control.** Once the inline "All shows" link scrolls out of view on a show or
  live page, a pill takes over, positioned so its text lands exactly where the link's text was.
- **Scroll-to-top cue.** The nav brand becomes a scroll-to-top control on the same scroll travel
  that hands the back link over to its pill, so the two read as one change of state.

### Fixed

- Playlist parsing no longer leaks page chrome into the library. Script and style bodies are
  stripped before tags, so their JS and CSS cannot survive as words in a title or a track field;
  HTML entities decode `&amp;` last so correctly escaped text is not double-decoded; and a
  plausibility gate drops rows that are document fragments rather than tracks. An episode whose
  every row fails the gate now ends up with no playlist instead of a garbage one. Regression
  fixture added for the playlist that surfaced this.
- The folded volume popover in mini mode no longer disappears the moment the pointer leaves it.
- Play controls now report the player's state rather than the action a click performs, and do so
  identically in the transport, episode rows, track rows and live. Their labels stay
  action-worded, which is what assistive technology should announce.

### Changed

- **Windows is the only distributed platform.** The macOS leg is paused alongside Linux. Both
  still build from source, and CI keeps building macOS on every pull request.
- Release notes now come from this file. CI reads the section matching the tag, and the local
  release gate refuses a tag that has no section.
- The content security policy narrowed from `*.amazonaws.com` to `s3.amazonaws.com`.
- Release tooling forwards native command flags correctly.
