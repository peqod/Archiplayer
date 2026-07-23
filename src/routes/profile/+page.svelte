<script lang="ts">
  import {
    api,
    fmtHours,
    fmtTime,
    type Favourites,
    type Stats,
    type DownloadRow,
  } from "$lib/api";
  import { player } from "$lib/player.svelte";
  import { open, save } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";
  import ThemePicker from "$lib/ThemePicker.svelte";
  import Icon from "$lib/Icon.svelte";
  import { shareShow, shareEpisode, shareTrack, wfmuShowUrl } from "$lib/share";

  let favs = $state<Favourites | null>(null);
  let stats = $state<Stats | null>(null);
  let downloads = $state<DownloadRow[]>([]);
  let dlLoading = $state(true);
  let downloadDir = $state("");
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

  async function loadDownloads() {
    error = null;
    try {
      downloads = await api.listDownloads();
      downloadDir = await api.getDownloadDir();
    } catch (e) {
      error = String(e);
    } finally {
      dlLoading = false;
    }
  }
  loadDownloads();

  async function changeDownloadDir() {
    try {
      const picked = await open({ directory: true, defaultPath: downloadDir || undefined });
      if (typeof picked === "string") {
        await api.setDownloadDir(picked);
        downloadDir = picked;
      }
    } catch (e) {
      error = String(e);
    }
  }

  async function revealDownload(row: DownloadRow) {
    try {
      await revealItemInDir(row.download.path);
    } catch (e) {
      error = String(e);
    }
  }

  onMount(() => {
    const un = listen<{ episode_id: number; bytes: number; total: number; status: string }>(
      "download-progress",
      (e) => {
        const p = e.payload;
        const known = downloads.some((r) => r.download.episode_id === p.episode_id);
        if (!known || p.status === "done") {
          loadDownloads();
          return;
        }
        downloads = downloads.map((r) =>
          r.download.episode_id === p.episode_id
            ? { ...r, download: { ...r.download, bytes: p.bytes, total: p.total, status: p.status } }
            : r,
        );
      },
    );
    return () => {
      un.then((f) => f());
    };
  });

  async function playDownload(row: DownloadRow) {
    if (!row.show_id) return;
    try {
      const detail = await api.getShow(row.show_id);
      const ep = detail.episodes.find((e) => e.id === row.download.episode_id);
      if (ep) await player.playEpisode(ep, row.show_name ?? row.show_id);
    } catch (e) {
      error = String(e);
    }
  }

  async function removeDownload(row: DownloadRow) {
    try {
      await api.deleteDownload(row.download.episode_id);
      downloads = downloads.filter((r) => r.download.episode_id !== row.download.episode_id);
    } catch (e) {
      error = String(e);
    }
  }

  function fmtBytes(n: number): string {
    if (n > 1e9) return `${(n / 1e9).toFixed(2)} GB`;
    if (n > 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${Math.round(n / 1e3)} KB`;
  }

  async function unfav(kind: "show" | "episode" | "track", refId: string) {
    try {
      await api.toggleFavourite(kind, refId);
      await load();
    } catch (e) {
      error = String(e);
    }
  }

  async function playFavEpisode(
    episodeId: number,
    showName: string,
    startTrackSec: number | null = null,
  ) {
    try {
      const eps = favs?.episodes.find((f) => f.episode.id === episodeId)?.episode;
      if (eps) {
        await player.playEpisode(eps, showName, null, startTrackSec);
        return;
      }
      // favourite track: episode not in favourites list — fetch via its show
      const t = favs?.tracks.find((f) => f.track.episode_id === episodeId);
      if (t) {
        const detail = await api.getShow(t.show_id);
        const ep = detail.episodes.find((e) => e.id === episodeId);
        if (ep) await player.playEpisode(ep, t.show_name, null, startTrackSec);
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

<div class="cols">
  <details class="fold col-fold" open>
    <summary><span class="cz-title">Listening stats</span></summary>
    {#if stats}
      <details class="fold stat-fold" open>
        <summary>
          <span class="cz-title">Totals</span>
        </summary>
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
      </details>

      <details class="fold stat-fold" open>
        <summary>
          <span class="cz-title">Audition ranking — shows</span>
        </summary>
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
      </details>

      {#if stats.episodes.length}
        <details class="fold stat-fold" open>
          <summary>
            <span class="cz-title">Most-listened episodes</span>
          </summary>
          <ol class="rank">
            {#each stats.episodes.slice(0, 15) as e (e.episode_id)}
              <li>
                <span>{e.show_name} — {e.air_date ?? ""}{e.title ? ` · ${e.title}` : ""}</span>
                <span class="rank-meta">{fmtHours(e.seconds)}</span>
              </li>
            {/each}
          </ol>
        </details>
      {/if}
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </details>

  <details class="fold col-fold" open>
    <summary><span class="cz-title">Favourites</span></summary>
    {#if favs}
      <details class="fold stat-fold" open>
        <summary><span class="cz-title">Shows ({favs.shows.length})</span></summary>
        {#each favs.shows as f (f.show.id)}
          <div class="fav">
            <a href={"/show/" + f.show.id}>{f.show.name}</a>
            {#if f.show.dj}<span class="muted">with {f.show.dj}</span>{/if}
            <div class="fav-actions">
              <button class="mini share" onclick={() => shareShow(f.show)} title="Share"><Icon name="share" /></button>
              <button class="mini" onclick={() => unfav("show", f.show.id)} title="Remove"><Icon name="star" filled /></button>
            </div>
          </div>
        {:else}
          <p class="muted">None yet — hover a show and hit the star.</p>
        {/each}
      </details>

      <details class="fold stat-fold" open>
        <summary><span class="cz-title">Episodes ({favs.episodes.length})</span></summary>
        {#each favs.episodes as f (f.episode.id)}
          <div class="fav">
            <button
              class="linkish"
              onclick={() => playFavEpisode(f.episode.id, f.show_name)}
              disabled={!f.episode.has_audio}
              title="Play"
            ><Icon name="play" /></button>
            <span>{f.show_name} — {f.episode.air_date ?? ""}</span>
            {#if f.episode.title}<span class="muted ellip">{f.episode.title}</span>{/if}
            <div class="fav-actions">
              <button class="mini share" onclick={() => shareEpisode(f.show_name, f.episode)} title="Share"><Icon name="share" /></button>
              <button class="mini" onclick={() => unfav("episode", String(f.episode.id))} title="Remove"><Icon name="star" filled /></button>
            </div>
          </div>
        {:else}
          <p class="muted">None yet.</p>
        {/each}
      </details>

      <details class="fold stat-fold" open>
        <summary><span class="cz-title">Songs ({favs.tracks.length})</span></summary>
        {#each favs.tracks as f (f.track.id)}
          <div class="fav">
            <button
              class="linkish"
              onclick={() => playFavEpisode(f.track.episode_id, f.show_name, f.track.start_sec)}
              title={f.track.start_sec !== null ? `Play at ${fmtTime(f.track.start_sec)}` : "Play episode"}
            ><Icon name="play" /></button>
            <span class="ellip">
              <b>{f.track.artist ?? "?"}</b> — {f.track.title ?? "?"}
            </span>
            <span class="muted">{f.show_name} · {f.air_date ?? ""}</span>
            <div class="fav-actions">
              <button class="mini share" onclick={() => shareTrack(f.track, f.show_name, f.air_date, wfmuShowUrl(f.show_id))} title="Share"><Icon name="share" /></button>
              <button class="mini" onclick={() => unfav("track", String(f.track.id))} title="Remove"><Icon name="star" filled /></button>
            </div>
          </div>
        {:else}
          <p class="muted">None yet — favourite songs from a playlist.</p>
        {/each}
      </details>
    {:else}
      <p class="muted">Loading…</p>
    {/if}
  </details>
</div>

<div class="export">
  <span>Export CSV:</span>
  <button class="ghost" onclick={() => exportCsv("favourites")}>Favourites</button>
  <button class="ghost" onclick={() => exportCsv("listens")}>Listening history</button>
  <button class="ghost" onclick={() => exportCsv("stats")}>Stats ranking</button>
</div>

<details class="fold downloads">
  <summary>
    <span class="cz-title">Downloads</span>
    <span class="cz-hint">saved for offline</span>
  </summary>
  <div class="dl-dir">
    <span class="muted">Folder:</span>
    <span class="dl-dir-path" title={downloadDir}>{downloadDir || "default"}</span>
    <button class="ghost" onclick={changeDownloadDir}>Change…</button>
  </div>
  {#if dlLoading}
    <p class="muted">Loading…</p>
  {:else if downloads.length === 0}
    <p class="muted">Nothing downloaded yet. Use the ⤓ button on any episode.</p>
  {:else}
    <div class="dl-list">
      {#each downloads as row (row.download.episode_id)}
        <div class="dl-row">
          <button
            class="dl-play"
            onclick={() => playDownload(row)}
            disabled={!row.show_id || row.download.status !== "done"}
            title="Play offline copy"
          ><Icon name="play" /></button>
          <div class="dl-info">
            <div class="dl-title">
              {#if row.show_name}
                {row.show_name} — {row.air_date ?? ""}
                {#if row.title}<span class="muted"> · {row.title}</span>{/if}
              {:else}
                Episode #{row.download.episode_id}
              {/if}
            </div>
            <div class="dl-sub">
              {#if row.download.status === "done"}
                {fmtBytes(row.download.bytes)} · {row.download.path}
              {:else if row.download.status === "downloading"}
                downloading… {fmtBytes(row.download.bytes)}{row.download.total ? ` / ${fmtBytes(row.download.total)}` : ""}
              {:else}
                {row.download.status}
              {/if}
            </div>
          </div>
          <button
            class="dl-open"
            onclick={() => revealDownload(row)}
            disabled={row.download.status !== "done"}
            title="Show in file explorer"
          >📁</button>
          <button class="dl-del" onclick={() => removeDownload(row)} title="Delete file">🗑</button>
        </div>
      {/each}
    </div>
  {/if}
</details>

<details class="fold customize">
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
  .col-fold > summary {
    border-bottom: 1px solid var(--c-surface2);
    padding-bottom: 6px;
    margin-bottom: 6px;
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
  .fold summary {
    cursor: pointer;
    list-style: none;
    display: flex;
    align-items: baseline;
    gap: 10px;
    user-select: none;
  }
  .fold summary::-webkit-details-marker {
    display: none;
  }
  .fold summary::before {
    content: "▸";
    color: var(--c-dim);
    font-size: 12px;
    transition: transform 0.15s;
  }
  .fold[open] summary::before {
    transform: rotate(90deg);
  }
  .customize,
  .downloads {
    margin: 28px 0 8px;
    border-top: 1px solid var(--c-surface2);
    padding-top: 14px;
  }
  .stat-fold {
    margin: 12px 0;
  }
  .stat-fold .cz-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--c-accent);
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
  .fav-actions {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
  }
  .mini {
    background: none;
    border: none;
    color: var(--c-gold);
    cursor: pointer;
    font-size: 15px;
    display: inline-flex;
    align-items: center;
  }
  .mini:hover {
    color: var(--c-danger);
  }
  .mini.share {
    color: var(--c-dim);
  }
  .mini.share:hover {
    color: var(--c-accent);
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
  .dl-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 12px;
  }
  .dl-row {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--c-surface);
    border-radius: 8px;
    padding: 10px 12px;
  }
  .dl-play {
    background: var(--c-surface2);
    color: var(--c-text);
    border: none;
    border-radius: 50%;
    width: 34px;
    height: 34px;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .dl-play:hover:not(:disabled) {
    background: var(--c-accent);
    color: var(--c-surface);
  }
  .dl-play:disabled {
    opacity: 0.35;
  }
  .dl-info {
    flex: 1 1 auto;
    min-width: 0;
  }
  .dl-title {
    font-weight: 600;
    font-size: 14px;
  }
  .dl-sub {
    color: var(--c-dim);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dl-del {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 16px;
  }
  .dl-del:hover {
    color: var(--c-danger);
  }
  .dl-open {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 15px;
  }
  .dl-open:hover:not(:disabled) {
    color: var(--c-accent);
  }
  .dl-open:disabled {
    opacity: 0.35;
  }
  .dl-dir {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 8px 0 12px;
    font-size: 13px;
  }
  .dl-dir-path {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--c-dim);
    font-family: monospace;
  }
</style>
