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
  // Player-only collapse shortens the window to the player + tabs. The routed
  // content remains mounted underneath, so manually enlarging the window reveals it.
  // Width remains user-controlled so the responsive player can still be resized.
  let collapsed = $state(false);
  let navEl: HTMLElement;
  let headerEl: HTMLElement;
  let expandedHeight = 0; // remembered logical height to restore on expand
  // Mirrors the CSS mini-mode breakpoint so volume can fold to a popover.
  let mini = $state(false);
  let volOpen = $state(false);
  let applyingWindowSize = false;
  let windowSizeQueued = false;
  let collapseTransitioning = false;

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
    if (applyingWindowSize) {
      // ResizeObserver can fire while a native setSize call is still resolving.
      // Keep one trailing measurement instead of losing the final reflow.
      windowSizeQueued = true;
      return;
    }
    applyingWindowSize = true;
    try {
      const win = getCurrentWindow();
      const scale = await win.scaleFactor();
      const physical = await win.innerSize();
      if (collapsed) {
        // Collapse only the library area. Preserve the current width so users can
        // keep resizing the compact player through its responsive breakpoints.
        const compact = Math.ceil((headerEl?.offsetHeight ?? 0) + (navEl?.offsetHeight ?? 0));
        const height = Math.round(compact * scale);
        if (compact > 0 && Math.abs(physical.height - height) > 1) {
          await win.setSize(new PhysicalSize(physical.width, height));
        }
      } else {
        // Restore only the pre-collapse height. A restored collapsed session has
        // no in-memory height, so use a comfortable library default.
        const targetHeight = expandedHeight > 0 ? expandedHeight : 860;
        const height = Math.round(targetHeight * scale);
        if (Math.abs(physical.height - height) > 1) {
          await win.setSize(new PhysicalSize(physical.width, height));
        }
      }
    } catch {
      /* not in Tauri / no window perms — button still hides the library */
    } finally {
      applyingWindowSize = false;
      if (windowSizeQueued) {
        windowSizeQueued = false;
        void applyWindowSize();
      }
    }
  }

  async function toggleCollapse() {
    if (collapseTransitioning) return;
    collapseTransitioning = true;
    try {
      if (!collapsed) {
        // About to collapse: remember only the height. Width stays user-controlled.
        try {
          const win = getCurrentWindow();
          const scale = await win.scaleFactor();
          const size = (await win.innerSize()).toLogical(scale);
          expandedHeight = size.height;
        } catch {
          /* ignore */
        }
      }
      collapsed = !collapsed;
      localStorage.setItem("ap.collapsed", collapsed ? "1" : "0");
      await tick();
      await applyWindowSize();
    } finally {
      collapseTransitioning = false;
    }
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

  <main>
    {@render children()}
  </main>
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

  /* Compact mode keeps one stacked composition and scales it continuously from the
     roomy 760px endpoint down to the native 324px minimum window width. */
  @media (max-width: 760px) {
    .player {
      flex-direction: column;
      align-items: stretch;
      gap: var(--mini-player-gap);
      padding: var(--mini-pad-top) var(--mini-pad-inline) var(--mini-pad-bottom);

      /* Each clamp is linear between 324px and 760px. Keeping the endpoints here
         makes the intended minimum and intermediate compositions explicit. */
      --pbtn-size: clamp(32px, calc(29.028px + 0.917vw), 36px);
      --pbtn-main-size: clamp(46px, calc(41.541px + 1.376vw), 52px);
      --pctl-gap: clamp(6px, calc(4.514px + 0.459vw), 8px);
      --player-gap: clamp(14px, calc(11.028px + 0.917vw), 18px);
      --icon-size: clamp(11.4px, calc(8.428px + 0.917vw), 15.4px);
      --transport-width: clamp(274px, calc(232.385px + 12.844vw), 330px);
      --mini-player-gap: var(--player-gap);
      --mini-pad-top: clamp(18px, calc(16.514px + 0.459vw), 20px);
      --mini-pad-inline: clamp(12px, calc(7.541px + 1.376vw), 18px);
      --mini-pad-bottom: clamp(8px, calc(5.028px + 0.917vw), 12px);
      --mini-control-font: clamp(12px, calc(10.514px + 0.459vw), 14px);
      --mini-main-font: clamp(14px, calc(12.514px + 0.459vw), 16px);
      --mini-main-icon: clamp(22px, calc(19.028px + 0.917vw), 26px);
      --mini-title-font: clamp(12px, calc(10.514px + 0.459vw), 14px);
      --mini-track-font: clamp(11px, calc(9.514px + 0.459vw), 13px);
      --mini-meta-margin: clamp(3px, calc(2.257px + 0.229vw), 4px);
      --mini-skip-font: clamp(9px, calc(7.514px + 0.459vw), 11px);
      --mini-scrub-gap: clamp(8px, calc(6.514px + 0.459vw), 10px);
      --mini-scrub-height: clamp(28px, calc(25.028px + 0.917vw), 32px);
      --mini-volume-column: clamp(28px, calc(25.028px + 0.917vw), 32px);
      --mini-time-font: clamp(11px, calc(10.257px + 0.229vw), 12px);
      --mini-time-width: clamp(40px, calc(34.055px + 1.835vw), 48px);
      --mini-volume-width: clamp(36px, calc(33.028px + 0.917vw), 40px);
      --mini-volume-height: clamp(130px, calc(110.679px + 5.963vw), 156px);
      --mini-volume-pad-top: clamp(7px, calc(6.257px + 0.229vw), 8px);
      --mini-volume-pad-inline: clamp(5px, calc(4.257px + 0.229vw), 6px);
      --mini-volume-pad-bottom: clamp(34px, calc(31.028px + 0.917vw), 38px);
      --mini-volume-button: var(--mini-volume-column);
      --mini-volume-slider: clamp(88px, calc(73.138px + 4.587vw), 108px);
      --mini-volume-track-inset: clamp(6px, calc(4.514px + 0.459vw), 8px);
      --mini-volume-track: clamp(5px, calc(4.257px + 0.229vw), 6px);
      --mini-volume-thumb: clamp(13px, calc(11.514px + 0.459vw), 15px);
      --mini-volume-input: clamp(84px, calc(69.138px + 4.587vw), 104px);
    }
    .player.inactive {
      gap: clamp(8px, calc(5.028px + 0.917vw), 12px);
      padding-top: clamp(10px, calc(7.028px + 0.917vw), 14px);
    }
    .p-controls {
      justify-content: center;
      flex-wrap: nowrap;
      width: min(100%, var(--transport-width));
      margin-inline: auto;
    }
    /* Hardcoded (non-token) sizes that won't follow the tokens. */
    .pbtn {
      font-size: var(--mini-control-font);
    }
    .pbtn.main {
      font-size: var(--mini-main-font);
    }
    .pbtn.main :global(svg.icon) {
      width: var(--mini-main-icon);
      height: var(--mini-main-icon);
    }
    .p-title {
      margin-bottom: var(--mini-meta-margin);
      font-size: var(--mini-title-font);
    }
    .p-track {
      margin-bottom: var(--mini-meta-margin);
      font-size: var(--mini-track-font);
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
      font-size: var(--mini-skip-font);
      letter-spacing: -0.04em;
    }
    .p-scrub {
      display: grid;
      grid-template-columns: auto minmax(0, 1fr) auto var(--mini-volume-column);
      align-items: center;
      gap: var(--mini-scrub-gap);
      position: relative;
      min-height: var(--mini-scrub-height);
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
      min-width: var(--mini-volume-column);
    }
    .player.volumeOpen .p-scrub {
      padding-right: 0;
    }
    .p-time {
      font-size: var(--mini-time-font);
      min-width: var(--mini-time-width);
    }
    /* Mini volume is custom-painted; the transparent native range on top keeps
       pointer and keyboard behavior without relying on WebView range styling. */
    .vol-pop {
      position: absolute;
      right: -4px;
      bottom: -4px;
      width: var(--mini-volume-width);
      height: var(--mini-volume-height);
      display: flex;
      justify-content: center;
      align-items: flex-start;
      padding: var(--mini-volume-pad-top) var(--mini-volume-pad-inline)
        var(--mini-volume-pad-bottom);
      background: var(--c-surface);
      border: 1px solid var(--c-border);
      border-radius: 7px;
      z-index: 1;
    }
    .p-volume.open {
      z-index: 20;
    }
    .p-volume.open .pvol-btn {
      width: var(--mini-volume-button);
      height: var(--mini-volume-button);
      padding: 2px;
      justify-content: center;
      position: relative;
      z-index: 2;
    }
    .vol-slider {
      position: relative;
      width: 24px;
      height: var(--mini-volume-slider);
    }
    .vol-track {
      position: absolute;
      top: var(--mini-volume-track-inset);
      bottom: var(--mini-volume-track-inset);
      left: 50%;
      width: var(--mini-volume-track);
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
      width: var(--mini-volume-thumb);
      height: var(--mini-volume-thumb);
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
      width: var(--mini-volume-input);
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
    nav {
      gap: 8px;
      padding: 8px;
    }
    .brand {
      gap: 4px;
    }
    .brand-logo {
      width: 24px;
      height: 24px;
    }
    .brand-name {
      display: inline;
      font-size: 12px;
    }
    .brand-sub {
      display: inline;
      font-size: 10px;
    }
    .nav-links {
      min-width: 0;
      gap: 4px;
      flex: 1 1 auto;
      justify-content: space-evenly;
    }
    .nav-links a {
      padding: 3px 5px;
      font-size: 13px;
    }
  }
</style>
