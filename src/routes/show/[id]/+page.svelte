<script lang="ts">
  import { page } from "$app/stores";
  import {
    api,
    type Episode,
    type Show,
    type Track,
  } from "$lib/api";
  import { player, type QueueItem } from "$lib/player.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "$lib/Icon.svelte";
  import { shareShow, shareEpisode, shareTrack, wfmuEpisodeUrl } from "$lib/share";
  import TrackRow from "$lib/TrackRow.svelte";
  import CatalogNav from "$lib/CatalogNav.svelte";
  import { centerEpisodeRow } from "$lib/episode-scroll";
  import { LatestRequest } from "$lib/request-gate";
  import { onMount, tick } from "svelte";

  function openWfmu(e: MouseEvent) {
    e.preventDefault();
    if (show) openUrl("https://wfmu.org/playlists/" + show.id).catch(() => {});
  }

  let show = $state<Show | null>(null);
  let episodes = $state<Episode[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let expanded = $state<Record<number, Track[] | "loading">>({});
  let downloading = $state<Record<number, { bytes: number; total: number }>>({});
  let episodeListEl = $state<HTMLElement | null>(null);
  let centeredEpisodeId: number | null = null;
  let reverse = $state(false);
  const detailRequests = new LatestRequest();
  const playlistRequests = new Map<number, LatestRequest>();

  const showId = $derived($page.params.id ?? "");

  // Displayed episode order. WFMU lists newest first; the reverse toggle flips to
  // oldest first. Queue building and Play-all follow this so playback matches the view.
  const orderedEpisodes = $derived(reverse ? [...episodes].reverse() : episodes);

  // The playing/paused episode, but only if it belongs to the show being viewed —
  // returning here (e.g. from search) should recentre the list on the playhead.
  function playheadEpisodeId(): number | null {
    const ep = player.current?.episode;
    return ep && ep.show_id === showId ? ep.id : null;
  }

  async function centerRequestedEpisode() {
    // Explicit request (random-show nav) wins; otherwise fall back to the playhead.
    const episodeId = $page.state.centerEpisodeId ?? playheadEpisodeId();
    if (!episodeId || centeredEpisodeId === episodeId) return;

    await tick();
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (centerEpisodeRow(episodeListEl, episodeId, reducedMotion)) {
      centeredEpisodeId = episodeId;
    }
  }

  // Crossing a responsive breakpoint restacks the player header and reflows the
  // episode rows (grid at <=760px), changing every row height. The scroll offset
  // that had the playhead centred now points elsewhere, so recompute it. Same
  // episode, so this skips the centerRequestedEpisode guard on purpose.
  function recenterPlayhead() {
    const episodeId = playheadEpisodeId();
    if (!episodeId) return;
    // Recompute after the reflow flushes so scrollIntoView reads the new heights.
    requestAnimationFrame(() => {
      const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      centerEpisodeRow(episodeListEl, episodeId, reducedMotion);
    });
  }

  async function load(requestedShowId: string, generation: number) {
    if (detailRequests.isCurrent(generation)) error = null;
    try {
      const detail = await api.getShow(requestedShowId);
      if (!detailRequests.isCurrent(generation)) return;
      show = detail.show;
      episodes = detail.episodes;
    } catch (e) {
      if (detailRequests.isCurrent(generation)) error = String(e);
    } finally {
      if (detailRequests.isCurrent(generation)) loading = false;
    }
    if (detailRequests.isCurrent(generation)) await centerRequestedEpisode();
  }

  $effect(() => {
    const requestedShowId = showId;
    const generation = detailRequests.begin();
    for (const request of playlistRequests.values()) request.invalidate();
    playlistRequests.clear();
    show = null;
    episodes = [];
    expanded = {};
    centeredEpisodeId = null;
    reverse = false; // each show opens newest-first
    loading = true;
    // The backend serves cached history while applying its freshness policy, avoiding
    // a second immediate show scrape and repeated archive-year walks.
    void load(requestedShowId, generation);

    return () => detailRequests.invalidate(generation);
  });

  onMount(() => {
    const un = listen<{
      episode_id: number;
      bytes: number;
      total: number;
      status: string;
    }>("download-progress", (e) => {
      const p = e.payload;
      if (p.status === "downloading") {
        downloading[p.episode_id] = { bytes: p.bytes, total: p.total };
      } else {
        delete downloading[p.episode_id];
        if (p.status === "done") {
          episodes = episodes.map((ep) =>
            ep.id === p.episode_id ? { ...ep, downloaded: true } : ep,
          );
        }
      }
    });
    // Recentre the playhead whenever a layout breakpoint flips under a resize.
    const breakpoints = ["(max-width: 760px)", "(max-width: 420px)"].map((q) =>
      window.matchMedia(q),
    );
    const onBreakpoint = () => recenterPlayhead();
    for (const mq of breakpoints) mq.addEventListener("change", onBreakpoint);

    return () => {
      un.then((f) => f()).catch(() => {});
      for (const mq of breakpoints) mq.removeEventListener("change", onBreakpoint);
    };
  });

  async function togglePlaylist(ep: Episode) {
    if (expanded[ep.id]) {
      playlistRequests.get(ep.id)?.invalidate();
      playlistRequests.delete(ep.id);
      const { [ep.id]: _, ...rest } = expanded;
      expanded = rest;
      return;
    }
    const requestedShowId = showId;
    const request = new LatestRequest();
    const generation = request.begin();
    playlistRequests.set(ep.id, request);
    expanded = { ...expanded, [ep.id]: "loading" };
    try {
      const tracks = await api.getPlaylist(ep.id);
      if (
        playlistRequests.get(ep.id) !== request ||
        !request.isCurrent(generation) ||
        showId !== requestedShowId
      )
        return;
      expanded = { ...expanded, [ep.id]: tracks };
      episodes = episodes.map((e) =>
        e.id === ep.id ? { ...e, track_count: tracks.length } : e,
      );
    } catch (e) {
      if (
        playlistRequests.get(ep.id) !== request ||
        !request.isCurrent(generation) ||
        showId !== requestedShowId
      )
        return;
      error = String(e);
      const { [ep.id]: _, ...rest } = expanded;
      expanded = rest;
    } finally {
      if (playlistRequests.get(ep.id) === request) playlistRequests.delete(ep.id);
    }
  }

  // Play `ep` inside a queue of the whole show (oldest → newest) so the transport's
  // prev/next-episode buttons can walk the show. Falls back to a lone-episode queue.
  async function queueShowAt(
    ep: Episode,
    startSec: number | null,
    startTrackSec: number | null = null,
  ) {
    if (!show) return;
    const list = orderedEpisodes.filter((e) => e.has_audio); // follows displayed order
    const idx = list.findIndex((e) => e.id === ep.id);
    if (idx < 0) {
      await player.playEpisode(ep, show.name, startSec, startTrackSec);
      return;
    }
    const items: QueueItem[] = list.map((e) => ({ episode: e, showName: show!.name }));
    await player.playQueue(items, idx, startSec, startTrackSec);
  }

  async function playEpisode(ep: Episode) {
    if (!show) return;
    if (player.current?.episode.id === ep.id) {
      player.toggle();
      return;
    }
    try {
      // Resume where the listener left off (unless finished — then start over).
      const resume =
        !ep.completed && ep.resume_sec && ep.resume_sec > 5 ? ep.resume_sec : null;
      await queueShowAt(ep, resume);
    } catch (e) {
      error = String(e);
    }
  }

  // Fraction (0–1) an episode has been listened to, for the row's progress bar.
  function progressFrac(ep: Episode): number {
    if (player.current?.episode.id === ep.id && player.duration > 0)
      return Math.min(player.currentTime / player.duration, 1);
    if (ep.completed) return 1;
    if (ep.resume_sec && ep.duration_sec && ep.duration_sec > 0)
      return Math.min(ep.resume_sec / ep.duration_sec, 1);
    return 0;
  }

  async function playFromHere(ep: Episode) {
    // queue: this episode then everything below it in the displayed order
    if (!show) return;
    const list = orderedEpisodes.filter((e) => e.has_audio); // follows displayed order
    const idx = list.findIndex((e) => e.id === ep.id);
    if (idx < 0) return;
    const items: QueueItem[] = list
      .slice(idx)
      .map((e) => ({ episode: e, showName: show!.name }));
    await player.playQueue(items);
  }

  async function playTrack(ep: Episode, track: Track) {
    if (!show) return;
    if (player.current?.episode.id === ep.id) {
      const activeTrack =
        player.currentTrackIndex >= 0
          ? player.tracks[player.currentTrackIndex]
          : null;
      if (activeTrack?.id === track.id) {
        player.toggle();
        return;
      }
      player.seekToTrack(track);
      return;
    }
    await queueShowAt(ep, null, track.start_sec);
  }

  async function favEpisode(ep: Episode) {
    try {
      const fav = await api.toggleFavourite("episode", String(ep.id));
      episodes = episodes.map((e) => (e.id === ep.id ? { ...e, favourite: fav } : e));
    } catch (e) {
      error = String(e);
    }
  }

  async function favTrack(ep: Episode, track: Track) {
    try {
      const fav = await api.toggleFavourite("track", String(track.id));
      const cur = expanded[ep.id];
      if (Array.isArray(cur)) {
        expanded = {
          ...expanded,
          [ep.id]: cur.map((t) => (t.id === track.id ? { ...t, favourite: fav } : t)),
        };
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function favShow() {
    if (!show) return;
    try {
      const fav = await api.toggleFavourite("show", show.id);
      show = { ...show, favourite: fav };
    } catch (e) {
      error = String(e);
    }
  }

  async function download(ep: Episode) {
    downloading[ep.id] = { bytes: 0, total: 0 };
    try {
      await api.downloadEpisode(ep.id);
    } catch (e) {
      error = String(e);
      delete downloading[ep.id];
    }
  }

  async function playAll() {
    if (!show) return;
    const items: QueueItem[] = orderedEpisodes
      .filter((e) => e.has_audio) // follows displayed order (reverse = oldest first)
      .map((e) => ({ episode: e, showName: show!.name }));
    if (items.length) await player.playQueue(items);
  }

  const playableCount = $derived(episodes.filter((e) => e.has_audio).length);

  function tracksOf(ep: Episode): Track[] {
    const v = expanded[ep.id];
    return Array.isArray(v) ? v : [];
  }
</script>

<CatalogNav bind:reverse />

<a href="/" class="back">← All shows</a>

{#if error}
  <div class="error" role="alert">{error} <button class="ghost" aria-label="Dismiss error" onclick={() => (error = null)}>✕</button></div>
{/if}

{#if loading}
  <p class="muted" role="status">Loading episodes{episodes.length === 0 ? " (scraping show page on first visit)" : ""}…</p>
{:else if show}
  <div class="showhead">
    <div>
      <h1>{show.name}</h1>
      {#if show.dj}<div class="dj">with {show.dj}</div>{/if}
      {#if show.description}<p class="blurb">{show.description}</p>{/if}
      <div class="meta">
        {episodes.length} episodes · {playableCount} with audio
        <span class="dot">·</span>
        <a class="wfmu-link" href={"https://wfmu.org/playlists/" + show.id} onclick={openWfmu}>View on WFMU ↗</a>
      </div>
    </div>
    <div class="actions">
      <button class="primary" onclick={playAll} disabled={!playableCount}><Icon name="play" /> Play all ({reverse ? "oldest" : "newest"} first)</button>
      <button class="ghost fav" class:on={show.favourite} onclick={favShow}>
        <Icon name="star" filled={show.favourite} /> {show.favourite ? "Favourited" : "Favourite"}
      </button>
      <button class="ghost share" onclick={() => show && shareShow(show)}>
        <Icon name="share" /> Share
      </button>
    </div>
  </div>

  <div class="eplist" bind:this={episodeListEl}>
    {#each orderedEpisodes as ep (ep.id)}
      <div
        class="ep"
        class:current={player.current?.episode.id === ep.id}
        data-episode-id={ep.id}
        aria-current={player.current?.episode.id === ep.id ? "true" : undefined}
      >
        <div class="ep-row">
          {#if progressFrac(ep) > 0}
            <div
              class="ep-prog"
              class:done={ep.completed}
              style="width:{progressFrac(ep) * 100}%"
              aria-hidden="true"
            ></div>
          {/if}
          <button
            class="playbtn"
            class:playing={player.current?.episode.id === ep.id && player.playing}
            onclick={() => playEpisode(ep)}
            disabled={!ep.has_audio}
            title={!ep.has_audio
              ? "No audio archive"
              : player.current?.episode.id === ep.id
                ? player.playing ? "Pause episode" : "Resume episode"
                : "Play this episode"}
            aria-label={!ep.has_audio
              ? "No audio archive"
              : player.current?.episode.id === ep.id
                ? player.playing ? "Pause episode" : "Resume episode"
                : "Play this episode"}
          >
            {#if ep.has_audio}
              <Icon
                name={player.current?.episode.id === ep.id && player.playing ? "pause" : "play"}
              />
            {:else}–{/if}
          </button>
          <button
            type="button"
            class="ep-main"
            onclick={() => togglePlaylist(ep)}
            aria-expanded={Boolean(expanded[ep.id])}
            aria-controls={`playlist-${ep.id}`}
          >
            <span class="ep-date">{ep.air_date ?? "unknown date"}</span>
            {#if ep.title}<span class="ep-title">{ep.title}</span>{/if}
            {#if ep.completed}<span class="badge done">completed</span>{/if}
            {#if !ep.has_audio}<span class="badge">playlist only</span>{/if}
            {#if ep.downloaded}<span class="badge dl">offline</span>{/if}
            {#if downloading[ep.id]}
              <span class="badge prog">
                ↓ {Math.round((downloading[ep.id].bytes / Math.max(downloading[ep.id].total, 1)) * 100)}%
              </span>
            {/if}
          </button>
          <div class="ep-actions">
            <button class="mini" onclick={() => playFromHere(ep)} disabled={!ep.has_audio} aria-label="Play from this episode onward" title="Play from this episode onward"><Icon name="next" /></button>
            <button class="mini" class:on={ep.favourite} onclick={() => favEpisode(ep)} aria-label={ep.favourite ? "Remove episode from favourites" : "Save episode"} aria-pressed={ep.favourite} title="Save episode"><Icon name="save" filled={ep.favourite} /></button>
            <button class="mini" onclick={() => shareEpisode(show?.name ?? "", ep)} aria-label="Share episode" title="Share episode"><Icon name="share" /></button>
            <button
              class="mini"
              class:on={ep.downloaded}
              onclick={() => download(ep)}
              disabled={!ep.has_audio || ep.downloaded || !!downloading[ep.id]}
              title={ep.downloaded ? "Already downloaded" : "Download for offline"}
              aria-label={ep.downloaded ? "Already downloaded" : "Download episode for offline playback"}
            ><Icon name="download" /></button>
            <button
              class="mini"
              onclick={() => togglePlaylist(ep)}
              title={expanded[ep.id] ? "Hide playlist" : "Show playlist"}
              aria-label={expanded[ep.id] ? "Hide playlist" : "Show playlist"}
              aria-expanded={Boolean(expanded[ep.id])}
              aria-controls={`playlist-${ep.id}`}
            >
              {expanded[ep.id] ? "▴" : "▾"}
            </button>
          </div>
        </div>
        {#if expanded[ep.id] === "loading"}
          <div id={`playlist-${ep.id}`} class="tracks muted" role="status">Loading playlist…</div>
        {:else if Array.isArray(expanded[ep.id])}
          {@const tracks = tracksOf(ep)}
          {#if tracks.length === 0}
            <div id={`playlist-${ep.id}`} class="tracks muted">No playlist recorded for this episode.</div>
          {:else}
            <div id={`playlist-${ep.id}`} class="tracks">
              {#each tracks as t (t.id)}
                <TrackRow
                  track={t}
                  current={player.current?.episode.id === ep.id && player.tracks[player.currentTrackIndex]?.id === t.id}
                  playing={player.playing}
                  playable={ep.has_audio}
                  onplay={() => playTrack(ep, t)}
                  onfavourite={() => favTrack(ep, t)}
                  onshare={() =>
                    shareTrack(
                      t,
                      show?.name ?? "",
                      ep.air_date,
                      wfmuEpisodeUrl(ep.id),
                    )}
                />
              {/each}
            </div>
          {/if}
        {/if}
      </div>
    {/each}
  </div>
{/if}

<style>
  .back {
    display: inline-block;
    margin-bottom: 12px;
    color: var(--c-dim);
  }
  .error {
    background: var(--c-surface2);
    border: 1px solid var(--c-danger);
    padding: 8px 12px;
    border-radius: 8px;
    margin-bottom: 12px;
  }
  .muted {
    color: var(--c-dim);
  }
  .showhead {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    gap: 20px;
    margin-bottom: 18px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0 0 4px;
    font-size: 26px;
  }
  .dj {
    color: var(--c-accent);
    font-size: 15px;
  }
  .blurb {
    color: var(--c-dim);
    font-size: 14px;
    line-height: 1.5;
    margin: 8px 0 4px;
    max-width: 720px;
  }
  .meta {
    color: var(--c-dim);
    font-size: 13px;
    margin-top: 4px;
  }
  .dot {
    margin: 0 6px;
    color: var(--c-dim);
  }
  .wfmu-link {
    color: var(--c-dim);
    text-decoration: underline;
  }
  .wfmu-link:hover {
    color: var(--c-accent);
  }
  .actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .primary {
    background: var(--c-accent);
    color: var(--c-on-accent);
    border: none;
    border-radius: 8px;
    padding: 8px 16px;
    font-weight: 700;
    cursor: pointer;
  }
  .primary:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .ghost {
    background: none;
    border: 1px solid var(--c-border);
    color: var(--c-dim);
    border-radius: 8px;
    padding: 7px 12px;
    cursor: pointer;
  }
  .ghost:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }
  .eplist {
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
    max-width: 100%;
  }
  .ep {
    background: var(--c-surface);
    border-radius: 8px;
    padding: 4px 8px;
    min-width: 0;
    max-width: 100%;
    overflow: hidden;
  }
  .ep.current {
    outline: 1px solid var(--c-accent);
  }
  .ep-row {
    display: flex;
    align-items: center;
    gap: 10px;
    position: relative;
  }
  .ep-prog {
    position: absolute;
    inset: 0 auto 0 0;
    border-radius: 6px;
    background: color-mix(in srgb, var(--c-accent) 20%, transparent);
    pointer-events: none;
    transition: width 0.25s ease;
    z-index: 0;
  }
  .ep-prog.done {
    background: color-mix(in srgb, var(--c-accent) 9%, transparent);
  }
  .ep-row > .playbtn,
  .ep-row > .ep-main,
  .ep-row > .ep-actions {
    position: relative;
    z-index: 1;
  }
  .playbtn {
    background: var(--c-surface2);
    color: var(--c-text);
    border: none;
    border-radius: 50%;
    width: 32px;
    height: 32px;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .playbtn:hover:not(:disabled) {
    background: var(--c-accent);
    color: var(--c-on-accent);
  }
  .playbtn:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .ep-main {
    flex: 1 1 auto;
    min-width: 0;
    cursor: pointer;
    padding: 8px 0;
    border: 0;
    background: none;
    color: inherit;
    font: inherit;
    text-align: left;
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
  }
  .ep-date {
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }
  .ep-title {
    color: var(--c-dim);
    font-size: 14px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 60%;
  }
  .badge {
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    background: var(--c-surface2);
    color: var(--c-dim);
    border-radius: 4px;
    padding: 2px 6px;
  }
  .badge.dl {
    color: var(--c-accent);
  }
  .badge.prog {
    color: var(--c-accent);
  }
  .badge.done {
    color: var(--c-on-accent);
    background: var(--c-accent);
  }
  .ep-actions {
    display: flex;
    gap: 4px;
    flex: 0 0 auto;
  }
  .mini {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 15px;
    padding: 4px 6px;
    border-radius: 6px;
    display: inline-flex;
    align-items: center;
  }
  .mini:hover:not(:disabled) {
    color: var(--c-accent);
    background: var(--c-surface2);
  }
  .mini.on {
    color: var(--c-gold);
  }
  .mini:disabled {
    opacity: 0.3;
    cursor: default;
  }
  .ghost.fav,
  .ghost.share {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .ghost.fav.on {
    color: var(--c-gold);
    border-color: var(--c-gold);
  }
  .tracks {
    border-top: 1px solid var(--c-surface2);
    margin-top: 2px;
    padding: 6px 0 8px;
    min-width: 0;
    max-width: 100%;
  }

  @media (max-width: 760px) {
    .ep-row {
      display: grid;
      grid-template-columns: 32px minmax(0, 1fr);
      column-gap: 8px;
      row-gap: 0;
      align-items: start;
      padding: 4px 0;
    }
    .playbtn {
      grid-column: 1;
      grid-row: 1 / span 2;
      margin-top: 2px;
    }
    .ep-main {
      grid-column: 2;
      grid-row: 1;
      width: 100%;
      padding: 2px 0 4px;
      gap: 4px 8px;
    }
    .ep-date {
      flex: 0 0 auto;
    }
    .ep-title {
      flex: 1 1 120px;
      min-width: 0;
      max-width: 100%;
    }
    .ep-actions {
      grid-column: 2;
      grid-row: 2;
      justify-content: flex-end;
      min-width: 0;
    }
  }
</style>
