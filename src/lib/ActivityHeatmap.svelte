<script lang="ts">
  // Calendar of listening intensity, one cell per day, Monday at the top of each column and
  // one stacked row per year. Plain CSS grid rather than SVG: no viewBox maths, the columns
  // are fr tracks so the grid fills whatever width the band has, and the cells inherit the
  // theme through color-mix so every user palette keeps working.
  import { fmtHours, type DayStat } from "$lib/api";
  import { buildYearGrids, YEAR_COLUMNS, type DayCell } from "$lib/profile-lists";
  import { onMount } from "svelte";

  let { days, firstListen }: { days: DayStat[]; firstListen: number | null } = $props();

  const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
  const DAY_MS = 86_400_000;
  /** The backend only keeps seven years of day rows, so asking for more would draw blanks. */
  const MAX_YEARS = 7;

  // Fewer rows on a narrow window. The cells stay square and legible at any width, but a
  // seven-row stack on a phone is mostly scrollbar.
  let maxYears = $state(MAX_YEARS);

  onMount(() => {
    const mid = window.matchMedia("(max-width: 760px)");
    const small = window.matchMedia("(max-width: 520px)");
    const sync = () => {
      maxYears = small.matches ? 1 : mid.matches ? 3 : MAX_YEARS;
    };
    sync();
    mid.addEventListener("change", sync);
    small.addEventListener("change", sync);
    return () => {
      mid.removeEventListener("change", sync);
      small.removeEventListener("change", sync);
    };
  });

  const now = new Date();

  const years = $derived.by(() => {
    const last = now.getFullYear();
    const earliest = firstListen ? new Date(firstListen * 1000).getFullYear() : last;
    const first = Math.max(earliest, last - (maxYears - 1));
    const out: number[] = [];
    for (let y = last; y >= first; y--) out.push(y);
    return out;
  });

  const grids = $derived(buildYearGrids(days, years, now));

  // Month starts for the newest row. Across years a month's column shifts by at most one
  // column, which is under a cell width, so one shared header is honest and far quieter
  // than stamping the same twelve words onto every row.
  const monthLabels = $derived.by(() => {
    const year = years[0] ?? now.getFullYear();
    const jan1 = new Date(year, 0, 1);
    const offset = (jan1.getDay() + 6) % 7;
    return MONTHS.map((label, m) => {
      const doy = Math.round((new Date(year, m, 1).getTime() - jan1.getTime()) / DAY_MS);
      return { label, col: Math.floor((offset + doy) / 7) + 1 };
    });
  });

  function niceDay(key: string): string {
    const [y, m, d] = key.split("-").map(Number);
    return `${d} ${MONTHS[(m ?? 1) - 1]} ${y}`;
  }

  function rowLabel(
    year: number,
    seconds: number,
    activeDays: number,
    busiest: DayCell | null,
  ): string {
    if (!busiest) return `${year}, no listening.`;
    return `${year}, ${fmtHours(seconds)} across ${activeDays} active days, busiest ${fmtHours(busiest.seconds)} on ${niceDay(busiest.day)}.`;
  }

  // One delegated listener rather than a title attribute per cell: seven years is roughly
  // 2600 cells, and native tooltips there are both slow to open and unstyleable.
  let tip = $state<{ x: number; y: number; text: string } | null>(null);
  let box: HTMLDivElement | undefined = $state();
  let hovered: HTMLElement | null = null;

  function onPointer(event: PointerEvent) {
    const cell = (event.target as HTMLElement | null)?.closest<HTMLElement>(".cell[data-day]");
    if (!cell || !box) {
      hovered = null;
      tip = null;
      return;
    }
    // Pointermove fires per pixel; only the cell changing is worth a re-render.
    if (cell === hovered) return;
    hovered = cell;
    const day = cell.dataset.day ?? "";
    const secs = Number(cell.dataset.secs ?? 0);
    const here = cell.getBoundingClientRect();
    const outer = box.getBoundingClientRect();
    tip = {
      x: here.left - outer.left + here.width / 2,
      y: here.top - outer.top,
      text: secs > 0 ? `${niceDay(day)}, ${fmtHours(secs)}` : `${niceDay(day)}, no listening`,
    };
  }
</script>

{#if days.length === 0}
  <p class="empty">No listening history yet. Play something and this fills in.</p>
{:else}
  <div class="heat" bind:this={box}>
    <div class="head" aria-hidden="true">
      <span class="ylab"></span>
      <div class="months" style="--cols:{YEAR_COLUMNS}">
        {#each monthLabels as m (m.label)}
          <span style="grid-column:{m.col}">{m.label}</span>
        {/each}
      </div>
    </div>

    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="rows"
      onpointermove={onPointer}
      onpointerleave={() => {
        hovered = null;
        tip = null;
      }}
    >
      {#each grids as grid (grid.year)}
        <div class="row">
          <span class="ylab">{grid.year}</span>
          <div
            class="cells"
            style="--cols:{YEAR_COLUMNS}"
            role="img"
            aria-label={rowLabel(grid.year, grid.seconds, grid.activeDays, grid.busiest)}
          >
            {#each grid.weeks as week, w (w)}
              <div class="col">
                {#each week as cell, d (d)}
                  {#if cell.day}
                    <div class="cell l{cell.level}" data-day={cell.day} data-secs={cell.seconds}></div>
                  {:else}
                    <div class="cell pad"></div>
                  {/if}
                {/each}
              </div>
            {/each}
          </div>
        </div>
      {/each}
    </div>

    {#if tip}
      <div class="tip" style="left:{tip.x}px; top:{tip.y}px">{tip.text}</div>
    {/if}
  </div>
{/if}

<style>
  /* fr columns rather than a fixed cell size: the grid takes whatever width the band gives
     it and the cells stay square through aspect-ratio. The max-width stops an ultrawide
     window from inflating the cells into tiles. */
  .heat {
    position: relative;
    max-width: 960px;
    margin-top: 16px;
    --gap: 2px;
    --ylab: 30px;
  }
  .head,
  .row {
    display: grid;
    grid-template-columns: var(--ylab) minmax(0, 1fr);
    align-items: center;
    gap: 8px;
  }
  .row + .row {
    margin-top: 6px;
  }
  .months {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: var(--gap);
    font-size: 9px;
    line-height: 1;
    color: var(--c-dim);
    margin-bottom: 3px;
    height: 10px;
  }
  .months span {
    white-space: nowrap;
    overflow: visible;
  }
  .ylab {
    font-size: 11px;
    line-height: 1;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .cells {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: var(--gap);
  }
  .col {
    display: grid;
    grid-template-rows: repeat(7, auto);
    gap: var(--gap);
  }
  .cell {
    aspect-ratio: 1;
    border-radius: 1px;
    background: var(--c-surface2);
  }
  .cell.l1 {
    background: color-mix(in srgb, var(--c-accent) 25%, var(--c-surface2));
  }
  .cell.l2 {
    background: color-mix(in srgb, var(--c-accent) 45%, var(--c-surface2));
  }
  .cell.l3 {
    background: color-mix(in srgb, var(--c-accent) 70%, var(--c-surface2));
  }
  .cell.l4 {
    background: var(--c-accent);
  }
  .cell.pad {
    background: transparent;
  }
  .tip {
    position: absolute;
    transform: translate(-50%, -100%);
    margin-top: -6px;
    padding: 4px 7px;
    border-radius: 6px;
    background: var(--c-surface2);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    font-size: 11px;
    line-height: 1.2;
    white-space: nowrap;
    pointer-events: none;
    z-index: 5;
  }
  .empty {
    color: var(--c-dim);
    font-size: 13px;
    margin: 16px 0 0;
  }
  @media (max-width: 520px) {
    .heat {
      --ylab: 26px;
    }
  }
</style>
