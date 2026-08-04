<script lang="ts">
  // The headline band: five numbers plus the activity calendar. Everything here is
  // derived from `days`, which the backend returns as one row per local calendar day.
  import { fmtHours, type EraStat, type Stats } from "$lib/api";
  import { buildEraGrid, monthSeconds, streaks } from "$lib/profile-lists";
  import ActivityHeatmap from "$lib/ActivityHeatmap.svelte";

  let { stats, eras }: { stats: Stats; eras: EraStat[] } = $props();

  // Listening rolled up by the era of the archive it came from, rather than by when it was
  // played. Only the peak year is surfaced, as a tile.
  const era = $derived(buildEraGrid(eras));

  const MONTHS = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ];

  const now = new Date();
  const run = $derived(streaks(stats.days, now));
  const thisMonth = $derived(monthSeconds(stats.days, now));

  const since = $derived.by(() => {
    if (!stats.first_listen) return "no history yet";
    const d = new Date(stats.first_listen * 1000);
    return `since ${MONTHS[d.getMonth()]} ${d.getFullYear()}`;
  });

  const perShow = $derived(
    stats.shows.length > 0
      ? `${fmtHours(stats.total_seconds / stats.shows.length)} each on average`
      : "nothing yet",
  );

  const plural = (n: number, word: string) => `${n} ${word}${n === 1 ? "" : "s"}`;
</script>

<section class="band" aria-label="Listening totals">
  <div class="tiles">
    <div class="tile">
      <div class="num">{fmtHours(stats.total_seconds)}</div>
      <div class="label">total listened</div>
      <div class="sub">{since}</div>
    </div>
    <div class="tile">
      <div class="num">{stats.total_sessions}</div>
      <div class="label">sessions</div>
      <div class="sub">recorded plays</div>
    </div>
    <div class="tile">
      <div class="num">{stats.shows.length}</div>
      <div class="label">shows heard</div>
      <div class="sub">{perShow}</div>
    </div>
    <div class="tile">
      <div class="num">{run.current}</div>
      <div class="label">day streak</div>
      <div class="sub">best {plural(run.longest, "day")}</div>
    </div>
    <div class="tile">
      <div class="num">{fmtHours(thisMonth)}</div>
      <div class="label">this month</div>
      <div class="sub">{MONTHS[now.getMonth()]}</div>
    </div>
    <div class="tile">
      <div class="num">{era.peak ? era.peak.year : "-"}</div>
      <div class="label">peak era</div>
      <div class="sub">
        {era.peak ? `${fmtHours(era.peak.seconds)} from that year` : "no archive dates yet"}
      </div>
    </div>
  </div>

  <ActivityHeatmap days={stats.days} firstListen={stats.first_listen} />
</section>

<style>
  .band {
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 12px;
    padding: 18px 20px;
    margin: 14px 0 20px;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
    gap: 4px 18px;
  }
  .tile {
    min-width: 0;
  }
  .num {
    font-size: 26px;
    font-weight: 800;
    line-height: 1.1;
    color: var(--c-accent);
    font-variant-numeric: tabular-nums;
  }
  .label {
    font-size: 12px;
    color: var(--c-text);
    margin-top: 3px;
  }
  .sub {
    font-size: 11px;
    color: var(--c-dim);
    margin-top: 1px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 520px) {
    .band {
      padding: 14px;
    }
    .num {
      font-size: 20px;
    }
  }
</style>
