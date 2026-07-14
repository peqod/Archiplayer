<script lang="ts">
  import { page } from "$app/stores";
  import {
    api,
    LIVE_STREAMS,
    type LivePage,
    type LiveProgram,
    type Track,
  } from "$lib/api";
  import Icon from "$lib/Icon.svelte";
  import TrackRow from "$lib/TrackRow.svelte";
  import { player } from "$lib/player.svelte";
  import { onMount } from "svelte";

  const streamId = $derived($page.params.id ?? "");
  const stream = $derived(LIVE_STREAMS.find((candidate) => candidate.id === streamId));
  let detail = $state<LivePage | null>(null);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let requestGeneration = 0;

  async function load(refresh = false) {
    if (!stream) {
      error = "Unknown live station.";
      loading = false;
      return;
    }
    const generation = ++requestGeneration;
    if (refresh) refreshing = true;
    try {
      const next = await api.getLivePage(stream.id, refresh);
      if (generation !== requestGeneration) return;
      detail = next;
      error = null;
    } catch (cause) {
      if (generation !== requestGeneration) return;
      error = String(cause);
    } finally {
      if (generation === requestGeneration) {
        loading = false;
        refreshing = false;
      }
    }
  }

  $effect(() => {
    void streamId;
    detail = null;
    loading = true;
    void load();
  });

  onMount(() => {
    const timer = setInterval(() => void load(), 30_000);
    return () => {
      clearInterval(timer);
      requestGeneration += 1;
    };
  });

  async function favourite(track: Track) {
    try {
      const favourite = await api.toggleFavourite("track", String(track.id));
      if (detail) {
        detail = {
          ...detail,
          tracks: detail.tracks.map((candidate) =>
            candidate.id === track.id ? { ...candidate, favourite } : candidate,
          ),
        };
      }
      player.setTrackFavourite(track.id, favourite);
    } catch (cause) {
      error = String(cause);
    }
  }

  function playedTime(timestamp: number | null): string | null {
    if (timestamp === null) return null;
    return new Intl.DateTimeFormat(undefined, {
      timeZone: "America/New_York",
      hour: "numeric",
      minute: "2-digit",
    }).format(new Date(timestamp * 1000));
  }

  function programTime(program: LiveProgram): string {
    const start = program.starts_at;
    const end = program.ends_at;
    if (!start) return "Later today";
    const parse = (value: string) => {
      const date = new Date(value);
      if (Number.isNaN(date.getTime())) return value;
      return new Intl.DateTimeFormat(undefined, {
        timeZone: "America/New_York",
        hour: "numeric",
        minute: "2-digit",
      }).format(date);
    };
    return end ? `${parse(start)}–${parse(end)} ET` : start;
  }
</script>

<a class="back" href="/">← All shows</a>

{#if !stream}
  <div class="error">Unknown live station.</div>
{:else}
  <div class="showhead">
    <div>
      <div class="eyebrow"><span class="live-dot"></span> Live now</div>
      <h1>{stream.name}</h1>
      <div class="dj">{stream.tagline}</div>
      {#if detail?.current_show}
        <p class="now-label">Current show</p>
        <h2>{detail.current_show.name}</h2>
        {#if detail.current_show.host}<div class="host">with {detail.current_show.host}</div>{/if}
        {#if detail.current_show.description}<p class="blurb">{detail.current_show.description}</p>{/if}
        {#if detail.current_show.show_id}
          <a class="archive-link" href={"/show/" + detail.current_show.show_id}>Browse show archives →</a>
        {/if}
      {/if}
    </div>
    <div class="actions">
      <button class="primary" onclick={() => player.playLive(stream)}>
        <Icon name={player.live?.id === stream.id && player.playing ? "pause" : "play"} />
        {player.live?.id === stream.id && player.playing ? "Pause live" : "Listen live"}
      </button>
      <button class="ghost" onclick={() => load(true)} disabled={refreshing}>
        <Icon name="refresh" /> {refreshing ? "Refreshing…" : "Refresh"}
      </button>
    </div>
  </div>

  {#if error}
    <div class="error">{error}</div>
  {/if}
  {#if detail?.warning}
    <div class="warning">Showing the last available information. {detail.warning}</div>
  {/if}

  <section>
    <div class="section-head">
      <h2>Last 20 songs</h2>
      {#if detail}
        <span class="meta">
          {detail.history_source === "radio_rethink" ? "Live history" : "Local history"}
          · updated {playedTime(detail.updated_at)} ET
        </span>
      {/if}
    </div>
    {#if loading && !detail}
      <div class="tracks muted">Loading live history…</div>
    {:else if !detail?.tracks.length}
      <div class="tracks muted">No songs have been observed yet.</div>
    {:else}
      <div class="tracks">
        {#each detail.tracks as track (track.id)}
          <TrackRow
            {track}
            current={track.id === detail.current_track_id}
            playable={false}
            timeLabel={playedTime(track.played_at)}
            onfavourite={() => favourite(track)}
          />
        {/each}
      </div>
    {/if}
  </section>

  <section>
    <div class="section-head"><h2>Up next today</h2><span class="meta">Eastern Time</span></div>
    {#if loading && !detail}
      <div class="schedule muted">Loading schedule…</div>
    {:else if !detail?.upcoming_shows.length}
      <div class="schedule muted">No more scheduled shows are listed for today.</div>
    {:else}
      <div class="schedule">
        {#each detail.upcoming_shows as program, index (`${program.show_id ?? program.name}-${index}`)}
          <article class="program">
            <div class="program-time">{programTime(program)}</div>
            <div class="program-main">
              {#if program.show_id}
                <a class="program-name" href={"/show/" + program.show_id}>{program.name}</a>
              {:else}
                <span class="program-name">{program.name}</span>
              {/if}
              {#if program.host}<span class="host">with {program.host}</span>{/if}
              {#if program.description}<p>{program.description}</p>{/if}
            </div>
            {#if program.show_id}
              <a class="archive-button" href={"/show/" + program.show_id}>Archives</a>
            {/if}
          </article>
        {/each}
      </div>
    {/if}
  </section>
{/if}

<style>
  .back { display: inline-block; margin-bottom: 12px; color: var(--c-dim); }
  .showhead { display: flex; justify-content: space-between; align-items: flex-start; gap: 20px; margin-bottom: 18px; flex-wrap: wrap; }
  h1 { margin: 0 0 4px; font-size: 26px; }
  h2 { margin: 0; font-size: 17px; }
  .eyebrow, .now-label { color: var(--c-dim); font-size: 11px; font-weight: 700; letter-spacing: .08em; text-transform: uppercase; }
  .eyebrow { display: flex; align-items: center; gap: 7px; margin-bottom: 5px; }
  .now-label { margin: 18px 0 4px; }
  .live-dot { width: 8px; height: 8px; border-radius: 50%; background: var(--c-line); box-shadow: 0 0 0 4px color-mix(in srgb, var(--c-line) 15%, transparent); }
  .dj, .host { color: var(--c-accent); font-size: 14px; }
  .blurb { color: var(--c-dim); font-size: 14px; line-height: 1.5; margin: 8px 0; max-width: 720px; }
  .archive-link { color: var(--c-dim); font-size: 13px; text-decoration: underline; }
  .actions { display: flex; gap: 8px; flex-wrap: wrap; }
  button { display: inline-flex; align-items: center; gap: 7px; }
  .primary { background: var(--c-accent); color: var(--c-on-accent); border: none; border-radius: 8px; padding: 8px 16px; font-weight: 700; cursor: pointer; }
  .ghost, .archive-button { background: none; border: 1px solid var(--c-border); color: var(--c-dim); border-radius: 8px; padding: 7px 12px; cursor: pointer; text-decoration: none; }
  .ghost:hover, .archive-button:hover { border-color: var(--c-accent); color: var(--c-accent); }
  .ghost:disabled { opacity: .45; }
  .error, .warning { padding: 8px 12px; border-radius: 8px; margin-bottom: 12px; background: var(--c-surface2); }
  .error { border: 1px solid var(--c-danger); }
  .warning { border: 1px solid var(--c-border); color: var(--c-dim); font-size: 13px; }
  section { margin-top: 24px; }
  .section-head { display: flex; align-items: baseline; justify-content: space-between; gap: 12px; margin-bottom: 8px; }
  .meta, .muted { color: var(--c-dim); font-size: 12px; }
  .tracks, .schedule { background: var(--c-surface); border-radius: 8px; padding: 8px; }
  .tracks { display: flex; flex-direction: column; gap: 2px; }
  .program { display: grid; grid-template-columns: 130px 1fr auto; gap: 14px; align-items: start; padding: 11px 8px; border-bottom: 1px solid var(--c-border); }
  .program:last-child { border-bottom: 0; }
  .program-time { color: var(--c-dim); font-size: 12px; font-variant-numeric: tabular-nums; }
  .program-main { display: flex; min-width: 0; flex-direction: column; gap: 2px; }
  .program-name { color: var(--c-text); font-size: 14px; font-weight: 700; }
  .program-main p { color: var(--c-dim); font-size: 13px; line-height: 1.4; margin: 4px 0 0; }
  .archive-button { font-size: 12px; padding: 5px 9px; }
  @media (max-width: 700px) {
    .program { grid-template-columns: 1fr auto; }
    .program-time { grid-column: 1 / -1; }
  }
</style>
