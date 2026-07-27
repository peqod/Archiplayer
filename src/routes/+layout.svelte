<script lang="ts">
  import "@fontsource/inter/400.css";
  import "@fontsource/inter/600.css";
  import "@fontsource/inter/700.css";
  import "@fontsource/inter/800.css";
  import { player, type QueueItem } from "$lib/player.svelte";
  import { VOLUME_TICKS } from "$lib/volume";
  import { LOUPE_SIZE, LOUPE_ZOOM, gearedTime, timeAtPointer } from "$lib/seek-drag";
  import { scrollCueVisible } from "$lib/scroll-cue";
  import { shortcuts, type ActionHandlers } from "$lib/shortcuts.svelte";
  import { matchesAccel, shouldIgnoreKey } from "$lib/shortcuts";
  import { api, fmtTime } from "$lib/api";
  import { selectRandomPlayback } from "$lib/random-show";
  import { theme } from "$lib/theme.svelte";
  import Icon from "$lib/Icon.svelte";
  import { playGlyph } from "$lib/play-icon";
  import Toast from "$lib/Toast.svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { onMount, tick } from "svelte";
  import { getCurrentWindow, PhysicalSize } from "@tauri-apps/api/window";
  import { openUrl } from "@tauri-apps/plugin-opener";

  let { children } = $props();
  let audioEl: HTMLAudioElement;
  let favError = $state<string | null>(null);
  let luckyBusy = $state(false);

  // Same action the catalog dice fires: random show + random starting episode.
  async function feelingLucky() {
    if (luckyBusy) return;
    luckyBusy = true;
    try {
      const shows = await api.getCatalog();
      if (!shows.length) return;
      const selection = await selectRandomPlayback(
        shows,
        (show) => api.getShow(show.id),
        player.current?.episode.show_id ?? null,
        player.current?.episode.id ?? null,
      );
      if (!selection) return;
      const items: QueueItem[] = selection.episodes.map((episode) => ({
        episode,
        showName: selection.show.name,
      }));
      await goto("/show/" + selection.show.id, {
        state: { centerEpisodeId: selection.episodes[selection.index].id },
      });
      // In minimal (collapsed) view, size the window for playback before the header expands.
      if (collapsed) await growForPlayback();
      await player.playQueue(items, selection.index);
    } catch (err) {
      favError = String(err);
    } finally {
      luckyBusy = false;
    }
  }
  // Player-only collapse shrinks the window to the minimum-width player + tabs.
  // The routed content remains mounted underneath, and the exact pre-collapse
  // client size is restored when the shell is expanded again.
  const COMPACT_WIDTH = 324;
  let collapsed = $state(false);
  let navEl: HTMLElement;
  let headerEl: HTMLElement;
  let mainEl: HTMLElement;
  let expandedWidth = 0;
  let expandedHeight = 0;
  // Last measured collapsed height while a track was loaded. Lets feelingLucky pre-grow
  // the window to the playing size before the header expands, so it never clips.
  let playingCompactHeight = 0;
  // Mirrors the CSS mini-mode breakpoint so volume can fold to a popover.
  let mini = $state(false);
  let volOpen = $state(false);
  // Grace period before an un-hovered mini volume popover folds away.
  const VOL_CLOSE_DELAY = 1000;
  let volCloseTimer: ReturnType<typeof setTimeout> | null = null;
  let miniMedia: MediaQueryList | null = null;
  // Loupe scrubbing. `dragSec` being non-null is what raises the lens, and it is the
  // authoritative position while it is up: the geared value accumulates from the press
  // point, so it must not be re-derived from the element's clock.
  let seekTrackEl = $state<HTMLElement | null>(null);
  let seekTrackW = $state(0);
  let dragSec = $state<number | null>(null);
  let dragAnchorSec = 0;
  let dragAnchorX = 0;
  let dragTrackW = 0;
  // Brand cue: <main> is scrolled far enough that getting back to the top is worth
  // offering. See scroll-cue.ts for where the threshold comes from.
  let scrollCued = $state(false);
  let applyingWindowSize = false;
  let windowSizeQueued = false;
  let collapseTransitioning = false;

  if (typeof window !== "undefined") {
    theme.load();
    shortcuts.load();
    collapsed = localStorage.getItem("ap.collapsed") === "1";
    miniMedia = window.matchMedia("(max-width: 760px)");
    mini = miniMedia.matches;
  }

  onMount(() => {
    if (!miniMedia) return;
    const onMiniChange = (e: MediaQueryListEvent) => {
      mini = e.matches;
      cancelVolClose();
      volOpen = false; // leaving/entering mini closes any open volume popover
    };
    miniMedia.addEventListener("change", onMiniChange);
    return () => {
      miniMedia?.removeEventListener("change", onMiniChange);
      cancelVolClose();
    };
  });

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
        // Tauri setSize targets the client area. Resize to the native minimum
        // width and the measured player + tab height.
        const compact = Math.ceil((headerEl?.offsetHeight ?? 0) + (navEl?.offsetHeight ?? 0));
        // Remember the collapsed height while playing so feelingLucky can pre-size to it.
        if (compact > 0 && (player.current || player.live)) playingCompactHeight = compact;
        const width = Math.round(COMPACT_WIDTH * scale);
        const height = Math.round(compact * scale);
        if (
          compact > 0 &&
          (Math.abs(physical.width - width) > 1 || Math.abs(physical.height - height) > 1)
        ) {
          await win.setSize(new PhysicalSize(width, height));
        }
      } else {
        // Restore the exact pre-collapse client size. A restored collapsed session
        // has no in-memory size, so use the configured default window dimensions.
        const targetWidth = expandedWidth > 0 ? expandedWidth : 1280;
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
      if (windowSizeQueued) {
        windowSizeQueued = false;
        void applyWindowSize();
      }
    }
  }

  // Collapsed idle -> playing grows the header from one row to three synchronously, but the
  // window resize is async. Pre-grow the window to the playing height first so the header
  // expands into it (no nav clip / transport shift); the ResizeObserver then settles the exact
  // height. Grows only — the observer handles any final shrink.
  async function growForPlayback() {
    if (!collapsed) return;
    try {
      const win = getCurrentWindow();
      const scale = await win.scaleFactor();
      const physical = await win.innerSize();
      const targetLogical = playingCompactHeight > 0 ? playingCompactHeight : 200;
      const width = Math.round(COMPACT_WIDTH * scale);
      const height = Math.round(targetLogical * scale);
      if (height > physical.height + 1) {
        await win.setSize(new PhysicalSize(width, height));
      }
    } catch {
      /* not in Tauri / no window perms — the header still expands, just without the pre-grow */
    }
  }

  async function toggleCollapse() {
    if (collapseTransitioning) return;
    collapseTransitioning = true;
    try {
      if (!collapsed) {
        // About to collapse: remember the complete client size for restoration.
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

  // The brand is a scroll-to-top control, not a link. The shell never scrolls, so
  // this has to target <main>; the Shows tab remains the way home.
  function scrollMainToTop() {
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    mainEl?.scrollTo({ top: 0, behavior: reducedMotion ? "auto" : "smooth" });
  }

  // Past the threshold the brand drops the wordmark and says what pressing it does,
  // on the same scroll travel that hands the "← All shows" link over to its pill.
  function measureMainScroll() {
    scrollCued = scrollCueVisible(mainEl?.scrollTop ?? 0);
  }

  function supportWfmu(e: MouseEvent) {
    e.preventDefault();
    openUrl("https://pledge.wfmu.org/donate").catch(() => {});
  }

  $effect(() => {
    if (audioEl) player.attach(audioEl);
  });

  // A drag cannot outlive the bar it started on. An episode ending mid-hold clears the
  // duration, which disables the input, so no pointerup would ever arrive to put the
  // loupe down, and a stuck loupe silently swallows every arrow key after it.
  $effect(() => {
    if (!player.duration) dragSec = null;
  });

  // <main> keeps its scroll position across a client-side navigation, so a route with
  // shorter content lands already scrolled and gets clamped by the browser. That clamp
  // is not reliably an event we hear, so re-read the position once the new page paints.
  $effect(() => {
    $page.url.pathname;
    void tick().then(measureMainScroll);
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
  const playPauseLabel = $derived(
    player.loading ? "Loading audio" : player.playing ? "Pause" : "Play",
  );
  // A held loupe outranks the element's clock, so the bar tracks the pointer rather
  // than the four-a-second `timeupdate`.
  const displaySec = $derived(dragSec ?? player.currentTime);
  // Painted-slider fill/thumb offsets. Both sliders read them from --hsl-level.
  const seekLevel = $derived(
    player.duration ? `${(displaySec / player.duration) * 100}%` : "0%",
  );
  // Unitless twin of --hsl-level, so the lens can multiply it by the track width in calc().
  const seekFrac = $derived(player.duration ? displaySec / player.duration : 0);
  const volumeLevel = $derived(`${(player.muted ? 0 : player.volume) * 100}%`);
  // Playlist marks as track-relative percentages; empty for untimecoded playlists.
  const seekMarks = $derived(
    player.duration ? player.trackMarks.map((sec) => (sec / player.duration) * 100) : [],
  );

  function onScrub(e: Event) {
    const input = e.target as HTMLInputElement;
    // While the loupe is up the geared value is the truth, so an input event here can
    // only be the native slider tracking the pointer after all. Put its value back
    // rather than ignoring it: Svelte rewrites `value` only when the expression
    // changes, so a value left diverged would still be there for the next arrow key.
    if (dragSec !== null) {
      input.value = String(Math.round(dragSec));
      return;
    }
    player.seek(Number(input.value));
  }

  function onSeekDown(e: PointerEvent) {
    // Mini keeps the plain native drag: the row is tight and the thumb is already
    // clamped down there. A second contact must not re-anchor a live drag either.
    if (mini || !e.isPrimary || e.button !== 0 || !player.duration || !seekTrackEl) return;
    // Stop the range from tracking the pointer itself. Preventing the default on
    // pointerdown suppresses the compatibility mouse events the native slider drags
    // on, which also means focus has to be moved by hand.
    e.preventDefault();
    const rect = seekTrackEl.getBoundingClientRect();
    dragTrackW = rect.width;
    dragAnchorX = e.clientX;
    dragAnchorSec = timeAtPointer(e.clientX, rect.left, rect.width, player.duration);
    dragSec = dragAnchorSec;
    player.seek(dragAnchorSec);
    // Last, because setPointerCapture throws on a pointer that is already gone. Raising
    // the loupe first means such a throw costs the capture, not the whole press.
    const input = e.currentTarget as HTMLInputElement;
    input.focus({ preventScroll: true });
    input.setPointerCapture(e.pointerId);
  }

  function onSeekMove(e: PointerEvent) {
    if (dragSec === null) return;
    dragSec = gearedTime(dragAnchorSec, dragAnchorX, e.clientX, dragTrackW, player.duration);
    player.seek(dragSec);
  }

  // Every exit funnels here, and it has to be safe to run twice: releasing the capture
  // fires lostpointercapture straight after the pointerup that asked for it.
  function endSeek() {
    if (dragSec === null) return;
    player.seek(dragSec);
    dragSec = null;
  }

  function onSeekUp(e: PointerEvent) {
    const input = e.currentTarget as HTMLInputElement;
    if (input.hasPointerCapture(e.pointerId)) input.releasePointerCapture(e.pointerId);
    endSeek();
  }
  function onVolume(e: Event) {
    player.setVolume(Number((e.target as HTMLInputElement).value));
  }
  // Desktop: icon toggles mute. Mini: icon opens the folded vertical slider.
  function onVolClick() {
    cancelVolClose();
    if (mini) volOpen = !volOpen;
    else player.toggleMute();
  }

  function cancelVolClose() {
    if (volCloseTimer === null) return;
    clearTimeout(volCloseTimer);
    volCloseTimer = null;
  }

  // The mini popover overlays a window only a few hundred pixels wide, so it folds
  // itself away once the pointer leaves rather than sitting open. Keyboard focus
  // inside keeps it up, and touch (no mouseleave) still closes by tapping the icon.
  // Only two things may hold the popover open once the pointer is gone: a keyboard
  // focus ring inside the panel, or a slider drag still in progress. Plain focus is
  // not enough — clicking the toggle, or the slider, leaves focus sitting there.
  function volHeldOpen(panel: Element | null): boolean {
    const active = document.activeElement;
    if (!panel || !active || !panel.contains(active)) return false;
    return active.matches(":focus-visible") || active.matches(":active");
  }

  function scheduleVolClose(panel: Element | null) {
    cancelVolClose();
    volCloseTimer = setTimeout(() => {
      volCloseTimer = null;
      // Re-arm instead of giving up: a drag that outlives the delay ends with the
      // pointer already outside, so no second mouseleave would ever arrive.
      if (volHeldOpen(panel)) scheduleVolClose(panel);
      else volOpen = false;
    }, VOL_CLOSE_DELAY);
  }

  function onVolLeave(event: MouseEvent) {
    scheduleVolClose((event.currentTarget as HTMLElement).querySelector(".vol-pop"));
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

  // The one place a bound key turns into an act. Both tiers of shortcut and the OS
  // media controls come through here, so a rebind never means new behaviour.
  const actionHandlers: ActionHandlers = {
    "play-pause": () => player.toggle(),
    "prev-track": () => player.prevTrack(),
    "next-track": () => player.nextTrack(),
    "prev-episode": () => void player.prevEpisode(),
    "next-episode": () => void player.nextEpisode(),
    mute: () => player.toggleMute(),
    // Live has its own favourite targets, so one binding covers both modes.
    "fav-song": () => void (player.live ? bookmarkLiveSong() : bookmarkSong()),
    "save-show": () => void (player.live ? bookmarkLiveEpisode() : bookmarkShow()),
    random: () => void feelingLucky(),
  };

  onMount(() => shortcuts.attach(actionHandlers));

  function onKey(e: KeyboardEvent) {
    if (!shortcuts.enabled || shortcuts.recording) return;
    // Held keys would step a song per repeat, and a raised loupe owns the keyboard.
    if (e.defaultPrevented || e.repeat || dragSec !== null) return;
    for (const [id, accel] of shortcuts.localBindings()) {
      if (!matchesAccel(e, accel)) continue;
      // The focused control keeps the key: typing in search must stay typing.
      if (shouldIgnoreKey(e.target, accel)) return;
      e.preventDefault();
      shortcuts.run(id);
      return;
    }
  }

  // What Windows shows in the volume flyout and on the lock screen, and where the
  // hardware media keys land. Chromium wires this to the OS for us, which is why the
  // transport keys need no registration of their own.
  $effect(() => {
    if (typeof navigator === "undefined" || !("mediaSession" in navigator)) return;
    const ms = navigator.mediaSession;
    const cur = player.current;
    const live = player.live;
    if (!cur && !live) {
      ms.metadata = null;
      ms.playbackState = "none";
      return;
    }
    const song = player.live ? player.liveSong : null;
    const track = currentTrack;
    const artist = song?.artist ?? track?.artist ?? null;
    const songTitle = song?.title ?? track?.title ?? null;
    const showName = cur?.showName ?? player.liveEpisode?.showName ?? live?.name ?? "WFMU";
    ms.metadata = new MediaMetadata({
      // The song is the headline once one is known; the show is what is on otherwise.
      title: songTitle ?? cur?.episode.title ?? showName,
      artist: artist ?? showName,
      album: songTitle ? showName : (cur?.episode.air_date ?? live?.tagline ?? ""),
      artwork: [{ src: "/favicon.png", sizes: "512x512", type: "image/png" }],
    });
    ms.playbackState = player.playing ? "playing" : "paused";
  });

  // Position only exists for the archive: a live stream has no timeline to report.
  $effect(() => {
    if (typeof navigator === "undefined" || !("mediaSession" in navigator)) return;
    const ms = navigator.mediaSession;
    if (!ms.setPositionState) return;
    if (!player.current || !player.duration || !Number.isFinite(player.duration)) {
      ms.setPositionState();
      return;
    }
    ms.setPositionState({
      duration: player.duration,
      playbackRate: 1,
      position: Math.min(player.currentTime, player.duration),
    });
  });

  onMount(() => {
    if (typeof navigator === "undefined" || !("mediaSession" in navigator)) return;
    const ms = navigator.mediaSession;
    const bound: [MediaSessionAction, MediaSessionActionHandler][] = [
      ["play", () => player.toggle()],
      ["pause", () => player.toggle()],
      ["stop", () => player.stop()],
      ["previoustrack", () => player.prevTrack()],
      ["nexttrack", () => player.nextTrack()],
      ["seekbackward", () => player.skip(-15)],
      ["seekforward", () => player.skip(15)],
      ["seekto", (details) => {
        if (typeof details.seekTime === "number") player.seek(details.seekTime);
      }],
    ];
    for (const [action, handler] of bound) {
      // Not every action is supported by every build; an unsupported one throws.
      try {
        ms.setActionHandler(action, handler);
      } catch {
        /* ignore */
      }
    }
    return () => {
      for (const [action] of bound) {
        try {
          ms.setActionHandler(action, null);
        } catch {
          /* ignore */
        }
      }
    };
  });
</script>

<svelte:window onkeydown={onKey} />

{#snippet volumeControl()}
  <div
    class="p-volume"
    class:open={mini && volOpen}
    onmouseenter={cancelVolClose}
    onmouseleave={onVolLeave}
    role="presentation"
  >
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
      <div class="hsl vol-h" style={`--hsl-level: ${volumeLevel}`}>
        <input
          class="sl-input"
          aria-label="Volume"
          type="range"
          min="0"
          max="1"
          step="0.02"
          value={player.volume}
          oninput={onVolume}
        />
        <span class="hsl-track" aria-hidden="true">
          <span class="hsl-fill"></span>
          {#each VOLUME_TICKS as tick (tick.pct)}
            <span class="hsl-tick" style={`left: ${tick.pct}%; width: ${tick.size / 2}px`}></span>
          {/each}
          <span class="hsl-thumb"></span>
        </span>
      </div>
    {:else}
      <!-- Stays mounted so opacity can transition; visibility keeps it out of the
           tab order and away from the pointer while folded. -->
      <div
        class="vol-pop"
        style={`--volume-level: ${volumeLevel}`}
      >
        <div class="vol-slider">
          <input
            class="vol-v sl-input"
            aria-label="Volume"
            aria-orientation="vertical"
            type="range"
            min="0"
            max="1"
            step="0.02"
            value={player.volume}
            oninput={onVolume}
          />
          <span class="vol-track" aria-hidden="true">
            <span class="vol-fill"></span>
            {#each VOLUME_TICKS as tick (tick.pct)}
              <span class="vol-tick" style={`bottom: ${tick.pct}%; height: ${tick.size / 2}px`}></span>
            {/each}
            <span class="vol-thumb"></span>
          </span>
        </div>
      </div>
    {/if}
  </div>
{/snippet}

<div class="app" class:collapsed={collapsed}>
  <a class="skip-link" href="#main-content">Skip to content</a>
  <header
    class="player"
    class:inactive={!player.current && !player.live}
    bind:this={headerEl}
  >
    <!-- svelte-ignore a11y_media_has_caption -->
    <audio bind:this={audioEl} preload="none"></audio>
    <div class="p-controls">
      <button class="pbtn" onclick={() => player.prevEpisode()} disabled={!player.current} aria-label="Previous episode or restart" title="Previous episode / restart"><Icon name="prev-ep" /></button>
      <button class="pbtn" onclick={() => player.prevTrack()} disabled={!player.tracks.length} aria-label="Previous song" title="Previous song"><Icon name="prev" /></button>
      <button class="pbtn skip" onclick={() => player.skip(-15)} disabled={!player.current} aria-label="Back 15 seconds" title="Back 15 seconds">«15</button>
      <button class="pbtn main" onclick={() => player.toggle()} disabled={!player.current && !player.live} aria-label={playPauseLabel} title={playPauseLabel}>
        {#if player.loading}…{:else}<Icon name={playGlyph(!!player.current || !!player.live, player.playing)} size="22px" />{/if}
      </button>
      <button class="pbtn skip" onclick={() => player.skip(15)} disabled={!player.current} aria-label="Forward 15 seconds" title="Forward 15 seconds">15»</button>
      <button class="pbtn" onclick={() => player.nextTrack()} disabled={!player.tracks.length} aria-label="Next song" title="Next song"><Icon name="next" /></button>
      <button class="pbtn" onclick={() => player.nextEpisode()} disabled={!player.current || player.queueIndex >= player.queue.length - 1} aria-label="Next episode" title="Next episode"><Icon name="next-ep" /></button>
    </div>
    <div class="p-info">
      {#if player.current}
        <div class="p-title ellipsis">
          <button
            class="pfav"
            class:on={player.current.episode.favourite}
            onclick={bookmarkShow}
            aria-label={player.current.episode.favourite ? "Remove episode from favourites" : "Save this episode"}
            aria-pressed={player.current.episode.favourite}
            title="Save this episode"
          ><Icon name="save" filled={player.current.episode.favourite} /></button>
          <a href={"/show/" + player.current.episode.show_id}>{player.current.showName}</a>
          <span class="p-date">{player.current.episode.air_date ?? ""}</span>
          {#if player.queue.length > 1}
            <span class="p-queue">{player.queueIndex + 1}/{player.queue.length}</span>
          {/if}
        </div>
        <div class="p-track ellipsis">
          <button
            class="pfav"
            class:on={currentTrack?.favourite}
            onclick={bookmarkSong}
            disabled={!currentTrack}
            aria-label={currentTrack?.favourite ? "Remove song from favourites" : "Star this song"}
            aria-pressed={currentTrack?.favourite ?? false}
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
          <span class="p-time">{fmtTime(displaySec)}</span>
          <div
            class="hsl seek"
            class:off={!player.duration}
            class:lensing={dragSec !== null}
            style={`--hsl-level: ${seekLevel}; --seek-n: ${seekFrac}; --track-w: ${seekTrackW}px; --lens-zoom: ${LOUPE_ZOOM}; --lens-size: ${LOUPE_SIZE}px`}
          >
            <!-- Input first so the paint can key its focus ring off it with `~`. It is
                 transparent and the paint ignores the pointer, so order costs nothing. -->
            <input
              class="sl-input"
              type="range"
              min="0"
              max={player.duration || 0}
              step="1"
              value={Math.round(displaySec)}
              oninput={onScrub}
              onpointerdown={onSeekDown}
              onpointermove={onSeekMove}
              onpointerup={onSeekUp}
              onpointercancel={onSeekUp}
              onlostpointercapture={endSeek}
              disabled={!player.duration}
              aria-label="Episode position"
            />
            <span
              class="hsl-track"
              aria-hidden="true"
              bind:this={seekTrackEl}
              bind:clientWidth={seekTrackW}
            >
              <span class="hsl-fill"></span>
              {#each seekMarks as pct (pct)}
                <span class="hsl-tick mark" style={`left: ${pct}%`}></span>
              {/each}
              <span class="hsl-thumb"></span>
              {#if dragSec !== null}
                <!-- The loupe: the same track, fill and notches re-rendered at zoom and
                     slid so the held position lands under the crosshair. -->
                <span class="seek-lens">
                  <span class="lens-strip">
                    <span class="hsl-fill"></span>
                    {#each seekMarks as pct (pct)}
                      <span class="hsl-tick mark" style={`left: ${pct}%`}></span>
                    {/each}
                  </span>
                  <span class="lens-cross"></span>
                </span>
              {/if}
            </span>
          </div>
          <span class="p-time">{fmtTime(player.duration)}</span>
          {@render volumeControl()}
        </div>
      {:else if player.live}
        <div class="p-title ellipsis">
          <button
            class="pfav"
            class:on={player.liveEpisode?.episode.favourite}
            onclick={bookmarkLiveEpisode}
            disabled={!player.liveEpisode}
            aria-label={player.liveEpisode?.episode.favourite ? "Remove live episode from favourites" : "Save this live episode"}
            aria-pressed={player.liveEpisode?.episode.favourite ?? false}
            title="Save this live episode"
          ><Icon name="save" filled={player.liveEpisode?.episode.favourite ?? false} /></button>
          <span class="live-badge">● LIVE</span>
          {player.liveEpisode?.showName ?? player.live.name}
          {#if player.liveEpisode?.episode.air_date}
            <span class="p-date">{player.liveEpisode.episode.air_date}</span>
          {/if}
        </div>
        <div class="p-track ellipsis">
          <button
            class="pfav"
            class:on={currentTrack?.favourite}
            onclick={bookmarkLiveSong}
            disabled={!currentTrack}
            aria-label={currentTrack?.favourite ? "Remove live song from favourites" : "Star this live song"}
            aria-pressed={currentTrack?.favourite ?? false}
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
        <div class="p-track idle">
          <button class="lucky" onclick={feelingLucky} disabled={luckyBusy}>
            {luckyBusy ? "Finding…" : "🎲 I'm feeling lucky today"}
          </button>
        </div>
      {/if}
    </div>
  </header>

  <nav bind:this={navEl}>
    <button
      class="brand"
      class:cued={scrollCued}
      type="button"
      onclick={scrollMainToTop}
      title="Scroll to top"
      aria-label="Scroll to top"
    >
      <!-- Both states of each half stay mounted and share one grid cell: the slot is
           as wide as its widest state, so the swap crossfades in place instead of
           reflowing the tabs beside it. aria-hidden throughout — the button's own
           label is what gets announced, and it says the same thing either way. -->
      <span class="brand-slot brand-mark" aria-hidden="true">
        <img class="brand-logo" src="/logo.gif" alt="" width="34" height="34" />
        <span class="brand-arrow"><Icon name="arrow-up" size="calc(var(--brand-mark) * 0.58)" /></span>
      </span>
      <span class="brand-slot brand-type" aria-hidden="true">
        <span class="brand-wordmark">
          <span class="brand-name">Archiplayer</span>
          <span class="brand-sub">WFMU</span>
        </span>
        <span class="brand-cue">Press to scroll to top</span>
      </span>
    </button>
    <div class="nav-links">
      <a
        href="/"
        class:active={isActive("/")}
        aria-current={isActive("/") ? "page" : undefined}
        title={isActive("/") && !collapsed ? "Collapse to player only" : "Shows"}
        onclick={(event) => onNavClick(event, "/")}
      >Shows</a>
      <a
        href="/profile"
        class:active={isActive("/profile")}
        aria-current={isActive("/profile") ? "page" : undefined}
        title={isActive("/profile") && !collapsed ? "Collapse to player only" : "Profile"}
        onclick={(event) => onNavClick(event, "/profile")}
      >Profile</a>
    </div>
    <a
      class="nav-donate"
      href="https://pledge.wfmu.org/donate"
      title="Support WFMU"
      onclick={supportWfmu}
    ><span class="d-full">♥ Support WFMU</span><span class="d-mini">♥ WFMU</span></a>
  </nav>

  <main id="main-content" bind:this={mainEl} tabindex="-1" onscroll={measureMainScroll}>
    {@render children()}
  </main>
  {#if favError}
    <div class="fav-error" role="alert">
      <span>{favError}</span>
      <button aria-label="Dismiss error" onclick={() => (favError = null)}>✕</button>
    </div>
  {/if}
  <Toast />
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
    --pbtn-main-size: 42px;   /* transport main play/pause button */
    --pctl-gap: 8px;          /* gap between transport buttons */
    --player-gap: 18px;       /* gap between player sections */
    --hsl-h: 18px;            /* row height of a painted horizontal slider */
    --hsl-track: 3px;         /* its bar thickness — thin, so ticks clear it */
    --hsl-thumb: 12px;        /* its knob diameter */
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
  /* Painted sliders hide their native input, so the ring is drawn on the track instead. */
  :global(:where(a, button, input, summary):not(.sl-input):focus-visible) {
    outline: 2px solid var(--c-accent) !important;
    outline-offset: 2px;
  }
  /* Single-line truncation utility (shared across the player, show list and track rows). */
  :global(.ellipsis) {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    /* clip (not hidden): hidden is still script-scrollable, so an episode
       scrollIntoView() inside <main> would scroll the whole shell and shove the
       player header off the top. clip makes the shell a non-scroll container. */
    overflow: clip; /* shell never scrolls — only <main> does */
  }
  .skip-link {
    position: fixed;
    top: 8px;
    left: 8px;
    z-index: 200;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--c-accent);
    color: var(--c-on-accent);
    font-weight: 700;
    transform: translateY(calc(-100% - 12px));
  }
  .skip-link:focus {
    transform: translateY(0);
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
    /* Button reset: it scrolls <main> to the top instead of navigating. */
    background: none;
    border: 0;
    padding: 0;
    font: inherit;
    text-align: left;
    cursor: pointer;
    --brand-mark: 34px; /* logo and arrow share it, so the swap can't resize the nav */
  }
  .brand:hover {
    text-decoration: none;
  }
  /* Scrolled far enough that the top is worth offering, the brand stops being a
     wordmark and becomes the control it always was. Both states of each half live
     in one grid cell: the cell is as wide as the wider of the two, so the swap
     crossfades in place and the tabs beside it never move. */
  .brand-slot {
    display: grid;
    place-items: center start;
  }
  .brand-slot > * {
    grid-area: 1 / 1;
    transition: opacity 140ms ease-out;
  }
  .brand-arrow,
  .brand-cue {
    opacity: 0;
  }
  .brand.cued .brand-logo,
  .brand.cued .brand-wordmark {
    opacity: 0;
  }
  .brand.cued .brand-arrow,
  .brand.cued .brand-cue {
    opacity: 1;
  }
  .brand-logo {
    width: var(--brand-mark);
    height: var(--brand-mark);
    border-radius: 6px;
    display: block;
  }
  /* Same footprint and corner as the logo it replaces, so only the glyph changes. */
  .brand-arrow {
    display: flex;
    align-items: center;
    justify-content: center;
    width: var(--brand-mark);
    height: var(--brand-mark);
    background: var(--c-surface2);
    border-radius: 6px;
    color: var(--c-accent);
  }
  .brand-wordmark {
    display: flex;
    align-items: baseline;
    gap: 9px;
    white-space: nowrap;
  }
  .brand-name {
    font-weight: 800;
    letter-spacing: 0.01em;
  }
  .brand-sub {
    color: var(--c-dim);
    font-size: 12px;
  }
  .brand-cue {
    color: var(--c-dim);
    font-size: 13px;
    font-weight: 600;
    white-space: nowrap;
  }
  .brand:hover .brand-arrow {
    background: var(--c-accent);
    color: var(--c-on-accent);
  }
  .brand:hover .brand-cue {
    color: var(--c-accent);
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
  .nav-donate {
    margin-left: auto;
    color: var(--c-gold);
    padding: 4px 10px;
    border-radius: 6px;
    font-weight: 700;
    white-space: nowrap;
  }
  .nav-donate:hover {
    color: var(--c-line);
    text-decoration: none;
  }
  .nav-donate .d-mini {
    display: none;
  }
  main {
    flex: 1 1 auto;
    overflow-y: auto;
    padding: 20px;
  }
  /* Player-only (collapsed) view: the window shrinks to player + nav, but that
     resize is async and floored by minHeight, so the frame is briefly taller than
     the content. Hide the routed page so its sticky search bar can never leak in
     below the nav. Kept mounted + in flow, so scroll and size-restore survive. */
  .app.collapsed main {
    visibility: hidden;
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
  /* Optically centre the right-pointing play/playing triangles wherever they render
     (header, track rows, show + profile pages). A percentage keeps the shift proportional
     across icon sizes. */
  :global(.icon-play),
  :global(.icon-playing) {
    transform: translateX(12%);
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
  }
  .p-track.idle {
    color: var(--c-dim);
    opacity: 0.7;
  }
  .lucky {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--c-accent);
    cursor: pointer;
  }
  .lucky:hover:not(:disabled) {
    text-decoration: underline;
  }
  .lucky:disabled {
    cursor: default;
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
  /* Horizontal sliders are custom-painted for the same reason the mini volume is: the
     WebView does not style native ranges reliably, and only a painted track lets the
     ticks line up with where the thumb actually travels. */
  .hsl {
    position: relative;
    flex: 1 1 auto;
    min-width: 0;
    height: var(--hsl-h);
    --hsl-inset: calc(var(--hsl-thumb) / 2);
  }
  .hsl-track {
    /* Inset by half a thumb, so a tick at N% sits under the thumb at N%. */
    position: absolute;
    top: 50%;
    right: var(--hsl-inset);
    left: var(--hsl-inset);
    height: var(--hsl-track);
    overflow: visible;
    transform: translateY(-50%);
    background: var(--c-border);
    border-radius: 999px;
    pointer-events: none;
  }
  .hsl-fill {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 0;
    width: var(--hsl-level);
    background: var(--c-accent);
    border-radius: inherit;
  }
  .hsl-thumb {
    position: absolute;
    top: 50%;
    left: var(--hsl-level);
    width: var(--hsl-thumb);
    height: var(--hsl-thumb);
    transform: translate(-50%, -50%);
    background: var(--c-accent);
    border: 2px solid var(--c-surface);
    border-radius: 50%;
    box-shadow: 0 0 0 1px var(--c-border);
  }
  /* Scale ticks and playlist marks are cut into the bar rather than drawn over it:
     a notch in the player colour, painted after the fill so it reads the same on
     filled and unfilled stretches. Emphasis is width, since depth is fixed. */
  .hsl-tick {
    position: absolute;
    top: 0;
    height: 100%;
    transform: translateX(-50%);
    background: var(--c-surface);
    pointer-events: none;
  }
  .hsl-tick.mark {
    width: 2px;
  }
  /* The knob is the only part of the bar that reacts to a hover, and it grows by a few
     per cent rather than changing colour: enough to say it is grabbable, not enough to
     shift the position it is reporting. The input is the pointer target, so the hover
     has to be read off it. */
  .hsl.seek .hsl-thumb {
    transition: transform 90ms ease;
  }
  .hsl.seek .sl-input:hover:not(:disabled) ~ .hsl-track .hsl-thumb {
    transform: translate(-50%, -50%) scale(1.07);
  }
  /* The loupe. Pressing the bar swaps the knob for a round window that re-renders the
     same track at --lens-zoom, so playlist marks a few pixels apart separate far enough
     to aim at. Pointer travel is geared down by the same factor, so the magnification
     buys real seek resolution and not just a better view of it. */
  .hsl.seek.lensing .hsl-thumb {
    opacity: 0;
  }
  /* 36px of lens on an 18px row spills over the title line above it. .hsl-track has a
     transform and is therefore a stacking context, so this has to be raised out here on
     the wrapper or it raises nothing. 3 stays under the mini volume popover's 20. */
  .hsl.seek.lensing {
    z-index: 3;
  }
  .seek-lens {
    position: absolute;
    top: 50%;
    left: var(--hsl-level);
    width: var(--lens-size);
    height: var(--lens-size);
    overflow: hidden;
    transform: translate(-50%, -50%);
    background: var(--c-surface);
    border: 2px solid var(--c-accent);
    border-radius: 50%;
    box-shadow:
      0 0 0 1px var(--c-border),
      0 2px 8px rgb(0 0 0 / 0.35);
    pointer-events: none;
    animation: lens-in 120ms ease-out;
  }
  @keyframes lens-in {
    from {
      transform: translate(-50%, -50%) scale(0.35);
    }
  }
  .lens-strip {
    position: absolute;
    top: 50%;
    /* Slide the magnified track left by the magnified distance to the playhead, so that
       position lands on the lens centre. --seek-n is unitless for exactly this. The ends
       are deliberately not clamped: running out of bar is how the lens shows you have
       reached the start or the finish. */
    left: calc(50% - var(--seek-n) * var(--track-w) * var(--lens-zoom));
    width: calc(var(--track-w) * var(--lens-zoom));
    height: calc(var(--hsl-track) * var(--lens-zoom));
    transform: translateY(-50%);
    background: var(--c-border);
    border-radius: 999px;
  }
  /* Notches scale with the bar they are cut into. */
  .lens-strip .hsl-tick.mark {
    width: calc(2px * var(--lens-zoom));
  }
  /* An aim mark rather than a bar: a dot on the exact second with a stroke above and
     below it, so the mark points at the position without covering the notch it is
     pointing at. Text colour rather than the accent, since the accent is the fill and
     would vanish on the played side. */
  .lens-cross {
    position: absolute;
    top: 50%;
    left: 50%;
    width: 3px;
    height: 3px;
    transform: translate(-50%, -50%);
    background: var(--c-text);
    border-radius: 50%;
  }
  .lens-cross::before,
  .lens-cross::after {
    position: absolute;
    left: 50%;
    width: 2px;
    height: 6px;
    transform: translateX(-50%);
    background: var(--c-text);
    border-radius: 1px;
    content: "";
  }
  /* The gap is wider than the dot so the strokes clear it, and each stroke starts
     inside the magnified bar and runs out past its edge. */
  .lens-cross::before {
    bottom: calc(100% + 3px);
  }
  .lens-cross::after {
    top: calc(100% + 3px);
  }
  .hsl .sl-input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    opacity: 0;
    cursor: pointer;
    touch-action: none;
  }
  /* Keyboard focus only. WCAG 2.4.7 wants an indicator for keyboard traversal, not for
     a click, and :focus-within would ring the bar every time it is grabbed. The halo
     rides the playhead so the indicator sits where the arrow keys act. */
  .sl-input:focus-visible ~ .hsl-track .hsl-thumb {
    box-shadow: 0 0 0 1px var(--c-border), 0 0 0 4px var(--c-accent);
  }
  .hsl.off {
    opacity: 0.4;
  }
  .hsl.off .sl-input {
    cursor: default;
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
    z-index: 10;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .fav-error button {
    border: 0;
    background: none;
    color: inherit;
    cursor: pointer;
    padding: 2px;
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
  .p-volume .vol-h {
    flex: 0 0 auto;
    width: 119px; /* 90px + 32% */
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
      gap: var(--player-gap);
      padding: var(--mini-pad-top) var(--mini-pad-inline) var(--mini-pad-bottom);

      /* Each clamp is linear between 324px and 760px. Keeping the endpoints here
         makes the intended minimum and intermediate compositions explicit. */
      --pbtn-size: clamp(32px, calc(29.028px + 0.917vw), 36px);
      --pctl-gap: clamp(6px, calc(4.514px + 0.459vw), 8px);
      --player-gap: clamp(14px, calc(11.028px + 0.917vw), 18px);
      --icon-size: clamp(11.4px, calc(8.428px + 0.917vw), 15.4px);
      --transport-width: clamp(274px, calc(232.385px + 12.844vw), 330px);
      --mini-pad-top: clamp(18px, calc(16.514px + 0.459vw), 20px);
      --mini-pad-inline: clamp(12px, calc(7.541px + 1.376vw), 18px);
      --mini-pad-bottom: clamp(8px, calc(5.028px + 0.917vw), 12px);
      --mini-control-font: clamp(12px, calc(10.514px + 0.459vw), 14px);
      --mini-title-font: clamp(12px, calc(10.514px + 0.459vw), 14px);
      --mini-track-font: clamp(11px, calc(9.514px + 0.459vw), 13px);
      --mini-meta-margin: clamp(3px, calc(2.257px + 0.229vw), 4px);
      --mini-skip-font: clamp(9px, calc(7.514px + 0.459vw), 11px);
      --mini-scrub-gap: clamp(8px, calc(6.514px + 0.459vw), 10px);
      --mini-scrub-height: clamp(28px, calc(25.028px + 0.917vw), 32px);
      --mini-seek-h: clamp(16px, calc(14.514px + 0.459vw), 18px);
      --mini-seek-thumb: clamp(11px, calc(10.257px + 0.229vw), 12px);
      --mini-volume-column: clamp(28px, calc(25.028px + 0.917vw), 32px);
      --mini-time-font: clamp(11px, calc(10.257px + 0.229vw), 12px);
      --mini-time-width: clamp(40px, calc(34.055px + 1.835vw), 48px);
      --mini-volume-width: clamp(36px, calc(33.028px + 0.917vw), 40px);
      --mini-volume-height: clamp(130px, calc(110.679px + 5.963vw), 156px);
      --mini-volume-pad-top: clamp(7px, calc(6.257px + 0.229vw), 8px);
      --mini-volume-pad-inline: clamp(5px, calc(4.257px + 0.229vw), 6px);
      --mini-volume-pad-bottom: clamp(34px, calc(31.028px + 0.917vw), 38px);
      --mini-volume-slider: clamp(88px, calc(73.138px + 4.587vw), 108px);
      --mini-volume-track-inset: clamp(6px, calc(4.514px + 0.459vw), 8px);
      --mini-volume-track: clamp(5px, calc(4.257px + 0.229vw), 6px);
      --mini-volume-thumb: clamp(13px, calc(11.514px + 0.459vw), 15px);
      --mini-volume-input: clamp(84px, calc(69.138px + 4.587vw), 104px);
    }
    .p-controls {
      justify-content: center;
      flex-wrap: nowrap;
      width: min(100%, var(--transport-width));
      margin-inline: auto;
    }
    /* Secondary control labels track the compact token above. */
    .pbtn {
      font-size: var(--mini-control-font);
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
    .p-scrub > .hsl.seek {
      grid-column: 2;
      width: 100%;
      height: var(--mini-seek-h);
      --hsl-thumb: var(--mini-seek-thumb);
    }
    .p-scrub .p-volume {
      margin-left: 0;
      grid-column: 4;
      justify-self: end;
      min-width: var(--mini-volume-column);
      z-index: 20; /* unconditional: the popover must clear the scrub row mid-fade */
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
      opacity: 0;
      visibility: hidden;
      /* Delay visibility so it only flips after the fade has finished. */
      transition:
        opacity 160ms ease,
        visibility 0s linear 160ms;
    }
    .p-volume.open .vol-pop {
      opacity: 1;
      visibility: visible;
      transition: opacity 160ms ease;
    }
    /* Constant stacking, not gated on .open, so the button stays above the panel
       while it fades back out. */
    .p-volume .pvol-btn {
      position: relative;
      z-index: 2;
    }
    .p-volume.open .pvol-btn {
      width: var(--mini-volume-column);
      height: var(--mini-volume-column);
      padding: 2px;
      justify-content: center;
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
    /* Same notch as the desktop bar, laid on its side. */
    .vol-tick {
      position: absolute;
      right: 0;
      left: 0;
      transform: translateY(50%);
      background: var(--c-surface);
      pointer-events: none;
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
    .vol-v:focus-visible ~ .vol-track .vol-thumb {
      box-shadow: 0 0 0 1px var(--c-border), 0 0 0 4px var(--c-accent);
    }

    /* Compact the nav so the tabs stay readable. */
    nav {
      gap: 12px;
      padding: 8px 12px;
    }
    .brand {
      gap: 6px;
      --brand-mark: 26px;
    }
    /* No room for either state of the type down here — the mark carries the swap. */
    .brand-type {
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
    /* Shorten the donate label so the button holds through the mini range. */
    .nav-donate .d-full {
      display: none;
    }
    .nav-donate .d-mini {
      display: inline;
    }
  }
  @media (max-width: 420px) {
    nav {
      gap: 8px;
      padding: 8px;
    }
    .brand {
      gap: 4px;
      --brand-mark: 24px;
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
  @media (prefers-reduced-motion: reduce) {
    :global(*) {
      scroll-behavior: auto !important;
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
    }
  }
</style>
