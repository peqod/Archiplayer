<script lang="ts">
  import { page } from "$app/stores";
  import {
    api,
    fmtTime,
    type Episode,
    type Show,
    type Track,
  } from "$lib/api";
  import { player, type QueueItem } from "$lib/player.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "$lib/Icon.svelte";
  import { onMount } from "svelte";

  function openWfmu(e: MouseEvent) {
    e.preventDefault();
    if (show) openUrl("https://wfmu.org/playlists/" + show.id).catch(() => {});
  }

  let show = $state<Show | null>(null);
  let episodes = $state<Episode[]>([]);
  let loading = $state(true);
  let refreshing = $state(false);
  let error = $state<string | null>(null);
  let expanded = $state<Record<number, Track[] | "loading">>({});
  let downloading = $state<Record<number, { bytes: number; total: number }>>({});

  const showId = $derived($page.params.id ?? "");

  async function load(refresh = false) {
    error = null;
    if (refresh) refreshing = true;
    try {
      const detail = await api.getShow(showId, refresh);
      show = detail.show;
      episodes = detail.episodes;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
      refreshing = false;
    }
  }

  $effect(() => {
    void showId;
    show = null;
    episodes = [];
    expanded = {};
    loading = true;
    load();
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
    return () => {
      un.then((f) => f());
    };
  });

  async function togglePlaylist(ep: Episode) {
    if (expanded[ep.id]) {
      const { [ep.id]: _, ...rest } = expanded;
      expanded = rest;
      return;
    }
    expanded = { ...expanded, [ep.id]: "loading" };
    try {
      const tracks = await api.getPlaylist(ep.id);
      expanded = { ...expanded, [ep.id]: tracks };
      episodes = episodes.map((e) =>
        e.id === ep.id ? { ...e, track_count: tracks.length } : e,
      );
    } catch (e) {
      error = String(e);
      const { [ep.id]: _, ...rest } = expanded;
      expanded = rest;
    }
  }

  // Play `ep` inside a queue of the whole show (oldest → newest) so the transport's
  // prev/next-episode buttons can walk the show. Falls back to a lone-episode queue.
  async function queueShowAt(ep: Episode, startSec: number | null) {
    if (!show) return;
    const chrono = [...episodes].reverse().filter((e) => e.has_audio);
    const idx = chrono.findIndex((e) => e.id === ep.id);
    if (idx < 0) {
      await player.playEpisode(ep, show.name, startSec);
      return;
    }
    const items: QueueItem[] = chrono.map((e) => ({ episode: e, showName: show!.name }));
    await player.playQueue(items, idx, startSec);
  }

  async function playEpisode(ep: Episode) {
    if (!show) return;
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
    // queue: this episode then everything after it chronologically
    if (!show) return;
    const chrono = [...episodes].reverse().filter((e) => e.has_audio);
    const idx = chrono.findIndex((e) => e.id === ep.id);
    if (idx < 0) return;
    const items: QueueItem[] = chrono
      .slice(idx)
      .map((e) => ({ episode: e, showName: show!.name }));
    await player.playQueue(items);
  }

  async function playTrack(ep: Episode, track: Track) {
    if (!show) return;
    if (player.current?.episode.id === ep.id) {
      player.seekToTrack(track);
      return;
    }
    await queueShowAt(ep, track.start_sec);
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

  async function playAllChrono() {
    if (!show) return;
    const items: QueueItem[] = [...episodes]
      .reverse()
      .filter((e) => e.has_audio)
      .map((e) => ({ episode: e, showName: show!.name }));
    if (items.length) await player.playQueue(items);
  }

  const playableCount = $derived(episodes.filter((e) => e.has_audio).length);

  function tracksOf(ep: Episode): Track[] {
    const v = expanded[ep.id];
    return Array.isArray(v) ? v : [];
  }
</script>

<a href="/" class="back">← All shows</a>

{#if error}
  <div class="error">{error} <button class="ghost" onclick={() => (error = null)}>✕</button></div>
{/if}

{#if loading}
  <p class="muted">Loading episodes{episodes.length === 0 ? " (scraping show page on first visit)" : ""}…</p>
{:else if show}
  <div class="showhead">
    <div>
      <h1>{show.name}</h1>
      {#if show.dj}<div class="dj">with {show.dj}</div>{/if}
      {#if show.description}<p class="blurb">{show.description}</p>{/if}
      <div class="meta">
        {episodes.length} episodes · {playableCount} with audio
        {#if !show.on_air}· no longer on air{/if}
        <span class="dot">·</span>
        <a class="wfmu-link" href={"https://wfmu.org/playlists/" + show.id} onclick={openWfmu}>View on WFMU ↗</a>
      </div>
    </div>
    <div class="actions">
      <button class="primary" onclick={playAllChrono} disabled={!playableCount}><Icon name="play" /> Play all (oldest first)</button>
      <button class="ghost fav" class:on={show.favourite} onclick={favShow}>
        <Icon name="star" filled={show.favourite} /> {show.favourite ? "Favourited" : "Favourite"}
      </button>
      <button class="ghost" onclick={() => load(true)} disabled={refreshing}>
        {#if refreshing}Refreshing…{:else}<Icon name="refresh" /> Refresh{/if}
      </button>
    </div>
  </div>

  <div class="eplist">
    {#each episodes as ep (ep.id)}
      <div class="ep" class:current={player.current?.episode.id === ep.id}>
        <div class="ep-row">
          {#if progressFrac(ep) > 0}
            <div
              class="ep-prog"
              class:done={ep.completed}
              style="width:{progressFrac(ep) * 100}%"
            ></div>
          {/if}
          <button
            class="playbtn"
            onclick={() => playEpisode(ep)}
            disabled={!ep.has_audio}
            title={ep.has_audio ? "Play this episode" : "No audio archive"}
          >
            {#if ep.has_audio}<Icon name="play" />{:else}–{/if}
          </button>
          <div class="ep-main" role="button" tabindex="0"
            onclick={() => togglePlaylist(ep)}
            onkeydown={(e) => e.key === "Enter" && togglePlaylist(ep)}>
            <span class="ep-date">{ep.air_date ?? "unknown date"}</span>
            {#if ep.title}<span class="ep-title">{ep.title}</span>{/if}
            {#if ep.completed}<span class="badge done">completed</span>{/if}
            {#if !ep.completed && ep.resume_sec && ep.resume_sec > 5}
              <span class="badge resume">↺ {fmtTime(ep.resume_sec)}</span>
            {/if}
            {#if !ep.has_audio}<span class="badge">playlist only</span>{/if}
            {#if ep.downloaded}<span class="badge dl">offline</span>{/if}
            {#if downloading[ep.id]}
              <span class="badge prog">
                ↓ {Math.round((downloading[ep.id].bytes / Math.max(downloading[ep.id].total, 1)) * 100)}%
              </span>
            {/if}
          </div>
          <div class="ep-actions">
            <button class="mini" onclick={() => playFromHere(ep)} disabled={!ep.has_audio} title="Play from this episode onward"><Icon name="next" /></button>
            <button class="mini" class:on={ep.favourite} onclick={() => favEpisode(ep)} title="Save episode"><Icon name="save" filled={ep.favourite} /></button>
            <button
              class="mini"
              class:on={ep.downloaded}
              onclick={() => download(ep)}
              disabled={!ep.has_audio || ep.downloaded || !!downloading[ep.id]}
              title={ep.downloaded ? "Already downloaded" : "Download for offline"}
            ><Icon name="download" /></button>
            <button class="mini" onclick={() => togglePlaylist(ep)} title="Playlist">
              {expanded[ep.id] ? "▴" : "▾"}
            </button>
          </div>
        </div>
        {#if expanded[ep.id] === "loading"}
          <div class="tracks muted">Loading playlist…</div>
        {:else if Array.isArray(expanded[ep.id])}
          {@const tracks = tracksOf(ep)}
          {#if tracks.length === 0}
            <div class="tracks muted">No playlist recorded for this episode.</div>
          {:else}
            <div class="tracks">
              {#each tracks as t, i (t.id)}
                <div
                  class="track"
                  class:now={player.current?.episode.id === ep.id && player.currentTrackIndex === i}
                >
                  <button
                    class="tplay"
                    onclick={() => playTrack(ep, t)}
                    disabled={!ep.has_audio}
                    title={t.start_sec !== null ? `Play at ${fmtTime(t.start_sec)}` : "Play episode (no timestamp)"}
                  ><Icon name="play" /></button>
                  <span class="ttime">{t.start_sec !== null ? fmtTime(t.start_sec) : "–"}</span>
                  <span class="tartist">{t.artist ?? ""}</span>
                  <span class="ttitle">{t.title ?? ""}</span>
                  <span class="talbum">{t.album ?? ""}</span>
                  <button class="mini" class:on={t.favourite} onclick={() => favTrack(ep, t)} title="Star song">
                    <Icon name="star" filled={t.favourite} />
                  </button>
                </div>
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
  }
  .ep {
    background: var(--c-surface);
    border-radius: 8px;
    padding: 4px 8px;
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
  /* Optical-centre the play triangle in the round episode button. */
  .playbtn :global(svg.icon) {
    transform: translateX(2px);
  }
  .ep-main {
    flex: 1 1 auto;
    min-width: 0;
    cursor: pointer;
    padding: 8px 0;
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
  .badge.resume {
    color: var(--c-accent);
    font-variant-numeric: tabular-nums;
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
  .ghost.fav {
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
  }
  .track {
    display: grid;
    grid-template-columns: 30px 64px 220px 1fr 220px 30px;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 6px;
    font-size: 13px;
  }
  .track:hover {
    background: var(--c-surface);
  }
  .track.now {
    background: var(--c-surface2);
    color: var(--c-gold);
  }
  .tplay {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    padding: 2px;
  }
  .tplay:hover:not(:disabled) {
    color: var(--c-accent);
  }
  .tplay:disabled {
    opacity: 0.3;
  }
  .ttime {
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .tartist {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ttitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .talbum {
    color: var(--c-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
  }
</style>
