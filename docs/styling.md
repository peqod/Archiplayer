# Styling the player buttons & icons

## Presets (change once, applies everywhere)

Size/spacing presets are CSS variables in `src/routes/+layout.svelte` → `:global(:root)`
(next to the `--c-*` colour tokens):

| token | controls | default |
|-------|----------|---------|
| `--icon-size` | glyph size inside every `<Icon>` | `1.35em` |
| `--pbtn-size` | transport prev/next buttons | `36px` |
| `--pbtn-main-size` | transport main play/pause button | `46px` |
| `--pctl-gap` | gap between transport buttons | `6px` |
| `--player-gap` | gap between player sections | `18px` |
| `--hsl-h` | row height of a painted horizontal slider | `18px` |
| `--hsl-track` | its bar thickness (scrubber + desktop volume) | `3px` |
| `--hsl-thumb` | its knob diameter | `12px` |

Edit a value here → it changes **globally**.

## Overrides (custom values in specific places)

Set the same variable **locally** on the target — the nearest value wins:

- **Inline** on one element: `style="--icon-size:1.7em; --pbtn-size:44px"`.
- **Modifier class** on a group: `.pbtn.compact { --pbtn-size: 28px; }` then `class="pbtn compact"`.
- **Per-instance icon**: the `size` prop beats the cascade → `<Icon name="play" size="1.6em" />`.

So: global default in `:root`, then override in place #1 and place #2 with one line each.

## Targeting a button / its icon

Each button is a CSS class scoped to its component file:

| where | classes | file |
|-------|---------|------|
| transport | `.player` `.p-controls` `.pbtn` `.pbtn.main` | `src/routes/+layout.svelte` |
| sliders | `.hsl` `.hsl-track` `.hsl-fill` `.hsl-thumb` `.hsl-tick` | `src/routes/+layout.svelte` |
| show page | `.playbtn` `.mini` `.tplay` `.primary` `.ghost` | `src/routes/show/[id]/+page.svelte` |
| shows list | `.rbtn.play` `.ghost` | `src/routes/+page.svelte` |
| profile downloads | `.dl-play` `.dl-open` `.dl-del` | `src/routes/profile/+page.svelte` |
| icon engine | `box` default, `.icon { vertical-align }` | `src/lib/Icon.svelte` |

The scrubber and the desktop volume bar are **painted**, not native: a `.hsl-track` /
`.hsl-fill` / `.hsl-thumb` trio with a transparent `<input type="range">` (`.sl-input`) on top,
so `accent-color` does nothing to them. `.hsl-track` is inset by half a thumb on each side —
put anything positional (`.hsl-tick`) inside it and `left: N%` lands where the thumb lands at
`N%`. The mini volume popover is the same idea rotated (`.vol-track` / `.vol-tick` / `.vol-v`).

Holding the desktop scrubber raises a **loupe**: `.hsl.seek.lensing` hides the thumb and shows a
`.seek-lens` circle containing `.lens-strip`, which is the same track, fill and marks re-rendered
at `--lens-zoom` and slid so the held position sits under `.lens-cross`. Both sizing variables are
set inline from `LOUPE_SIZE` and `LOUPE_ZOOM` in `src/lib/seek-drag.ts` rather than from `:root`,
because the zoom is not only a look: the same constant divides pointer travel in `gearedTime()`,
so the magnification and the seek resolution can never drift apart. `--lens-size` defaults to
`36px` and `--lens-zoom` to `4`. The mini scrubber and the volume sliders never lens.

The nav brand is a **two-state control**. Unscrolled it is the wordmark; once `<main>` passes
`SCROLL_CUE_AT` (`src/lib/scroll-cue.ts`) `.brand` gains `.cued` and it crossfades to an
`arrow-up` glyph plus "Press to scroll to top". The threshold is `<main>`'s top padding plus one
line of the inline "← All shows" link, i.e. the scroll position where `BackButton` hands that
link over to its floating pill — the two are meant to change together, so move them together.
Each half is a `.brand-slot` grid stack holding both its states in one cell, so the slot is as
wide as the wider state and the swap never reflows the tabs. Logo and arrow share `--brand-mark`
(34px, 26px under 760px, 24px under 420px); `.brand-type` is hidden below 760px, where only the
mark swaps.

The icon is a **child `Icon` component** (`<svg class="icon">`). Svelte scoped CSS can't reach
into a child component, so a plain `.pbtn.main svg {}` does nothing. Pierce it with `:global()`,
kept under the scoped parent so it hits only that button:

```css
/* only the main transport button's glyph */
.pbtn.main :global(svg.icon) {
  width: 22px;
  height: 22px;
  transform: translateX(1px);   /* optical-centre the triangle */
}
```

- Baseline too high/low? adjust `.icon { vertical-align: -0.15em }` in `Icon.svelte`.
- One icon looks off-centre for its box? tweak that icon's `viewBox` in `Icon.svelte` — the
  `next`/`prev` art is wide (511×324) vs square `play`/`pause`/`playing` (498×501).
