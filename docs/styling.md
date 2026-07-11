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
| show page | `.playbtn` `.mini` `.tplay` `.primary` `.ghost` | `src/routes/show/[id]/+page.svelte` |
| shows list | `.rbtn.play` `.ghost` | `src/routes/+page.svelte` |
| profile downloads | `.dl-play` `.dl-open` `.dl-del` | `src/routes/profile/+page.svelte` |
| icon engine | `box` default, `.icon { vertical-align }` | `src/lib/Icon.svelte` |

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
