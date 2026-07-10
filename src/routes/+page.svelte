<script lang="ts">
  import { api, type Show, type TrackHit } from "$lib/api";
  import { player, type QueueItem } from "$lib/player.svelte";
  import { goto } from "$app/navigation";
  import Icon from "$lib/Icon.svelte";

  let shows = $state<Show[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let query = $state("");
  let trackHits = $state<TrackHit[]>([]);
  let letterFilter = $state<string | null>(null);
  let busyShow = $state<string | null>(null);
  let searchTimer: ReturnType<typeof setTimeout> | undefined;

  const PAGE_SIZE = 60;
  let visibleCount = $state(PAGE_SIZE);

  async function load(refresh = false) {
    error = null;
    if (refresh) refreshing = true;
    try {
      shows = await api.getCatalog(refresh);
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
      refreshing = false;
    }
  }
  load();

  const letters = $derived.by(() => {
    const set = new Set<string>();
    for (const s of shows) {
      const c = s.name.replace(/^the\s+/i, "").charAt(0).toUpperCase();
      set.add(/[A-Z]/.test(c) ? c : "#");
    }
    return [...set].sort((a, b) => (a === "#" ? 1 : b === "#" ? -1 : a.localeCompare(b)));
  });

  function sortKey(s: Show): string {
    return s.name.replace(/^the\s+/i, "").toUpperCase();
  }

  const filtered = $derived.by(() => {
    let list = [...shows].sort((a, b) => sortKey(a).localeCompare(sortKey(b)));
    const q = query.trim().toLowerCase();
    if (q) {
      list = list.filter(
        (s) =>
          s.name.toLowerCase().includes(q) ||
          (s.dj ?? "").toLowerCase().includes(q),
      );
    } else if (letterFilter === "★") {
      list = list.filter((s) => s.favourite);
    } else if (letterFilter) {
      list = list.filter((s) => {
        const c = sortKey(s).charAt(0);
        return letterFilter === "#" ? !/[A-Z]/.test(c) : c === letterFilter;
      });
    }
    return list;
  });

  const visible = $derived(filtered.slice(0, visibleCount));
  const favCount = $derived(shows.filter((s) => s.favourite).length);

  $effect(() => {
    // reset paging when the filter changes
    void query;
    void letterFilter;
    visibleCount = PAGE_SIZE;
  });

  $effect(() => {
    const q = query.trim();
    clearTimeout(searchTimer);
    if (!q) {
      trackHits = [];
      return;
    }
    searchTimer = setTimeout(async () => {
      try {
        const r = await api.search(q);
        trackHits = r.tracks;
      } catch {
        trackHits = [];
      }
    }, 250);
  });

  async function launchShow(show: Show) {
    busyShow = show.id;
    try {
      const detail = await api.getShow(show.id);
      const items: QueueItem[] = detail.episodes
        .filter((ep) => ep.has_audio)
        .reverse() // site lists newest first; play chronologically
        .map((ep) => ({ episode: ep, showName: show.name }));
      if (!items.length) {
        error = `${show.name}: no playable archives found.`;
        return;
      }
      await player.playQueue(items);
    } catch (err) {
      error = String(err);
    } finally {
      busyShow = null;
    }
  }

  async function playAll(show: Show, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    await launchShow(show);
  }

  // Pick a show from the current instant: scramble the timestamp's digits into a
  // seed, then index into the catalog. Different every click, derived from "now".
  async function randomShow() {
    if (!shows.length) return;
    const stamp = String(Date.now()).split("").reverse().join("");
    let seed = 0;
    for (const ch of stamp) seed = (seed * 31 + ch.charCodeAt(0)) >>> 0;
    const show = shows[seed % shows.length];
    goto("/show/" + show.id);
    launchShow(show);
  }

  async function toggleFav(show: Show, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    try {
      const fav = await api.toggleFavourite("show", show.id);
      shows = shows.map((s) => (s.id === show.id ? { ...s, favourite: fav } : s));
    } catch (err) {
      error = String(err);
    }
  }

  async function playTrackHit(hit: TrackHit) {
    try {
      const detail = await api.getShow(hit.show_id);
      const ep = detail.episodes.find((e) => e.id === hit.track.episode_id);
      if (!ep || !ep.has_audio) {
        error = "That episode has no audio archive.";
        return;
      }
      await player.playEpisode(ep, hit.show_name, hit.track.start_sec);
    } catch (err) {
      error = String(err);
    }
  }
</script>

<div class="head">
  <h1>Shows</h1>
  <input
    class="search"
    type="search"
    placeholder="Search shows, DJs — and songs from cached playlists…"
    bind:value={query}
  />
  <button class="ghost" onclick={randomShow} disabled={!shows.length} title="Play a random show">
    🎲 Random
  </button>
  <button class="ghost" onclick={() => load(true)} disabled={refreshing}>
    {refreshing ? "Refreshing…" : "↻ Refresh catalog"}
  </button>
</div>

{#if error}
  <div class="error">{error} <button class="ghost" onclick={() => (error = null)}>✕</button></div>
{/if}

{#if !query}
  <div class="alpha">
    <button
      class="fav-filter"
      class:on={letterFilter === "★"}
      onclick={() => (letterFilter = letterFilter === "★" ? null : "★")}
      title="Show only favourited shows"
    >★ Favourites{favCount ? ` (${favCount})` : ""}</button>
    <span class="alpha-sep"></span>
    <button class:on={letterFilter === null} onclick={() => (letterFilter = null)}>All</button>
    {#each letters as l}
      <button class:on={letterFilter === l} onclick={() => (letterFilter = letterFilter === l ? null : l)}>{l}</button>
    {/each}
  </div>
{/if}

{#if loading}
  <p class="muted">Loading catalog{shows.length === 0 ? " (first run scrapes wfmu.org — a few seconds)" : ""}…</p>
{:else if query && trackHits.length}
  <h2 class="subhead">Songs <span class="muted">({trackHits.length} from cached playlists)</span></h2>
  <div class="tracklist">
    {#each trackHits.slice(0, 50) as hit}
      <button class="trackhit" onclick={() => playTrackHit(hit)} title="Play episode at this song">
        <span class="t-artist">{hit.track.artist ?? "?"}</span>
        <span class="t-title">{hit.track.title ?? "?"}</span>
        <span class="t-meta">{hit.show_name} · {hit.air_date ?? ""}</span>
      </button>
    {/each}
  </div>
  <h2 class="subhead">Shows</h2>
{/if}

<div class="list">
  {#each visible as show (show.id)}
    <div class="row">
      <a class="row-main" href={"/show/" + show.id}>
        <span class="row-text">
          <span class="row-name">
            {show.name}
            {#if show.favourite}<span class="row-fav"><Icon name="star" filled size="0.85em" /></span>{/if}
          </span>
          <span class="row-sub">
            {#if show.dj}{show.dj}{/if}
            {#if show.dj && !show.on_air}<span class="dot">·</span>{/if}
            {#if !show.on_air}<span class="row-off">archive</span>{/if}
          </span>
          {#if show.description}<span class="row-desc">{show.description}</span>{/if}
        </span>
      </a>
      <div class="row-actions">
        <button class="rbtn play" onclick={(e) => playAll(show, e)} title="Play all archives, oldest first">
          {busyShow === show.id ? "…" : "▶ Play all"}
        </button>
        <button class="rbtn star" class:on={show.favourite} onclick={(e) => toggleFav(show, e)} title="Favourite">
          <Icon name="star" filled={show.favourite} />
        </button>
      </div>
    </div>
  {/each}
</div>

{#if visible.length < filtered.length}
  <div class="more">
    <button class="ghost" onclick={() => (visibleCount += PAGE_SIZE)}>
      Show more ({filtered.length - visible.length} left)
    </button>
  </div>
{/if}
{#if !loading && !filtered.length}
  {#if letterFilter === "★"}
    <p class="muted">No favourited shows yet — hover a show and hit ☆ to add one.</p>
  {:else}
    <p class="muted">No shows match.</p>
  {/if}
{/if}

<style>
  .head {
    display: flex;
    align-items: center;
    gap: 16px;
    margin-bottom: 12px;
  }
  h1 {
    margin: 0;
    font-size: 22px;
  }
  .subhead {
    font-size: 16px;
    margin: 18px 0 8px;
  }
  .search {
    flex: 1 1 auto;
    max-width: 520px;
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    padding: 8px 12px;
    border-radius: 8px;
    font-size: 14px;
  }
  .search:focus {
    outline: none;
    border-color: var(--c-accent);
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
  .error {
    background: var(--c-surface2);
    border: 1px solid var(--c-danger);
    padding: 8px 12px;
    border-radius: 8px;
    margin-bottom: 12px;
  }
  .alpha {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-bottom: 14px;
  }
  .alpha button {
    background: var(--c-surface);
    color: var(--c-dim);
    border: none;
    border-radius: 6px;
    min-width: 28px;
    padding: 4px 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .alpha button.on {
    background: var(--c-accent);
    color: var(--c-surface);
    font-weight: 700;
  }
  .fav-filter {
    color: var(--c-gold) !important;
    font-weight: 600;
    white-space: nowrap;
  }
  .fav-filter.on {
    background: var(--c-gold) !important;
    color: var(--c-surface) !important;
  }
  .alpha-sep {
    width: 1px;
    align-self: stretch;
    background: var(--c-border);
    margin: 0 4px;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--c-surface);
    border-radius: 8px;
    padding: 6px 10px 6px 6px;
  }
  .row:hover {
    background: var(--c-surface2);
  }
  .row-main {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 12px;
    color: inherit;
    text-decoration: none;
  }
  .row-main:hover {
    text-decoration: none;
  }
  .row-text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .row-name {
    font-weight: 600;
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-fav {
    color: var(--c-gold);
    font-size: 12px;
    margin-left: 4px;
  }
  .row-sub {
    color: var(--c-dim);
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .row-desc {
    color: var(--c-dim);
    font-size: 12px;
    line-height: 1.35;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .dot {
    margin: 0 5px;
  }
  .row-off {
    text-transform: uppercase;
    letter-spacing: 0.06em;
    font-size: 10px;
  }
  .row-actions {
    flex: 0 0 auto;
    display: flex;
    gap: 6px;
    opacity: 0;
    transition: opacity 0.12s;
  }
  .row:hover .row-actions,
  .row:focus-within .row-actions {
    opacity: 1;
  }
  .rbtn {
    background: var(--c-surface2);
    color: var(--c-text);
    border: none;
    border-radius: 16px;
    padding: 6px 12px;
    font-weight: 600;
    cursor: pointer;
    font-size: 13px;
  }
  .rbtn.play {
    background: var(--c-accent);
    color: var(--c-on-accent);
    font-weight: 700;
  }
  .rbtn.star {
    display: inline-flex;
    align-items: center;
  }
  .rbtn.star.on {
    color: var(--c-gold);
  }
  .rbtn:hover {
    filter: brightness(1.1);
  }
  .more {
    text-align: center;
    margin: 18px 0;
  }
  .muted {
    color: var(--c-dim);
  }
  .tracklist {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .trackhit {
    display: grid;
    grid-template-columns: 220px 1fr 260px;
    gap: 12px;
    text-align: left;
    background: var(--c-surface);
    border: none;
    color: var(--c-text);
    padding: 8px 12px;
    border-radius: 6px;
    cursor: pointer;
    font-size: 13px;
  }
  .trackhit:hover {
    background: var(--c-surface2);
  }
  .t-artist {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .t-meta {
    color: var(--c-dim);
    text-align: right;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
