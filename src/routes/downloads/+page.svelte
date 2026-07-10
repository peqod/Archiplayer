<script lang="ts">
  import { api, type DownloadRow } from "$lib/api";
  import { player } from "$lib/player.svelte";
  import { listen } from "@tauri-apps/api/event";
  import { onMount } from "svelte";

  let rows = $state<DownloadRow[]>([]);
  let error = $state<string | null>(null);
  let loading = $state(true);

  async function load() {
    error = null;
    try {
      rows = await api.listDownloads();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }
  load();

  onMount(() => {
    const un = listen<{ episode_id: number; bytes: number; total: number; status: string }>(
      "download-progress",
      (e) => {
        const p = e.payload;
        const known = rows.some((r) => r.download.episode_id === p.episode_id);
        if (!known || p.status === "done") {
          load();
          return;
        }
        rows = rows.map((r) =>
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

  async function play(row: DownloadRow) {
    if (!row.show_id) return;
    try {
      const detail = await api.getShow(row.show_id);
      const ep = detail.episodes.find((e) => e.id === row.download.episode_id);
      if (ep) await player.playEpisode(ep, row.show_name ?? row.show_id);
    } catch (e) {
      error = String(e);
    }
  }

  async function remove(row: DownloadRow) {
    try {
      await api.deleteDownload(row.download.episode_id);
      rows = rows.filter((r) => r.download.episode_id !== row.download.episode_id);
    } catch (e) {
      error = String(e);
    }
  }

  function fmtBytes(n: number): string {
    if (n > 1e9) return `${(n / 1e9).toFixed(2)} GB`;
    if (n > 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${Math.round(n / 1e3)} KB`;
  }
</script>

<h1>Downloads</h1>
<p class="muted">Episodes saved for offline listening. Player prefers the local file automatically.</p>

{#if error}<div class="error">{error}</div>{/if}

{#if loading}
  <p class="muted">Loading…</p>
{:else if rows.length === 0}
  <p class="muted">Nothing downloaded yet. Use the ⤓ button on any episode.</p>
{:else}
  <div class="list">
    {#each rows as row (row.download.episode_id)}
      <div class="row">
        <button
          class="playbtn"
          onclick={() => play(row)}
          disabled={!row.show_id || row.download.status !== "done"}
          title="Play offline copy"
        >▶</button>
        <div class="info">
          <div class="title">
            {#if row.show_name}
              {row.show_name} — {row.air_date ?? ""}
              {#if row.title}<span class="muted"> · {row.title}</span>{/if}
            {:else}
              Episode #{row.download.episode_id}
            {/if}
          </div>
          <div class="sub">
            {#if row.download.status === "done"}
              {fmtBytes(row.download.bytes)} · {row.download.path}
            {:else if row.download.status === "downloading"}
              downloading… {fmtBytes(row.download.bytes)}{row.download.total ? ` / ${fmtBytes(row.download.total)}` : ""}
            {:else}
              {row.download.status}
            {/if}
          </div>
        </div>
        <button class="mini" onclick={() => remove(row)} title="Delete file">🗑</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  h1 {
    margin: 0 0 4px;
    font-size: 22px;
  }
  .muted {
    color: var(--c-dim);
  }
  .error {
    background: var(--c-surface2);
    border: 1px solid var(--c-danger);
    padding: 8px 12px;
    border-radius: 8px;
    margin: 10px 0;
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 12px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    background: var(--c-surface);
    border-radius: 8px;
    padding: 10px 12px;
  }
  .playbtn {
    background: var(--c-surface2);
    color: var(--c-text);
    border: none;
    border-radius: 50%;
    width: 34px;
    height: 34px;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .playbtn:hover:not(:disabled) {
    background: var(--c-accent);
    color: var(--c-surface);
  }
  .playbtn:disabled {
    opacity: 0.35;
  }
  .info {
    flex: 1 1 auto;
    min-width: 0;
  }
  .title {
    font-weight: 600;
    font-size: 14px;
  }
  .sub {
    color: var(--c-dim);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mini {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 16px;
  }
  .mini:hover {
    color: var(--c-danger);
  }
</style>
