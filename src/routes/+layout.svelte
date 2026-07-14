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
  import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";

  let { children } = $props();
  let audioEl: HTMLAudioElement;
  let favError = $state<string | null>(null);
  // Player-only collapse: hide <main> and shrink the window to just the player + tabs.
  let collapsed = $state(false);
  let navEl: HTMLElement;
  let headerEl: HTMLElement;
  let expandedWidth = 0;
  let expandedHeight = 0; // remembered logical height to restore on expand
  // Mirrors the CSS mini-mode breakpoint so volume can fold to a popover.
  let mini = $state(false);
  let volOpen = $state(false);
  let applyingWindowSize = false;

  if (typeof window !== "undefined") {
    theme.load();
    collapsed = localStorage.getItem("ap.collapsed") === "1";
    const mq = window.matchMedia("(max-width: 760px)");
    mini = mq.matches;
    mq.addEventListener("change", (e) => {
      mini = e.matches;
      volOpen = false; // leaving/entering mini closes any open volume popover
    });
  }

  async function applyWindowSize() {
    if (applyingWindowSize) return;
    applyingWindowSize = true;
    try {
      const win = getCurrentWindow();
      const scale = await win.scaleFactor();
      const physical = await win.innerSize();
      if (collapsed) {
        // Tauri setSize targets the client area. Only resize when the measured
        // content actually differs, otherwise ResizeObserver would feed itself.
        const compact = Math.ceil((headerEl?.offsetHeight ?? 0) + (navEl?.offsetHeight ?? 0));
        const compactWidth = 324;
        const width = Math.round(compactWidth * scale);
        const height = Math.round(compact * scale);
        if (
          compact > 0 &&
          (Math.abs(physical.width - width) > 1 || Math.abs(physical.height - height) > 1)
        ) {
          await win.setSize(new PhysicalSize(width, height));
        }
      } else {
        // Restore the exact pre-collapse client size. A restored collapsed session
        // has no in-memory size, so use a comfortable library default.
        const targetWidth = expandedWidth > 0 ? expandedWidth : 1100;
        const targetHeight = expandedHeight > 0 ? expandedHeight : 860;
        const width = Math.round(targetWidth * scale);
        const height = Math.round(targetHeight * scale);
        if (
          Math.abs(physical.width - width) > 1 ||
          Math.abs(physical.height - height) > 1
        ) {
          await win.setSize(new PhysicalSize(width, height));
        }
      }
    } catch {
      /* not in Tauri / no window perms — button still hides the library */
    } finally {
      applyingWindowSize = false;
    }
  }

  async function toggleCollapse() {
    if (!collapsed) {
      // about to collapse — remember the current height to restore later
      try {
        const win = getCurrentWindow();
        const scale = await win.scaleFactor();
        const size = (await win.innerSize()).toLogical(scale);
        expandedWidth = size.width;
        expandedHeight = size.height;
      } catch {
        /* ignore */
      }
    }
    collapsed = !collapsed;
    localStorage.setItem("ap.collapsed", collapsed ? "1" : "0");
    await tick();
    await applyWindowSize();
  }

  function isActive(path: string): boolean {
    return path === "/" ? $page.url.pathname === "/" : $page.url.pathname.startsWith(path);
  }

  // Native links handle navigation. Clicking the active tab only toggles player-only;
  // another tab expands the shell while its normal link event continues.
  function onNavClick(event: MouseEvent, path: string) {
    if (isActive(path)) {
      event.preventDefault();
      void toggleCollapse();
      return;
    }
    if (collapsed) void toggleCollapse();
  }

  $effect(() => {
    if (audioEl) player.attach(audioEl);
  });

  // Keep compact sizing synchronized when playback adds metadata/scrubbing rows or
  // fonts finish loading. This also performs the initial restored-state resize.
  $effect(() => {
    if (!collapsed || !headerEl || !navEl || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(() => void applyWindowSize());
    observer.observe(headerEl);
    observer.observe(navEl);
    void applyWindowSize();
    return () => observer.disconnect();
  });

  const volumeIcon = $derived(
    player.muted || player.volume < 0.02 ? "volume-mute" : player.volume < 0.5 ? "volume-quiet" : "volume-loud",
  );

  function onScrub(e: Event) {
    const v = Number((e.target as HTMLInputElement).value);
    player.seek(v);
  }
  function onVolume(e: Event) {
    player.setVolume(Number((e.target as HTMLInputElement).value));
  }
  // Desktop: icon toggles mute. Mini: icon opens the folded vertical slider.
  function onVolClick() {
    if (mini) volOpen = !volOpen;
    else player.toggleMute();
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

  async function bookmarkLiveEpisode() {
    const ep = player.liveEpisode?.episode;
    if (!ep) return;
    try {
      const fav = await api.toggleFavourite("episode", String(ep.id));
      player.setLiveEpisodeFavourite(fav);
    } catch (e) {
      favError = String(e);
    }
  }

  async function bookmarkLiveSong() {
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

{#snippet volumeControl()}
  <div class="p-volume" class:open={mini && volOpen}>
    <button
      class="pvol-btn"
      type="button"
      aria-label={mini ? "Volume" : player.muted ? "Unmute" : "Mute"}
      aria-expanded={mini ? volOpen : undefined}
      onclick={onVolClick}
      title={mini ? "Volume" : player.muted ? "Unmute" : "Mute"}
    >
      <Icon name={volumeIcon} size="24px" />
    </button>
    {#if !mini}
      <input type="range" min="0" max="1" step="0.02" value={player.volume} oninput={onVolume} />
    {:else if volOpen}
      <div
        class="vol-pop"
        style={`--volume-level: ${(player.muted ? 0 : player.volume) * 100}%`}
      >
        <div class="vol-slider">
          <span class="vol-track" aria-hidden="true">
            <span class="vol-fill"></span>
            <span class="vol-thumb"></span>
          </span>
          <input
            class="vol-v"
            aria-label="Volume"
            aria-orientation="vertical"
            type="range"
            min="0"
            max="1"
            step="0.02"
            value={player.volume}
            oninput={onVolume}
          />
        </div>
      </div>
    {/if}
  </div>
{/snippet}

<div class="app">
  <header
    class="player"
    class:inactive={!player.current && !player.live}
    class:volumeOpen={mini && volOpen}
    bind:this={headerEl}
  >
    <!-- svelte-ignore a11y_media_has_caption -->
    <audio bind:this={audioEl} preload="none"></audio>
    <div class="p-controls">
      <button class="pbtn" onclick={() => player.prevEpisode()} disabled={!player.current} title="Previous episode / restart"><Icon name="prev-ep" /></button>
      <button class="pbtn" onclick={() => player.prevTrack()} disabled={!player.tracks.length} title="Previous song"><Icon name="prev" /></button>
      <button class="pbtn skip" onclick={() => player.skip(-15)} disabled={!player.current} title="Back 15 seconds">«15</button>
      <button class="pbtn main" onclick={() => player.toggle()} disabled={!player.current && !player.live} title="Play/pause">
        {#if player.loading}…{:else if player.playing}<Icon name="playing" />{:else}<Icon name="play" />{/if}
      </button>
      <button class="pbtn skip" onclick={() => player.skip(15)} disabled={!player.current} title="Forward 15 seconds">15»</button>
      <button class="pbtn" onclick={() => player.nextTrack()} disabled={!player.tracks.length} title="Next song"><Icon name="next" /></button>
      <button class="pbtn" onclick={() => player.nextEpisode()} disabled={!player.current || player.queueIndex >= player.queue.length - 1} title="Next episode"><Icon name="next-ep" /></button>
    </div>
    <div class="p-info">
      {#if player.current}
        <div class="p-title">
          <button
            class="pfav"
            class:on={player.current.episode.favourite}
            onclick={bookmarkShow}
            title="Save this episode"
          ><Icon name="save" filled={player.current.episode.favourite} /></button>
          <a href={"/show/" + player.current.episode.show_id}>{player.current.showName}</a>
          <span class="p-date">{player.current.episode.air_date ?? ""}</span>
          {#if player.queue.length > 1}
            <span class="p-queue">{player.queueIndex + 1}/{player.queue.length}</span>
          {/if}
        </div>
        <div class="p-track">
          <button
            class="pfav"
            class:on={currentTrack?.favourite}
            onclick={bookmarkSong}
            disabled={!currentTrack}
            title={currentTrack ? "Star this song" : "No song info yet"}
          ><Icon name="star" filled={currentTrack?.favourite ?? false} /></button>
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
          {@render volumeControl()}
        </div>
      {:else if player.live}
        <div class="p-title">
          <button
            class="pfav"
            class:on={player.liveEpisode?.episode.favourite}
            onclick={bookmarkLiveEpisode}
            disabled={!player.liveEpisode}
            title="Save this live episode"
          ><Icon name="save" filled={player.liveEpisode?.episode.favourite ?? false} /></button>
          <span class="live-badge">● LIVE</span>
          {player.liveEpisode?.showName ?? player.live.name}
          {#if player.liveEpisode?.episode.air_date}
            <span class="p-date">{player.liveEpisode.episode.air_date}</span>
          {/if}
        </div>
        <div class="p-track">
          <button
            class="pfav"
            class:on={currentTrack?.favourite}
            onclick={bookmarkLiveSong}
            disabled={!currentTrack}
            title={currentTrack ? "Star this song" : "Song has not been persisted yet"}
          ><Icon name="star" filled={currentTrack?.favourite ?? false} /></button>
          {#if currentTrack}
            ♪ {currentTrack.artist ?? "?"} — {currentTrack.title ?? "?"}
          {:else if player.liveSong}
            ♪ {player.liveSong.artist ?? "?"} — {player.liveSong.title ?? "?"}
          {:else if player.error}
            <span class="err">{player.error}</span>
          {:else if player.livePlaylistError}
            <span class="err">{player.livePlaylistError}</span>
          {:else}
            {player.live.tagline}
          {/if}
        </div>
        <div class="p-scrub">
          <span class="p-time live-clock">{player.loading ? "Connecting…" : "Streaming live"}</span>
          <div class="live-fill"></div>
          {@render volumeControl()}
        </div>
      {:else}
        <div class="p-track idle">Nothing playing — pick a show.</div>
      {/if}
    </div>
  </header>

  <nav bind:this={navEl}>
    <a class="brand" href="/">
      <img class="brand-logo" src="/logo.gif" alt="Archiplayer" width="34" height="34" />
      <span class="brand-name">Archiplayer</span>
      <span class="brand-sub">WFMU</span>
    </a>
    <div class="nav-links">
      <a
        href="/"
        class:active={isActive("/")}
        title={isActive("/") && !collapsed ? "Collapse to player only" : "Shows"}
        onclick={(event) => onNavClick(event, "/")}
      >Shows</a>
      <a
        href="/profile"
        class:active={isActive("/profile")}
        title={isActive("/profile") && !collapsed ? "Collapse to player only" : "Profile"}
        onclick={(event) => onNavClick(event, "/profile")}
      >Profile</a>
    </div>
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

    /* Sizing tokens (presets). Change here = global; override locally on any element
       (inline style, modifier class, or <Icon size=…>) for per-place custom values. */
    --icon-size: 1.2em;      /* glyph size inside buttons; cascades into <Icon> */
    --pbtn-size: 32px;        /* transport secondary buttons (prev / next) */
    --pbtn-main-size: 46px;   /* transport main play/pause button */
    --pctl-gap: 8px;          /* gap between transport buttons */
    --player-gap: 18px;       /* gap between player sections */
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
    overflow: hidden; /* shell never scrolls — only <main> does */
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
    gap: var(--player-gap);
    padding: 10px 20px;
    background: var(--c-surface);
  }
  .player.inactive {
    opacity: 0.75;
  }
  .p-controls {
    display: flex;
    gap: var(--pctl-gap);
    align-items: center;
  }
  .pbtn {
    background: var(--c-surface2);
    color: var(--c-text);
    border: none;
    border-radius: 50%;
    width: var(--pbtn-size);
    height: var(--pbtn-size);
    cursor: pointer;
    font-size: 14px;
  }
  .pbtn.main {
    width: var(--pbtn-main-size);
    height: var(--pbtn-main-size);
    background: var(--c-accent);
    color: var(--c-on-accent);
    font-size: 18px;
  }
  /* Optical-centre the play/playing triangle in the round main button. */
  .pbtn.main :global(svg.icon) {
    transform: translateX(2px);
  }
  .pbtn.main :global(svg.icon) {
    width: 22px;
    height: 22px;
    transform: translateX(3px);   /* optical-centre the triangle */
  }
  .pbtn.skip {
    font-size: 12px;
    font-weight: 700;
    letter-spacing: -0.02em;
    font-variant-numeric: tabular-nums;
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
  .live-badge {
    color: var(--c-line);
    font-weight: 800;
    font-size: 11px;
    letter-spacing: 0.06em;
    margin-right: 6px;
    vertical-align: 1px;
    animation: live-blink 2s ease-in-out infinite;
  }
  @keyframes live-blink {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.45; }
  }
  .live-clock {
    min-width: auto;
    text-align: left;
    color: var(--c-accent);
    font-weight: 600;
  }
  .live-fill {
    flex: 1 1 auto;
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
  .pfav {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    padding: 2px 3px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    vertical-align: middle;
    flex: 0 0 auto;
  }
  .pfav:hover:not(:disabled) {
    color: var(--c-gold);
  }
  .pfav:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .pfav.on {
    color: var(--c-gold);
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
    margin-left: auto;
    flex-shrink: 0;
    color: var(--c-dim);
    position: relative;
  }
  .p-volume input {
    width: 90px;
    accent-color: var(--c-accent);
  }
  .pvol-btn {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    padding: 2px;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
  }
  .pvol-btn:hover {
    color: var(--c-accent);
  }

  /* Mini mode: below the point where the full transport + volume row overflows, stack the
     player into one column and scale key elements down ~25%, then stop. */
  @media (max-width: 760px) {
    .player {
      flex-direction: column;
      align-items: stretch;
      gap: 8px;
      padding: 8px 12px;
      /* Rescale via the sizing tokens — cascades through controls + icons. */
      --pbtn-size: 28px;
      --pbtn-main-size: 42px;
      --pctl-gap: 6px;
      --player-gap: 10px;
      --icon-size: 0.95em;
      --transport-width: 246px;
    }
    .p-controls {
      justify-content: center;
      flex-wrap: nowrap;
      width: min(100%, var(--transport-width));
      margin-inline: auto;
    }
    /* Hardcoded (non-token) sizes that won't follow the tokens. */
    .pbtn {
      font-size: 12px;
    }
    .pbtn.main {
      font-size: 14px;
    }
    .pbtn.main :global(svg.icon) {
      width: 20px;
      height: 20px;
    }
    .p-title {
      font-size: 12px;
    }
    .p-track {
      font-size: 11px;
    }
    /* Treat the compact player as one centred composition: transport, metadata,
       and scrubber share the same axis. */
    .p-info {
      text-align: center;
      position: relative;
      width: min(100%, var(--transport-width));
      margin-inline: auto;
    }
    .p-title,
    .p-track {
      width: 100%;
      text-align: center;
    }
    /* Shrink the «15 / 15» skip buttons so they stop overflowing. */
    .pbtn.skip {
      font-size: 9px;
      letter-spacing: -0.04em;
    }
    .p-scrub {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr) auto 28px;
      align-items: center;
      gap: 8px;
      position: relative;
      min-height: 28px;
      width: 100%;
    }
    .p-scrub > input[type="range"] {
      min-width: 0;
      width: 100%;
    }
    .p-scrub .p-volume {
      margin-left: 0;
      grid-column: 4;
      justify-self: end;
      min-width: 28px;
    }
    .player.volumeOpen .p-scrub {
      padding-right: 0;
    }
    .p-time {
      font-size: 11px;
      min-width: 40px;
    }
    /* Mini volume is custom-painted; the transparent native range on top keeps
       pointer and keyboard behavior without relying on WebView range styling. */
    .vol-pop {
      position: absolute;
      right: -4px;
      bottom: -4px;
      width: 36px;
      height: 108px;
      display: flex;
      justify-content: center;
      align-items: flex-start;
      padding: 7px 5px 34px;
      background: var(--c-surface);
      border: 1px solid var(--c-border);
      border-radius: 7px;
      z-index: 1;
    }
    .p-volume.open {
      z-index: 20;
    }
    .p-volume.open .pvol-btn {
      width: 28px;
      height: 28px;
      padding: 2px;
      justify-content: center;
      position: relative;
      z-index: 2;
    }
    .vol-slider {
      position: relative;
      width: 24px;
      height: 66px;
    }
    .vol-track {
      position: absolute;
      top: 6px;
      bottom: 6px;
      left: 50%;
      width: 5px;
      overflow: visible;
      transform: translateX(-50%);
      background: var(--c-border);
      border-radius: 999px;
      pointer-events: none;
    }
    .vol-fill {
      position: absolute;
      right: 0;
      bottom: 0;
      left: 0;
      height: var(--volume-level);
      background: var(--c-accent);
      border-radius: inherit;
    }
    .vol-thumb {
      position: absolute;
      bottom: var(--volume-level);
      left: 50%;
      width: 13px;
      height: 13px;
      transform: translate(-50%, 50%);
      background: var(--c-accent);
      border: 2px solid var(--c-surface);
      border-radius: 50%;
      box-shadow: 0 0 0 1px var(--c-border);
    }
    .p-volume .vol-v {
      position: absolute;
      top: 50%;
      left: 50%;
      z-index: 2;
      width: 66px;
      height: 24px;
      margin: 0;
      opacity: 0;
      transform: translate(-50%, -50%) rotate(-90deg);
      cursor: pointer;
      touch-action: none;
    }
    .vol-slider:focus-within .vol-track {
      box-shadow: 0 0 0 2px var(--c-surface), 0 0 0 4px var(--c-accent);
    }

    /* Compact the nav so the tabs stay readable. */
    nav {
      gap: 12px;
      padding: 8px 12px;
    }
    .brand {
      gap: 6px;
    }
    .brand-logo {
      width: 26px;
      height: 26px;
    }
    .brand-name {
      font-size: 13px;
    }
    .brand-sub {
      display: none;
    }
    .nav-links {
      gap: 10px;
      margin-left: 0;
      flex: 1 1 auto;
      justify-content: space-evenly;
    }
    .nav-links a {
      padding: 3px 8px;
    }
  }
  @media (max-width: 420px) {
    .player {
      gap: 14px;
      padding: 18px 12px 8px;
      --pbtn-size: 32px;
      --pbtn-main-size: 46px;
      --pctl-gap: 6px;
      --transport-width: 274px;
    }
    .player.inactive {
      gap: 8px;
      padding-top: 10px;
    }
    .pbtn.main :global(svg.icon) {
      width: 22px;
      height: 22px;
    }
    .p-title {
      margin-bottom: 3px;
    }
    .p-track {
      margin-bottom: 3px;
    }
    .p-volume.open {
      min-width: 28px;
    }
    .p-volume.open .vol-pop {
      height: 130px;
      padding-top: 7px;
    }
    .p-volume.open .vol-slider {
      height: 88px;
    }
    .p-volume.open .vol-v {
      width: 84px;
    }
    .brand-name,
    .brand-sub {
      display: none;
    }
    nav {
      gap: 44px;
      padding: 10px 18px;
    }
    .brand-logo {
      width: 32px;
      height: 32px;
    }
    .nav-links {
      flex: 0 0 auto;
      gap: 44px;
      justify-content: flex-start;
    }
  }
</style>
