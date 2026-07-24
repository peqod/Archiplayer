<script lang="ts">
  import { api, LIVE_STREAMS, type Show, type TrackHit } from "$lib/api";
  import { player, type QueueItem } from "$lib/player.svelte";
  import { goto } from "$app/navigation";
  import { page } from "$app/stores";
  import { onMount } from "svelte";
  import Icon from "$lib/Icon.svelte";
  import { selectRandomPlayback } from "$lib/random-show";
  import { LatestRequest } from "$lib/request-gate";
  import { shareShow } from "$lib/share";
  import { hasExactTrackTimestamp } from "$lib/track-playback";

  let shows = $state<Show[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let query = $state("");
  let trackHits = $state<TrackHit[]>([]);
  let letterFilter = $state<string | null>(null);
  let reverse = $state(false);
  let busyShow = $state<string | null>(null);
  let randomBusy = $state(false);
  const catalogRequests = new LatestRequest();
  const searchRequests = new LatestRequest();

  const PAGE_SIZE = 60;
  let visibleCount = $state(PAGE_SIZE);

  async function load(generation: number) {
    if (catalogRequests.isCurrent(generation)) error = null;
    try {
      const next = await api.getCatalog();
      if (catalogRequests.isCurrent(generation)) shows = next;
    } catch (e) {
      if (catalogRequests.isCurrent(generation)) error = String(e);
    } finally {
      if (catalogRequests.isCurrent(generation)) loading = false;
    }
  }

  // Seed the filter from URL params once, so the show-view CatalogNav bar can deep-link
  // back into a filtered catalog (?q= / ?letter= / ?fav=1). No params = home as usual.
  onMount(() => {
    const sp = $page.url.searchParams;
    const q = sp.get("q");
    const letter = sp.get("letter");
    if (q) query = q;
    else if (sp.get("fav") === "1") letterFilter = "★";
    else if (letter) letterFilter = letter;

    // The backend serves a fresh-enough cached catalog immediately and refreshes stale
    // data on demand. Ignore the completion if this route has already been destroyed.
    const generation = catalogRequests.begin();
    void load(generation);

    return () => catalogRequests.invalidate(generation);
  });

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
    if (reverse) list.reverse();
    return list;
  });

  const visible = $derived(filtered.slice(0, visibleCount));
  const favCount = $derived(shows.filter((s) => s.favourite).length);

  $effect(() => {
    // reset paging when the filter or order changes
    void query;
    void letterFilter;
    void reverse;
    visibleCount = PAGE_SIZE;
  });

  $effect(() => {
    const q = query.trim();
    const generation = searchRequests.begin();
    trackHits = [];
    if (!q) {
      return () => searchRequests.invalidate(generation);
    }

    const timer = setTimeout(async () => {
      try {
        const r = await api.search(q);
        if (searchRequests.isCurrent(generation)) trackHits = r.tracks;
      } catch {
        if (searchRequests.isCurrent(generation)) trackHits = [];
      }
    }, 250);

    return () => {
      clearTimeout(timer);
      searchRequests.invalidate(generation);
    };
  });

  async function launchShow(show: Show) {
    if (busyShow !== null) return;
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

  async function randomShow() {
    if (!shows.length || randomBusy) return;
    randomBusy = true;
    error = null;
    try {
      const selection = await selectRandomPlayback(
        shows,
        (show) => api.getShow(show.id),
        player.current?.episode.show_id ?? null,
        player.current?.episode.id ?? null,
      );
      if (!selection) {
        error = "No playable archives found in the catalog.";
        return;
      }

      const items: QueueItem[] = selection.episodes.map((episode) => ({
        episode,
        showName: selection.show.name,
      }));
      await goto("/show/" + selection.show.id, {
        state: { centerEpisodeId: selection.episodes[selection.index].id },
      });
      await player.playQueue(items, selection.index);
    } catch (err) {
      error = String(err);
    } finally {
      randomBusy = false;
    }
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

  function share(show: Show, e: MouseEvent) {
    e.preventDefault();
    e.stopPropagation();
    shareShow(show);
  }

  async function playTrackHit(hit: TrackHit) {
    if (!hasExactTrackTimestamp(hit.track.start_sec)) return;
    try {
      const detail = await api.getShow(hit.show_id);
      const ep = detail.episodes.find((e) => e.id === hit.track.episode_id);
      if (!ep || !ep.has_audio) {
        error = "That episode has no audio archive.";
        return;
      }
      await player.playEpisode(ep, hit.show_name, null, hit.track.start_sec);
    } catch (err) {
      error = String(err);
    }
  }

</script>

<div class="head">
  <h1>Shows</h1>
  <div class="search-wrap">
    <input
      class="search"
      type="search"
      aria-label="Search shows, DJs, and cached songs"
      placeholder="Search shows, DJs — and songs from cached playlists…"
      bind:value={query}
    />
    <button
      class="dice"
      onclick={randomShow}
      disabled={!shows.length || randomBusy}
      title="Play a random show and episode"
      aria-label="Play a random show and episode"
    >{randomBusy ? "…" : "🎲"}</button>
  </div>
</div>

{#if error}
  <div class="error" role="alert">{error} <button class="ghost" aria-label="Dismiss error" onclick={() => (error = null)}>✕</button></div>
{/if}

{#if !query}
  <div class="alpha" role="group" aria-label="Filter shows">
    <button
      class="fav-filter"
      class:on={letterFilter === "★"}
      aria-pressed={letterFilter === "★"}
      onclick={() => (letterFilter = letterFilter === "★" ? null : "★")}
      title="Show only favourited shows"
    >★ Favourites{favCount ? ` (${favCount})` : ""}</button>
    <span class="alpha-sep" aria-hidden="true"></span>
    <button aria-pressed={letterFilter === null} class:on={letterFilter === null} onclick={() => (letterFilter = null)}>All</button>
    {#each letters as l}
      <button aria-pressed={letterFilter === l} class:on={letterFilter === l} onclick={() => (letterFilter = letterFilter === l ? null : l)}>{l}</button>
    {/each}
    <span class="alpha-gap" aria-hidden="true"></span>
    <button
      class="rev"
      class:on={reverse}
      onclick={() => (reverse = !reverse)}
      aria-pressed={reverse}
      aria-label="Reverse sort order"
      title={reverse ? "Sort A→Z" : "Reverse order (Z→A)"}
    >⇅</button>
  </div>
{/if}

{#if !query}
  <div class="live">
    <span class="live-label"><span class="live-dot"></span> Listen Live Now</span>
    <div class="live-streams">
      {#each LIVE_STREAMS as s}
        <a
          class="live-card"
          class:on={player.live?.id === s.id}
          href={"/live/" + s.id}
          onclick={() => void player.playLive(s)}
          aria-label={(player.live?.id === s.id && player.playing ? "Pause " : "Play ") + s.name + " and open live details"}
          title={(player.live?.id === s.id && player.playing ? "Pause" : "Play") + " " + s.name + " and open live details"}
        >
          <span class="lc-play" aria-hidden="true">
            <Icon name={player.live?.id === s.id && player.playing ? "pause" : "play"} />
          </span>
          <span class="lc-text">
            <span class="lc-name ellipsis">{s.name}</span>
            <span class="lc-tag ellipsis">{s.tagline}</span>
          </span>
        </a>
      {/each}
    </div>
  </div>
{/if}

{#if loading}
  <p class="muted" role="status">Loading catalog{shows.length === 0 ? " (first run scrapes wfmu.org — a few seconds)" : ""}…</p>
{:else if query && trackHits.length}
  <h2 class="subhead">Songs <span class="muted">({trackHits.length} from cached playlists)</span></h2>
  <div class="tracklist">
    {#each trackHits.slice(0, 50) as hit}
      <button
        class="trackhit"
        onclick={() => playTrackHit(hit)}
        disabled={!hasExactTrackTimestamp(hit.track.start_sec)}
        title={hasExactTrackTimestamp(hit.track.start_sec)
          ? "Play episode at this song"
          : "No timestamp available"}
        aria-label={hasExactTrackTimestamp(hit.track.start_sec)
          ? `Play ${hit.track.artist ?? "unknown artist"} — ${hit.track.title ?? "unknown song"}`
          : `Exact-song playback unavailable for ${hit.track.artist ?? "unknown artist"} — ${hit.track.title ?? "unknown song"}: no timestamp`}
      >
        <span class="t-artist ellipsis">{hit.track.artist ?? "?"}</span>
        <span class="t-title ellipsis">{hit.track.title ?? "?"}</span>
        <span class="t-meta ellipsis">
          {hit.show_name} · {hit.air_date ?? ""}
          {#if !hasExactTrackTimestamp(hit.track.start_sec)} · No timestamp{/if}
        </span>
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
          <span class="row-name ellipsis">
            {show.name}
            {#if show.favourite}<span class="row-fav"><Icon name="star" filled size="0.85em" /></span>{/if}
          </span>
          <span class="row-sub ellipsis">
            {#if show.dj}{show.dj}{/if}
            {#if show.dj && !show.on_air}<span class="dot">·</span>{/if}
            {#if !show.on_air}<span class="row-off">archive</span>{/if}
          </span>
          {#if show.description}<span class="row-desc">{show.description}</span>{/if}
        </span>
      </a>
      <div class="row-actions">
        <button class="rbtn play" onclick={(e) => playAll(show, e)} disabled={busyShow !== null} title="Play all archives, oldest first">
          {#if busyShow === show.id}…{:else}<Icon name="play" /> Play all{/if}
        </button>
        <button
          class="rbtn star"
          class:on={show.favourite}
          onclick={(e) => toggleFav(show, e)}
          aria-label={show.favourite ? `Remove ${show.name} from favourites` : `Add ${show.name} to favourites`}
          aria-pressed={show.favourite}
          title="Favourite"
        >
          <Icon name="star" filled={show.favourite} />
        </button>
        <button class="rbtn share" onclick={(e) => share(show, e)} aria-label={`Share ${show.name}`} title="Share">
          <Icon name="share" />
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
  .search-wrap {
    position: relative;
    flex: 1 1 auto;
    max-width: 520px;
  }
  .search {
    width: 100%;
    display: block;
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    padding: 8px 42px 8px 12px; /* right pad reserves room for the inset dice */
    border-radius: 8px;
    font-size: 14px;
  }
  .search:focus {
    outline: none;
    border-color: var(--c-accent);
  }
  /* Drop the WebView native clear (✕) so it can't collide with the inset dice. */
  .search::-webkit-search-cancel-button {
    -webkit-appearance: none;
    appearance: none;
  }
  /* Random-show trigger, seated in the search bar's right corner. */
  .dice {
    position: absolute;
    top: 50%;
    right: 4px;
    transform: translateY(-50%);
    background: none;
    border: none;
    border-radius: 6px;
    padding: 4px 6px;
    font-size: 16px;
    line-height: 1;
    cursor: pointer;
    color: var(--c-dim);
  }
  .dice:hover:not(:disabled) {
    background: var(--c-surface2);
  }
  .dice:disabled {
    cursor: default;
    opacity: 0.6;
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
  .live {
    margin-bottom: 18px;
  }
  .live-label {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--c-dim);
    margin-bottom: 8px;
  }
  .live-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c-line);
    animation: live-pulse 2s infinite;
  }
  @keyframes live-pulse {
    0% { box-shadow: 0 0 0 0 rgba(216, 72, 63, 0.5); }
    70% { box-shadow: 0 0 0 6px rgba(216, 72, 63, 0); }
    100% { box-shadow: 0 0 0 0 rgba(216, 72, 63, 0); }
  }
  .live-streams {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 8px;
  }
  .live-card {
    display: flex;
    align-items: center;
    gap: 10px;
    text-align: left;
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 10px;
    padding: 9px 12px;
    color: var(--c-text);
    text-decoration: none;
    cursor: pointer;
  }
  .live-card:hover,
  .live-card:focus-within {
    border-color: var(--c-accent);
  }
  .live-card.on {
    border-color: var(--c-accent);
    background: var(--c-surface2);
  }
  .lc-play {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 32px;
    height: 32px;
    border-radius: 50%;
    background: var(--c-accent);
    color: var(--c-on-accent);
    flex: 0 0 auto;
  }
  .lc-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
    flex: 1 1 auto;
  }
  .lc-name {
    font-weight: 700;
    font-size: 14px;
  }
  .lc-tag {
    font-size: 12px;
    color: var(--c-dim);
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
  /* 16px breather between the letters and the reverse-order toggle. */
  .alpha-gap {
    width: 16px;
    flex: 0 0 auto;
  }
  .rev {
    font-size: 15px;
    line-height: 1;
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
  }
  .row-fav {
    color: var(--c-gold);
    font-size: 12px;
    margin-left: 4px;
  }
  .row-sub {
    color: var(--c-dim);
    font-size: 12px;
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
  .rbtn.share {
    display: inline-flex;
    align-items: center;
  }
  .rbtn:hover {
    filter: brightness(1.1);
  }
  .rbtn:disabled {
    cursor: wait;
    opacity: 0.55;
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
  .trackhit:hover:not(:disabled) {
    background: var(--c-surface2);
  }
  .trackhit:disabled {
    cursor: not-allowed;
    opacity: 0.65;
  }
  .t-artist {
    font-weight: 600;
  }
  .t-meta {
    color: var(--c-dim);
    text-align: right;
  }
  @media (hover: none) {
    .row-actions {
      opacity: 1;
    }
  }
  @media (max-width: 760px) {
    .trackhit {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
      grid-template-areas:
        "artist title"
        "meta meta";
      row-gap: 3px;
    }
    .t-artist {
      grid-area: artist;
    }
    .t-title {
      grid-area: title;
    }
    .t-meta {
      grid-area: meta;
      text-align: left;
    }
  }
  @media (max-width: 520px) {
    .head {
      align-items: stretch;
      flex-direction: column;
      gap: 8px;
    }
    .row {
      flex-wrap: wrap;
    }
    .row-main {
      flex-basis: 100%;
    }
    .row-actions {
      width: 100%;
      justify-content: flex-end;
      opacity: 1;
    }
  }
</style>
