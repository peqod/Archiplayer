<script lang="ts">
  import "@fontsource/inter/400.css";
  import "@fontsource/inter/600.css";
  import "@fontsource/inter/700.css";
  import "@fontsource/inter/800.css";
  import { player } from "$lib/player.svelte";
  import { api, fmtTime } from "$lib/api";
  import { theme } from "$lib/theme.svelte";
  import Icon from "$lib/Icon.svelte";
  import { page } from "$app/stores";
  import { tick } from "svelte";
  import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

  let { children } = $props();
  let audioEl: HTMLAudioElement;
  let favError = $state<string | null>(null);
  let collapsed = $state(false);
  let navEl: HTMLElement;
  let headerEl: HTMLElement;
  let expandedHeight = 0; // remembered logical height to restore on expand

  if (typeof window !== "undefined") {
    theme.load();
    collapsed = localStorage.getItem("ap.collapsed") === "1";
  }

  async function applyWindowSize() {
    try {
      const win = getCurrentWindow();
      const scale = await win.scaleFactor();
      const cur = (await win.innerSize()).toLogical(scale);
      if (collapsed) {
        // fit the window to just the player + tabs
        const compact = Math.ceil((headerEl?.offsetHeight ?? 0) + (navEl?.offsetHeight ?? 0));
        if (compact > 0) await win.setSize(new LogicalSize(cur.width, compact));
      } else {
        // restore the pre-collapse height (or a sensible default if we started collapsed)
        const target = expandedHeight > 0 ? expandedHeight : 860;
        if (cur.height < target) await win.setSize(new LogicalSize(cur.width, target));
      }
    } catch {
      /* not in Tauri / no window perms — button still hides the library */
    }
  }

  async function toggleCollapse() {
    if (!collapsed) {
      // about to collapse — remember the current height to restore later
      try {
        const win = getCurrentWindow();
        const scale = await win.scaleFactor();
        expandedHeight = (await win.innerSize()).toLogical(scale).height;
      } catch {
        /* ignore */
      }
    }
    collapsed = !collapsed;
    localStorage.setItem("ap.collapsed", collapsed ? "1" : "0");
    await tick();
    await applyWindowSize();
  }

  $effect(() => {
    if (audioEl) player.attach(audioEl);
  });

  // On startup, if we restored a collapsed state, shrink the window to match.
  $effect(() => {
    if (collapsed && headerEl && navEl) {
      applyWindowSize();
    }
  });

  const volumeIcon = $derived(
    player.volume < 0.02 ? "volume-mute" : player.volume < 0.5 ? "volume-quiet" : "volume-loud",
  );

  function onScrub(e: Event) {
    const v = Number((e.target as HTMLInputElement).value);
    player.seek(v);
  }
  function onVolume(e: Event) {
    player.setVolume(Number((e.target as HTMLInputElement).value));
  }

  const currentTrack = $derived(
    player.currentTrackIndex >= 0 ? player.tracks[player.currentTrackIndex] : null,
  );

  async function bookmarkShow() {
    const ep = player.current?.episode;
    if (!ep) return;
    try {
      const fav = await api.toggleFavourite("episode", String(ep.id));
      player.setEpisodeFavourite(fav);
    } catch (e) {
      favError = String(e);
    }
  }

  async function bookmarkSong() {
    const t = currentTrack;
    if (!t) return;
    try {
      const fav = await api.toggleFavourite("track", String(t.id));
      player.setTrackFavourite(t.id, fav);
    } catch (e) {
      favError = String(e);
    }
  }
</script>

<div class="app">
  <header class="player" class:inactive={!player.current} bind:this={headerEl}>
    <!-- svelte-ignore a11y_media_has_caption -->
    <audio bind:this={audioEl} preload="none"></audio>
    <div class="p-controls">
      <button class="pbtn" onclick={() => player.prev()} disabled={!player.current} title="Previous / restart">⏮</button>
      <button class="pbtn main" onclick={() => player.toggle()} disabled={!player.current} title="Play/pause">
        {#if player.loading}…{:else if player.playing}⏸{:else}▶{/if}
      </button>
      <button class="pbtn" onclick={() => player.next()} disabled={!player.current || player.queueIndex >= player.queue.length - 1} title="Next in queue">⏭</button>
    </div>
    <div class="p-info">
      {#if player.current}
        <div class="p-title">
          <a href={"/show/" + player.current.episode.show_id}>{player.current.showName}</a>
          <span class="p-date">{player.current.episode.air_date ?? ""}</span>
          {#if player.queue.length > 1}
            <span class="p-queue">{player.queueIndex + 1}/{player.queue.length}</span>
          {/if}
        </div>
        <div class="p-track">
          {#if currentTrack}
            ♪ {currentTrack.artist ?? "?"} — {currentTrack.title ?? "?"}
          {:else if player.error}
            <span class="err">{player.error}</span>
          {:else if player.current.episode.title}
            {player.current.episode.title}
          {/if}
        </div>
        <div class="p-scrub">
          <span class="p-time">{fmtTime(player.currentTime)}</span>
          <input
            type="range"
            min="0"
            max={player.duration || 0}
            step="1"
            value={player.currentTime}
            oninput={onScrub}
            disabled={!player.duration}
          />
          <span class="p-time">{fmtTime(player.duration)}</span>
        </div>
      {:else}
        <div class="p-track idle">Nothing playing — pick a show.</div>
      {/if}
    </div>
    {#if player.current}
      <div class="p-bookmarks">
        <button
          class="bm"
          class:on={currentTrack?.favourite}
          onclick={bookmarkSong}
          disabled={!currentTrack}
          title={currentTrack ? "Star this song" : "No current song"}
        ><Icon name="star" filled={currentTrack?.favourite} /> song</button>
        <button
          class="bm"
          class:on={player.current.episode.favourite}
          onclick={bookmarkShow}
          title="Save this episode"
        ><Icon name="save" filled={player.current.episode.favourite} /> show</button>
      </div>
    {/if}
    <div class="p-volume">
      <Icon name={volumeIcon} size="24px" />
      <input type="range" min="0" max="1" step="0.02" value={player.volume} oninput={onVolume} />
    </div>
  </header>

  <nav bind:this={navEl}>
    <a class="brand" href="/">
      <img class="brand-logo" src="/logo.gif" alt="Archiplayer" width="34" height="34" />
      <span class="brand-name">Archiplayer</span>
      <span class="brand-sub">WFMU</span>
    </a>
    <div class="nav-links">
      <a href="/" class:active={$page.url.pathname === "/"}>Shows</a>
      <a href="/profile" class:active={$page.url.pathname.startsWith("/profile")}>Profile</a>
      <a href="/downloads" class:active={$page.url.pathname.startsWith("/downloads")}>Downloads</a>
    </div>
    <button
      class="collapse-btn"
      onclick={toggleCollapse}
      title={collapsed ? "Show library" : "Collapse to player only"}
      aria-label={collapsed ? "Show library" : "Collapse to player only"}
    >{collapsed ? "▾ Library" : "▴ Player only"}</button>
  </nav>

  {#if !collapsed}
    <main>
      {@render children()}
    </main>
  {/if}
  {#if favError}<button class="fav-error" onclick={() => (favError = null)}>{favError} ✕</button>{/if}
</div>

<style>
  :global(:root) {
    /* Default tokens (Archiplayer brand); the theme store overrides these. */
    --c-bg: #141a2e;
    --c-surface: #1e2440;
    --c-surface2: #2b3355;
    --c-border: #38406a;
    --c-text: #f3edc6;
    --c-dim: #a6a2b4;
    --c-accent: #12a594;
    --c-on-accent: #04231f;
    --c-gold: #ffdd87;
    --c-danger: #c4453c;
    --c-line: #d8483f;
  }
  :global(*) {
    box-sizing: border-box;
  }
  :global(body) {
    margin: 0;
    background: var(--c-bg);
    color: var(--c-text);
    font-family: "Inter", "Segoe UI", system-ui, sans-serif;
    font-size: 15px;
  }
  :global(a) {
    color: var(--c-accent);
    text-decoration: none;
  }
  :global(a:hover) {
    text-decoration: underline;
  }
  :global(button) {
    font-family: inherit;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
  }
  nav {
    display: flex;
    align-items: center;
    gap: 24px;
    padding: 10px 20px;
    background: var(--c-surface);
    border-top: 2px solid var(--c-line);
    flex: 0 0 auto;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 9px;
    color: inherit;
  }
  .brand:hover {
    text-decoration: none;
  }
  .brand-logo {
    border-radius: 6px;
    display: block;
  }
  .brand-name {
    font-weight: 800;
    letter-spacing: 0.01em;
  }
  .brand-sub {
    color: var(--c-dim);
    font-size: 12px;
  }
  .nav-links {
    display: flex;
    gap: 16px;
  }
  .collapse-btn {
    margin-left: auto;
    background: var(--c-surface2);
    color: var(--c-dim);
    border: none;
    border-radius: 6px;
    padding: 5px 12px;
    font-weight: 600;
    font-size: 13px;
    cursor: pointer;
  }
  .collapse-btn:hover {
    color: var(--c-accent);
  }
  .nav-links a {
    color: var(--c-dim);
    padding: 4px 10px;
    border-radius: 6px;
    font-weight: 600;
  }
  .nav-links a:hover {
    color: var(--c-accent);
    text-decoration: none;
  }
  .nav-links a.active {
    background: var(--c-surface2);
    color: var(--c-accent);
  }
  main {
    flex: 1 1 auto;
    overflow-y: auto;
    padding: 20px;
  }
  .player {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 18px;
    padding: 10px 20px;
    background: var(--c-surface);
  }
  .player.inactive {
    opacity: 0.75;
  }
  .p-controls {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  .pbtn {
    background: var(--c-surface2);
    color: var(--c-text);
    border: none;
    border-radius: 50%;
    width: 36px;
    height: 36px;
    cursor: pointer;
    font-size: 14px;
  }
  .pbtn.main {
    width: 46px;
    height: 46px;
    background: var(--c-accent);
    color: var(--c-on-accent);
    font-size: 18px;
  }
  .pbtn:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .p-info {
    flex: 1 1 auto;
    min-width: 0;
  }
  .p-title {
    font-weight: 700;
    font-size: 14px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .p-date {
    color: var(--c-dim);
    font-weight: 400;
    margin-left: 8px;
    font-size: 12px;
  }
  .p-queue {
    color: var(--c-accent);
    font-size: 12px;
    margin-left: 8px;
  }
  .p-track {
    font-size: 13px;
    color: var(--c-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .p-track.idle {
    color: var(--c-dim);
    opacity: 0.7;
  }
  .err {
    color: var(--c-danger);
  }
  .p-scrub {
    display: flex;
    align-items: center;
    gap: 10px;
    margin-top: 4px;
  }
  .p-scrub input {
    flex: 1 1 auto;
    accent-color: var(--c-accent);
  }
  .p-time {
    font-size: 12px;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
    min-width: 48px;
    text-align: center;
  }
  .p-bookmarks {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
  }
  .bm {
    background: var(--c-surface2);
    color: var(--c-dim);
    border: none;
    border-radius: 16px;
    padding: 6px 10px;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    white-space: nowrap;
    display: inline-flex;
    align-items: center;
    gap: 5px;
  }
  .bm:hover:not(:disabled) {
    color: var(--c-gold);
  }
  .bm.on {
    color: var(--c-gold);
  }
  .bm:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .fav-error {
    position: fixed;
    bottom: 90px;
    right: 20px;
    background: var(--c-surface2);
    border: 1px solid var(--c-danger);
    color: var(--c-text);
    padding: 8px 14px;
    border-radius: 8px;
    font-size: 13px;
    cursor: pointer;
    z-index: 10;
  }
  .p-volume {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--c-dim);
  }
  .p-volume input {
    width: 90px;
    accent-color: var(--c-accent);
  }
</style>
