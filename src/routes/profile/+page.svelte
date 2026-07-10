<script lang="ts">
  import {
    api,
    fmtHours,
    fmtTime,
    type Favourites,
    type Stats,
  } from "$lib/api";
  import { player } from "$lib/player.svelte";
  import { save } from "@tauri-apps/plugin-dialog";
  import ThemePicker from "$lib/ThemePicker.svelte";
  import Icon from "$lib/Icon.svelte";

  let favs = $state<Favourites | null>(null);
  let stats = $state<Stats | null>(null);
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let showBanner = $state(false);

  $effect(() => {
    showBanner = localStorage.getItem("ab2.hideLocalBanner") !== "1";
  });

  function dismissBanner() {
    showBanner = false;
    localStorage.setItem("ab2.hideLocalBanner", "1");
  }

  async function load() {
    error = null;
    try {
      [favs, stats] = await Promise.all([api.listFavourites(), api.getStats()]);
    } catch (e) {
      error = String(e);
    }
  }
  load();

  async function unfav(kind: "show" | "episode" | "track", refId: string) {
    try {
      await api.toggleFavourite(kind, refId);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function playFavEpisode(episodeId: number, showName: string, startSec: number | null = null) {
    try {
      const eps = favs?.episodes.find((f) => f.episode.id === episodeId)?.episode;
      if (eps) {
        await player.playEpisode(eps, showName, startSec);
        return;
      }
      // favourite track: episode not in favourites list — fetch via its show
      const t = favs?.tracks.find((f) => f.track.episode_id === episodeId);
      if (t) {
        const detail = await api.getShow(t.show_id);
        const ep = detail.episodes.find((e) => e.id === episodeId);
        if (ep) await player.playEpisode(ep, t.show_name, startSec);
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function exportCsv(kind: "favourites" | "listens" | "stats") {
    error = null;
    notice = null;
    try {
      const dest = await save({
        title: `Export ${kind} as CSV`,
        defaultPath: `wfmu-${kind}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!dest) return;
      notice = await api.exportCsv(kind, dest);
    } catch (e) {
      error = String(e);
    }
  }
</script>

<h1>Profile</h1>
{#if showBanner}
  <div class="banner">
    <span>Local only — favourites and listening history live in a SQLite file on this machine. No login, nothing leaves your computer.</span>
    <button class="banner-x" onclick={dismissBanner} title="Dismiss" aria-label="Dismiss">✕</button>
  </div>
{/if}

{#if error}<div class="error">{error}</div>{/if}
{#if notice}<div class="notice">{notice}</div>{/if}

<div class="export">
  <span>Export CSV:</span>
  <button class="ghost" onclick={() => exportCsv("favourites")}>Favourites</button>
  <button class="ghost" onclick={() => exportCsv("listens")}>Listening history</button>
  <button class="ghost" onclick={() => exportCsv("stats")}>Stats ranking</button>
</div>

<div class="cols">
  <section>
    <h2>Listening stats</h2>
    {#if stats}
      <div class="stat-tiles">
        <div class="stat">
          <div class="stat-num">{fmtHours(stats.total_seconds)}</div>
          <div class="stat-label">total listened</div>
        </div>
        <div class="stat">
          <div class="stat-num">{stats.total_sessions}</div>
          <div class="stat-label">sessions</div>
        </div>
        <div class="stat">
          <div class="stat-num">{stats.shows.length}</div>
          <div class="stat-label">shows heard</div>
        </div>
      </div>

      <h3>Audition ranking — shows</h3>
      {#if stats.shows.length === 0}
        <p class="muted">Nothing yet. Listen to something.</p>
      {:else}
        <ol class="rank">
          {#each stats.shows as s (s.show_id)}
            <li>
              <a href={"/show/" + s.show_id}>{s.show_name}</a>
              <span class="rank-meta">{fmtHours(s.seconds)} · {s.plays} session{s.plays === 1 ? "" : "s"}</span>
            </li>
          {/each}
        </ol>
      {/if}

      {#if stats.episodes.length}
        <h3>Most-listened episodes</h3>
        <ol class="rank">
          {#each stats.episodes.slice(0, 15) as e (e.episode_id)}
            <li>
              <span>{e.show_name} — {e.air_date ?? ""}{e.title ? ` · ${e.title}` : ""}</span>
              <span class="rank-meta">{fmtHours(e.seconds)}</span>
            </li>
          {/each}
        </ol>
      {/if}
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </section>

  <section>
    <h2>Favourites</h2>
    {#if favs}
      <h3>Shows ({favs.shows.length})</h3>
      {#each favs.shows as f (f.show.id)}
        <div class="fav">
          <a href={"/show/" + f.show.id}>{f.show.name}</a>
          {#if f.show.dj}<span class="muted">with {f.show.dj}</span>{/if}
          <button class="mini" onclick={() => unfav("show", f.show.id)} title="Remove"><Icon name="star" filled /></button>
        </div>
      {:else}
        <p class="muted">None yet — hover a show and hit the star.</p>
      {/each}

      <h3>Episodes ({favs.episodes.length})</h3>
      {#each favs.episodes as f (f.episode.id)}
        <div class="fav">
          <button
            class="linkish"
            onclick={() => playFavEpisode(f.episode.id, f.show_name)}
            disabled={!f.episode.has_audio}
            title="Play"
          >▶</button>
          <span>{f.show_name} — {f.episode.air_date ?? ""}</span>
          {#if f.episode.title}<span class="muted ellip">{f.episode.title}</span>{/if}
          <button class="mini" onclick={() => unfav("episode", String(f.episode.id))} title="Remove"><Icon name="star" filled /></button>
        </div>
      {:else}
        <p class="muted">None yet.</p>
      {/each}

      <h3>Songs ({favs.tracks.length})</h3>
      {#each favs.tracks as f (f.track.id)}
        <div class="fav">
          <button
            class="linkish"
            onclick={() => playFavEpisode(f.track.episode_id, f.show_name, f.track.start_sec)}
            title={f.track.start_sec !== null ? `Play at ${fmtTime(f.track.start_sec)}` : "Play episode"}
          >▶</button>
          <span class="ellip">
            <b>{f.track.artist ?? "?"}</b> — {f.track.title ?? "?"}
          </span>
          <span class="muted">{f.show_name} · {f.air_date ?? ""}</span>
          <button class="mini" onclick={() => unfav("track", String(f.track.id))} title="Remove"><Icon name="star" filled /></button>
        </div>
      {:else}
        <p class="muted">None yet — favourite songs from a playlist.</p>
      {/each}
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </section>
</div>

<details class="customize">
  <summary>
    <span class="cz-title">Customize</span>
    <span class="cz-hint">colour scheme &amp; theme</span>
  </summary>
  <p class="muted">Pick a colour scheme, or tweak any colour to make it yours. Saved locally.</p>
  <ThemePicker />
</details>

<style>
  h1 {
    margin: 0 0 4px;
    font-size: 22px;
  }
  h2 {
    font-size: 17px;
    border-bottom: 1px solid var(--c-surface2);
    padding-bottom: 6px;
  }
  h3 {
    font-size: 14px;
    color: var(--c-accent);
    margin: 18px 0 8px;
  }
  .muted {
    color: var(--c-dim);
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--c-surface2);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    padding: 10px 12px;
    margin: 10px 0 4px;
    color: var(--c-dim);
    font-size: 13px;
  }
  .banner span {
    flex: 1 1 auto;
  }
  .banner-x {
    flex: 0 0 auto;
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 14px;
    padding: 2px 6px;
    border-radius: 5px;
  }
  .banner-x:hover {
    color: var(--c-text);
    background: var(--c-surface2);
  }
  .error {
    background: var(--c-surface2);
    border: 1px solid var(--c-danger);
    padding: 8px 12px;
    border-radius: 8px;
    margin: 10px 0;
  }
  .notice {
    background: var(--c-surface2);
    border: 1px solid var(--c-accent);
    padding: 8px 12px;
    border-radius: 8px;
    margin: 10px 0;
  }
  .customize {
    margin: 28px 0 8px;
    border-top: 1px solid var(--c-surface2);
    padding-top: 14px;
  }
  .customize summary {
    cursor: pointer;
    list-style: none;
    display: flex;
    align-items: baseline;
    gap: 10px;
    user-select: none;
  }
  .customize summary::-webkit-details-marker {
    display: none;
  }
  .customize summary::before {
    content: "▸";
    color: var(--c-dim);
    font-size: 12px;
    transition: transform 0.15s;
  }
  .customize[open] summary::before {
    transform: rotate(90deg);
  }
  .cz-title {
    font-size: 17px;
    font-weight: 700;
  }
  .cz-hint {
    color: var(--c-dim);
    font-size: 12px;
  }
  .export {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 14px 0 6px;
    flex-wrap: wrap;
  }
  .ghost {
    background: none;
    border: 1px solid var(--c-border);
    color: var(--c-dim);
    border-radius: 8px;
    padding: 6px 12px;
    cursor: pointer;
  }
  .ghost:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }
  .cols {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 32px;
    margin-top: 10px;
  }
  @media (max-width: 900px) {
    .cols {
      grid-template-columns: 1fr;
    }
  }
  .stat-tiles {
    display: flex;
    gap: 12px;
    margin: 10px 0;
  }
  .stat {
    background: var(--c-surface);
    border-radius: 10px;
    padding: 14px 18px;
    min-width: 110px;
  }
  .stat-num {
    font-size: 22px;
    font-weight: 800;
    color: var(--c-accent);
  }
  .stat-label {
    font-size: 12px;
    color: var(--c-dim);
    margin-top: 2px;
  }
  .rank {
    margin: 6px 0;
    padding-left: 22px;
  }
  .rank li {
    padding: 4px 0;
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .rank-meta {
    color: var(--c-dim);
    font-size: 12px;
    white-space: nowrap;
  }
  .fav {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--c-surface);
    border-radius: 8px;
    padding: 7px 10px;
    margin-bottom: 4px;
    font-size: 14px;
  }
  .fav .mini {
    margin-left: auto;
  }
  .mini {
    background: none;
    border: none;
    color: var(--c-gold);
    cursor: pointer;
    font-size: 15px;
  }
  .mini:hover {
    color: var(--c-danger);
  }
  .linkish {
    background: var(--c-surface2);
    border: none;
    color: var(--c-text);
    border-radius: 50%;
    width: 26px;
    height: 26px;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .linkish:hover:not(:disabled) {
    background: var(--c-accent);
    color: var(--c-surface);
  }
  .linkish:disabled {
    opacity: 0.35;
  }
  .ellip {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
