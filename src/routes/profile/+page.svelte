<script lang="ts">
  import {
    api,
    fmtHours,
    fmtTime,
    type EraStat,
    type Favourites,
    type Stats,
    type DownloadRow,
  } from "$lib/api";
  import { player } from "$lib/player.svelte";
  import { confirm, open, save } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { listen } from "@tauri-apps/api/event";
  import { onMount, tick, untrack } from "svelte";
  import { centerRow } from "$lib/episode-scroll";
  import ThemePicker from "$lib/ThemePicker.svelte";
  import ShortcutPicker from "$lib/ShortcutPicker.svelte";
  import { shortcuts } from "$lib/shortcuts.svelte";
  import Icon from "$lib/Icon.svelte";
  import StatBand from "$lib/StatBand.svelte";
  import ListToolbar from "$lib/ListToolbar.svelte";
  import { LatestRequest } from "$lib/request-gate";
  import { reportError } from "$lib/toaster.svelte";
  import { shareShowRef, shareEpisodeRef, shareTrackRef, wfmuShowUrl } from "$lib/share";
  import { canPlayExactTrack, hasExactTrackTimestamp } from "$lib/track-playback";
  import { playGlyph } from "$lib/play-icon";
  import { episodeQueueFrom, songQueueFrom } from "$lib/queue-build";
  import {
    applyList,
    groupTracks,
    mergeEpisodes,
    mergeShows,
    toTrackRows,
    type EpisodeRow,
    type GroupBy,
    type Scope,
    type Sort,
    type TrackGroup,
    type TrackRowData,
  } from "$lib/profile-lists";

  let favs = $state<Favourites | null>(null);
  let stats = $state<Stats | null>(null);
  let eras = $state<EraStat[]>([]);
  let downloads = $state<DownloadRow[]>([]);
  let dlLoading = $state(true);
  let downloadDir = $state("");
  let error = $state<string | null>(null);
  let notice = $state<string | null>(null);
  let showBanner = $state(false);
  let mounted = false;
  const profileRequests = new LatestRequest();
  const downloadRequests = new LatestRequest();

  // -- List state -----------------------------------------------------------

  type Tab = "shows" | "episodes" | "songs" | "downloads";
  const TABS: { id: Tab; label: string }[] = [
    { id: "shows", label: "Shows" },
    { id: "episodes", label: "Episodes" },
    { id: "songs", label: "Songs" },
    { id: "downloads", label: "Downloads" },
  ];
  const PAGE_SIZE = 15;

  const SHOW_SORTS: { value: Sort; label: string }[] = [
    { value: "time", label: "Time listened" },
    { value: "plays", label: "Sessions" },
    { value: "name", label: "Name A-Z" },
  ];
  const EPISODE_SORTS: { value: Sort; label: string }[] = [
    { value: "time", label: "Time listened" },
    { value: "plays", label: "Sessions" },
    { value: "recent", label: "Air date" },
    { value: "name", label: "Show A-Z" },
  ];
  const SONG_SORTS: { value: Sort; label: string }[] = [
    { value: "recent", label: "Air date" },
    { value: "name", label: "Artist A-Z" },
  ];

  let tab = $state<Tab>("shows");
  let query = $state("");
  let scope = $state<Scope>("all");
  let showSort = $state<Sort>("time");
  let epSort = $state<Sort>("time");
  let songSort = $state<Sort>("recent");
  let songGroup = $state<GroupBy>("none");
  let visible = $state(PAGE_SIZE);
  let tabEls = $state<HTMLButtonElement[]>([]);
  let prefsLoaded = $state(false);

  // Preferences ride in localStorage under the same `ab2.` prefix as the banner
  // dismissal. Deliberately not in the URL: the back button belongs to navigation,
  // not to which tab of one page was last open.
  function readPref<T extends string>(key: string, allowed: readonly T[], fallback: T): T {
    try {
      const raw = localStorage.getItem(`ab2.profile.${key}`);
      return allowed.includes(raw as T) ? (raw as T) : fallback;
    } catch {
      return fallback;
    }
  }

  function writePref(key: string, value: string) {
    try {
      localStorage.setItem(`ab2.profile.${key}`, value);
    } catch {
      /* preferences are best effort */
    }
  }

  function loadPrefs() {
    tab = readPref<Tab>("tab", ["shows", "episodes", "songs", "downloads"], "shows");
    scope = readPref<Scope>("scope", ["all", "fav", "listened"], "all");
    showSort = readPref<Sort>("sort.shows", ["time", "plays", "name"], "time");
    epSort = readPref<Sort>("sort.episodes", ["time", "plays", "recent", "name"], "time");
    songSort = readPref<Sort>("sort.songs", ["recent", "name"], "recent");
    songGroup = readPref<GroupBy>("group.songs", ["none", "show", "artist"], "none");
    prefsLoaded = true;
  }

  $effect(() => {
    if (!prefsLoaded) return;
    writePref("tab", tab);
    writePref("scope", scope);
    writePref("sort.shows", showSort);
    writePref("sort.episodes", epSort);
    writePref("sort.songs", songSort);
    writePref("group.songs", songGroup);
  });

  // Any change to what the list contains starts the cap over, so "show more" never
  // carries a count from a filter the user has since changed.
  $effect(() => {
    void [query, scope, showSort, epSort, songSort, songGroup];
    visible = PAGE_SIZE;
  });

  function selectTab(next: Tab) {
    if (next === tab) return;
    tab = next;
    query = "";
    visible = PAGE_SIZE;
    // Arriving at a tab should bring the playhead into view even if this row was already
    // followed on the tab that was open before.
    followedRowId = null;
  }

  function onTabKey(e: KeyboardEvent) {
    const at = TABS.findIndex((t) => t.id === tab);
    let next = -1;
    if (e.key === "ArrowRight") next = (at + 1) % TABS.length;
    else if (e.key === "ArrowLeft") next = (at - 1 + TABS.length) % TABS.length;
    else if (e.key === "Home") next = 0;
    else if (e.key === "End") next = TABS.length - 1;
    else return;
    e.preventDefault();
    selectTab(TABS[next].id);
    tabEls[next]?.focus();
  }

  // -- Rows -----------------------------------------------------------------

  const showRows = $derived(mergeShows(stats?.shows ?? [], favs?.shows ?? []));
  const episodeRows = $derived(mergeEpisodes(stats?.episodes ?? [], favs?.episodes ?? []));
  const trackRows = $derived(toTrackRows(favs?.tracks ?? []));

  const shownShows = $derived(applyList(showRows, { query, sort: showSort, scope }));
  const shownEpisodes = $derived(applyList(episodeRows, { query, sort: epSort, scope }));
  const shownTracks = $derived(applyList(trackRows, { query, sort: songSort }));

  const showMax = $derived(Math.max(0, ...shownShows.map((r) => r.seconds)));
  const episodeMax = $derived(Math.max(0, ...shownEpisodes.map((r) => r.seconds)));

  const songBlocks = $derived(groupTracks(shownTracks, songGroup));

  // Grouping still obeys the cap: the groups are filled in order until it runs out.
  const trackGroups = $derived.by(() => {
    let left = visible;
    const out: TrackGroup[] = [];
    for (const g of songBlocks) {
      if (left <= 0) break;
      out.push({ ...g, items: g.items.slice(0, left) });
      left -= Math.min(left, g.items.length);
    }
    return out;
  });

  const visibleEpisodes = $derived(shownEpisodes.slice(0, visible));

  // What a queue is built from: the whole filtered list, in the order the page renders it,
  // grouping included. Deliberately *not* the `visible` slice. That cap is pagination, and
  // queueing only the rendered rows would end the run two songs in whenever the listener
  // pressed play near the bottom of the page.
  const orderedTracks = $derived(songBlocks.flatMap((g) => g.items));

  // Which row is sounding. The queue entry is authoritative and is known before the lazily
  // fetched playlist lands; without one, the playhead's position inside the episode
  // decides. Live counts too: its tracks are real playlist rows and can be favourites.
  const playingEpisodeId = $derived(
    player.current?.episode.id ?? player.liveEpisode?.episode.id ?? null,
  );
  const playingTrackId = $derived(
    player.current?.song?.id ??
      (player.currentTrackIndex >= 0 ? player.tracks[player.currentTrackIndex].id : null),
  );

  const isPlayingTrack = (row: TrackRowData) =>
    row.episodeId === playingEpisodeId && row.id === playingTrackId;
  const isPlayingEpisode = (row: EpisodeRow) => row.id === playingEpisodeId;

  let episodeListEl = $state<HTMLElement | null>(null);
  let songListEl = $state<HTMLElement | null>(null);
  // Deliberately not $state: the effect below writes it, and a reactive read of its own
  // write would re-run the effect forever.
  let followedRowId: number | null = null;

  // Follow the queue. `block: "nearest"` means a row already on screen is left exactly
  // where it is, so this only ever acts when the queue has advanced out of view.
  $effect(() => {
    const songs = tab === "songs";
    const id = songs ? playingTrackId : tab === "episodes" ? playingEpisodeId : null;
    if (id === null || id === followedRowId) return;
    const rows: { id: number }[] = songs ? orderedTracks : shownEpisodes;
    const at = rows.findIndex((r) => r.id === id);
    if (at < 0) return; // filtered out of this list; nothing to follow
    // The queue runs past the cap, so the list has to unfold to keep up. Untracked: this
    // effect writes `visible`, and reading it reactively would make that write re-run it.
    if (at >= untrack(() => visible)) {
      visible = Math.ceil((at + 1) / PAGE_SIZE) * PAGE_SIZE;
    }
    const list = songs ? songListEl : episodeListEl;
    const reducedMotion = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    // After the rows for this change have been rendered, or the row may not exist yet.
    void tick().then(() => {
      if (centerRow(list, "data-row-id", id, reducedMotion, "nearest")) followedRowId = id;
    });
  });

  const counts = $derived({
    shows: showRows.length,
    episodes: episodeRows.length,
    songs: trackRows.length,
    downloads: downloads.length,
  });

  const totalShown = $derived(
    tab === "shows"
      ? shownShows.length
      : tab === "episodes"
        ? shownEpisodes.length
        : tab === "songs"
          ? shownTracks.length
          : downloads.length,
  );
  const moreCount = $derived(Math.max(0, totalShown - visible));

  // A row with no listening gets no bar at all; anything above zero keeps a visible
  // sliver so a short session does not read as "never played".
  function pct(seconds: number, max: number): number {
    if (seconds <= 0 || max <= 0) return 0;
    return Math.max(3, Math.round((seconds / max) * 100));
  }

  // -- Data -----------------------------------------------------------------

  function dismissBanner() {
    showBanner = false;
    try {
      localStorage.setItem("ab2.hideLocalBanner", "1");
    } catch {
      /* dismissal still applies for this visit */
    }
  }

  async function load() {
    const generation = profileRequests.begin();
    error = null;
    try {
      const [nextFavs, nextStats, nextEras] = await Promise.all([
        api.listFavourites(),
        api.getStats(),
        api.getEraStats(),
      ]);
      if (!profileRequests.isCurrent(generation)) return;
      favs = nextFavs;
      stats = nextStats;
      eras = nextEras;
    } catch (e) {
      if (profileRequests.isCurrent(generation)) reportError(e, (m) => (error = m));
    }
  }

  async function loadDownloads() {
    const generation = downloadRequests.begin();
    error = null;
    try {
      const [nextDownloads, nextDownloadDir] = await Promise.all([
        api.listDownloads(),
        api.getDownloadDir(),
      ]);
      if (!downloadRequests.isCurrent(generation)) return;
      downloads = nextDownloads;
      downloadDir = nextDownloadDir;
    } catch (e) {
      if (downloadRequests.isCurrent(generation)) reportError(e, (m) => (error = m));
    } finally {
      if (downloadRequests.isCurrent(generation)) dlLoading = false;
    }
  }

  async function changeDownloadDir() {
    try {
      const picked = await open({ directory: true, defaultPath: downloadDir || undefined });
      if (typeof picked === "string") {
        await api.setDownloadDir(picked);
        downloadDir = picked;
      }
    } catch (e) {
      reportError(e, (m) => (error = m));
    }
  }

  async function revealDownload(row: DownloadRow) {
    try {
      await revealItemInDir(row.download.path);
    } catch (e) {
      reportError(e, (m) => (error = m));
    }
  }

  onMount(() => {
    mounted = true;
    loadPrefs();
    try {
      showBanner = localStorage.getItem("ab2.hideLocalBanner") !== "1";
    } catch {
      showBanner = true;
    }
    void load();
    void loadDownloads();

    const un = listen<{ episode_id: number; bytes: number; total: number; status: string }>(
      "download-progress",
      (e) => {
        if (!mounted) return;
        const p = e.payload;
        const known = downloads.some((r) => r.download.episode_id === p.episode_id);
        if (!known || p.status === "done") {
          void loadDownloads();
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
      mounted = false;
      profileRequests.invalidate();
      downloadRequests.invalidate();
      un.then((f) => f()).catch(() => {});
    };
  });

  async function playDownload(row: DownloadRow) {
    if (!row.show_id) return;
    try {
      const detail = await api.getShow(row.show_id);
      const ep = detail.episodes.find((e) => e.id === row.download.episode_id);
      if (ep) await player.playEpisode(ep, row.show_name ?? row.show_id);
    } catch (e) {
      reportError(e, (m) => (error = m));
    }
  }

  async function removeDownload(row: DownloadRow) {
    try {
      const label = row.show_name
        ? `${row.show_name}${row.air_date ? `, ${row.air_date}` : ""}`
        : `episode #${row.download.episode_id}`;
      const accepted = await confirm(`Delete the offline copy of ${label}?`, {
        title: "Delete download",
        kind: "warning",
      });
      if (!accepted) return;
      await api.deleteDownload(row.download.episode_id);
      downloads = downloads.filter((r) => r.download.episode_id !== row.download.episode_id);
    } catch (e) {
      reportError(e, (m) => (error = m));
    }
  }

  function fmtBytes(n: number): string {
    if (n > 1e9) return `${(n / 1e9).toFixed(2)} GB`;
    if (n > 1e6) return `${(n / 1e6).toFixed(1)} MB`;
    return `${Math.round(n / 1e3)} KB`;
  }

  async function toggleFav(kind: "show" | "episode" | "track", refId: string) {
    try {
      await api.toggleFavourite(kind, refId);
      await load();
    } catch (e) {
      reportError(e, (m) => (error = m));
    }
  }

  // Playing a row queues that row and everything below it, so a list of favourites plays
  // through like a record rather than stopping after one item.
  async function playEpisodeRow(row: EpisodeRow) {
    if (isPlayingEpisode(row)) {
      player.toggle();
      return;
    }
    try {
      const items = episodeQueueFrom(shownEpisodes, row.id);
      if (items[0]?.episode.id === row.id) {
        await player.playQueue(items);
        return;
      }
      // A row from the listening stats has no stored `Episode`, so it is resolved through
      // its show the same way an offline copy is, then leads the rest of the list.
      const detail = await api.getShow(row.showId);
      const ep = detail.episodes.find((e) => e.id === row.id);
      if (ep) await player.playQueue([{ episode: ep, showName: row.showName }, ...items]);
    } catch (e) {
      reportError(e, (m) => (error = m));
    }
  }

  async function playFavTrack(row: TrackRowData) {
    if (!canPlayExactTrack(row.episode.has_audio, row.startSec)) return;
    if (isPlayingTrack(row)) {
      player.toggle();
      return;
    }
    try {
      const items = songQueueFrom(orderedTracks, row.id);
      if (items.length) await player.playQueue(items);
    } catch (e) {
      reportError(e, (m) => (error = m));
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
      reportError(e, (m) => (error = m));
    }
  }
</script>

<div class="head">
  <h1>Profile</h1>
  <div class="export">
    <span class="muted">Export CSV</span>
    <button class="ghost" onclick={() => exportCsv("favourites")}>Favourites</button>
    <button class="ghost" onclick={() => exportCsv("listens")}>History</button>
    <button class="ghost" onclick={() => exportCsv("stats")}>Ranking</button>
  </div>
</div>

{#if showBanner}
  <div class="banner">
    <span>Local only: favourites and listening history live in a SQLite file on this machine. No login, nothing leaves your computer.</span>
    <button class="banner-x" onclick={dismissBanner} title="Dismiss" aria-label="Dismiss">✕</button>
  </div>
{/if}

{#if error}<div class="error" role="alert">{error}</div>{/if}
{#if notice}<div class="notice" role="status">{notice}</div>{/if}

{#if stats}
  <StatBand {stats} {eras} />
{:else}
  <p class="muted">Loading...</p>
{/if}

<section class="card">
  <div class="tabs" role="tablist" aria-label="Profile lists">
    {#each TABS as t, i (t.id)}
      <button
        bind:this={tabEls[i]}
        role="tab"
        id="tab-{t.id}"
        aria-selected={tab === t.id}
        aria-controls="panel-{t.id}"
        tabindex={tab === t.id ? 0 : -1}
        class:on={tab === t.id}
        onclick={() => selectTab(t.id)}
        onkeydown={onTabKey}
      >{t.label} <span class="tab-count">{counts[t.id]}</span></button>
    {/each}
  </div>

  {#if tab === "shows"}
    <div class="panel" role="tabpanel" id="panel-shows" aria-labelledby="tab-shows" tabindex="-1">
      {#if !stats || !favs}
        <p class="muted">Loading...</p>
      {:else}
        <ListToolbar
          bind:query
          bind:sort={showSort}
          bind:scope
          sortOptions={SHOW_SORTS}
          showScope
          count={shownShows.length}
          noun="show"
          placeholder="Search shows and DJs" />
        {#if shownShows.length === 0}
          <p class="muted">
            {showRows.length === 0
              ? "Nothing yet. Listen to a show, or hover one and hit the star."
              : "No show matches that filter."}
          </p>
        {:else}
          <div class="list">
            {#each shownShows.slice(0, visible) as row, i (row.id)}
              <div class="row">
                <span class="rank">{i + 1}</span>
                <a class="row-main" href={"/show/" + row.id}>
                  <span class="row-text">
                    <span class="row-name">{row.name}</span>
                    {#if row.dj}<span class="row-sub">with {row.dj}</span>{/if}
                  </span>
                </a>
                <span class="meter" aria-hidden="true">
                  <span class="meter-fill" style="width:{pct(row.seconds, showMax)}%"></span>
                </span>
                <span class="num">{row.seconds ? fmtHours(row.seconds) : "-"}</span>
                <span class="num narrow">{row.plays ? `${row.plays}x` : "-"}</span>
                <div class="row-actions">
                  <button class="mini" onclick={() => shareShowRef(row.id, row.name, row.dj)} aria-label={`Share ${row.name}`} title="Share"><Icon name="share" /></button>
                  <button
                    class="mini"
                    class:starred={row.favourite}
                    onclick={() => toggleFav("show", row.id)}
                    aria-label={row.favourite ? `Remove ${row.name} from favourites` : `Add ${row.name} to favourites`}
                    title={row.favourite ? "Remove from favourites" : "Add to favourites"}
                  ><Icon name="star" filled={row.favourite} /></button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {:else if tab === "episodes"}
    <div class="panel" role="tabpanel" id="panel-episodes" aria-labelledby="tab-episodes" tabindex="-1">
      {#if !stats || !favs}
        <p class="muted">Loading...</p>
      {:else}
        <ListToolbar
          bind:query
          bind:sort={epSort}
          bind:scope
          sortOptions={EPISODE_SORTS}
          showScope
          showQueueModes
          count={shownEpisodes.length}
          noun="episode"
          placeholder="Search episodes" />
        {#if shownEpisodes.length === 0}
          <p class="muted">
            {episodeRows.length === 0
              ? "Nothing yet. Play an episode, or star one from a show page."
              : "No episode matches that filter."}
          </p>
        {:else}
          <div class="list" bind:this={episodeListEl}>
            {#each visibleEpisodes as row, i (row.id)}
              {@const now = isPlayingEpisode(row)}
              <div
                class="row"
                class:now
                data-row-id={row.id}
                aria-current={now ? "true" : undefined}
              >
                <span class="rank">{i + 1}</span>
                <button
                  class="play"
                  onclick={() => playEpisodeRow(row)}
                  disabled={row.episode !== null && !row.episode.has_audio}
                  aria-label={now
                    ? player.playing ? "Pause episode" : "Resume episode"
                    : `Play ${row.showName} episode`}
                  title={now
                    ? player.playing ? "Pause episode" : "Resume episode"
                    : "Play"}
                ><Icon name={playGlyph(now, player.playing)} /></button>
                <a class="row-main" href={"/show/" + row.showId}>
                  <span class="row-text">
                    <span class="row-name">{row.showName}</span>
                    <span class="row-sub">
                      {row.airDate ?? ""}{row.title ? `${row.airDate ? ", " : ""}${row.title}` : ""}
                    </span>
                  </span>
                </a>
                <span class="meter" aria-hidden="true">
                  <span class="meter-fill" style="width:{pct(row.seconds, episodeMax)}%"></span>
                </span>
                <span class="num">{row.seconds ? fmtHours(row.seconds) : "-"}</span>
                <span class="num narrow">{row.plays ? `${row.plays}x` : "-"}</span>
                <div class="row-actions">
                  <button class="mini" onclick={() => shareEpisodeRef(row.showName, row.id, row.airDate, row.title)} aria-label={`Share ${row.showName} episode`} title="Share"><Icon name="share" /></button>
                  <button
                    class="mini"
                    class:starred={row.favourite}
                    onclick={() => toggleFav("episode", String(row.id))}
                    aria-label={row.favourite ? `Remove ${row.showName} episode from favourites` : `Add ${row.showName} episode to favourites`}
                    title={row.favourite ? "Remove from favourites" : "Add to favourites"}
                  ><Icon name="star" filled={row.favourite} /></button>
                </div>
              </div>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {:else if tab === "songs"}
    <div class="panel" role="tabpanel" id="panel-songs" aria-labelledby="tab-songs" tabindex="-1">
      {#if !favs}
        <p class="muted">Loading...</p>
      {:else}
        <ListToolbar
          bind:query
          bind:sort={songSort}
          bind:group={songGroup}
          sortOptions={SONG_SORTS}
          showGroup
          showQueueModes
          count={shownTracks.length}
          noun="song"
          placeholder="Search artists, songs, shows" />
        {#if shownTracks.length === 0}
          <p class="muted">
            {trackRows.length === 0
              ? "None yet. Favourite songs from a playlist."
              : "No song matches that filter."}
          </p>
        {:else}
          <div class="list" bind:this={songListEl}>
            {#each trackGroups as group (group.key)}
              {#if group.label}
                <div class="group-head">
                  <span>{group.label}</span>
                  <span class="group-count">{group.items.length}</span>
                </div>
              {/if}
              {#each group.items as row (row.id)}
                {@const now = isPlayingTrack(row)}
                <div
                  class="row"
                  class:now
                  data-row-id={row.id}
                  aria-current={now ? "true" : undefined}
                >
                  <button
                    class="play"
                    onclick={() => playFavTrack(row)}
                    disabled={!canPlayExactTrack(row.episode.has_audio, row.startSec)}
                    aria-label={now
                      ? player.playing ? "Pause song" : "Resume song"
                      : canPlayExactTrack(row.episode.has_audio, row.startSec)
                        ? `Play ${row.artist ?? "unknown artist"}, ${row.title ?? "unknown song"}`
                        : `Exact-song playback unavailable for ${row.artist ?? "unknown artist"}, ${row.title ?? "unknown song"}: ${hasExactTrackTimestamp(row.startSec) ? "no archived audio" : "no timestamp"}`}
                    title={now
                      ? player.playing ? "Pause song" : "Resume song"
                      : !hasExactTrackTimestamp(row.startSec)
                        ? "No timestamp available"
                        : row.episode.has_audio
                          ? `Play at ${fmtTime(row.startSec)}`
                          : "No archived audio"}
                  ><Icon name={playGlyph(now, player.playing)} /></button>
                  <div class="row-main">
                    <span class="row-text">
                      <span class="row-name"><b>{row.artist ?? "?"}</b> / {row.title ?? "?"}</span>
                      <span class="row-sub">
                        {row.showName}, {row.airDate ?? ""}
                        {#if !hasExactTrackTimestamp(row.startSec)}, no timestamp
                        {:else if !row.episode.has_audio}, no archived audio{/if}
                      </span>
                    </span>
                  </div>
                  <div class="row-actions">
                    <button class="mini" onclick={() => shareTrackRef(row.artist, row.title, row.showName, row.airDate, wfmuShowUrl(row.showId))} aria-label="Share song" title="Share"><Icon name="share" /></button>
                    <button class="mini starred" onclick={() => toggleFav("track", String(row.id))} aria-label="Remove song from favourites" title="Remove from favourites"><Icon name="star" filled /></button>
                  </div>
                </div>
              {/each}
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  {:else}
    <div class="panel" role="tabpanel" id="panel-downloads" aria-labelledby="tab-downloads" tabindex="-1">
      <div class="dl-dir">
        <span class="muted">Folder:</span>
        <span class="dl-dir-path" title={downloadDir}>{downloadDir || "default"}</span>
        <button class="ghost" onclick={changeDownloadDir}>Change...</button>
      </div>
      {#if dlLoading}
        <p class="muted">Loading...</p>
      {:else if downloads.length === 0}
        <p class="muted">Nothing downloaded yet. Use the ⤓ button on any episode.</p>
      {:else}
        <div class="list">
          {#each downloads.slice(0, visible) as row (row.download.episode_id)}
            <div class="row">
              <button
                class="play"
                onclick={() => playDownload(row)}
                disabled={!row.show_id || row.download.status !== "done"}
                aria-label="Play offline copy"
                title="Play offline copy"
              ><Icon name="play" /></button>
              <div class="row-main">
                <span class="row-text">
                  <span class="row-name">
                    {#if row.show_name}
                      {row.show_name}
                    {:else}
                      Episode #{row.download.episode_id}
                    {/if}
                  </span>
                  <span class="row-sub">
                    {#if row.download.status === "done"}
                      {row.air_date ?? ""}{row.title ? `, ${row.title}` : ""}, {fmtBytes(row.download.bytes)}
                    {:else if row.download.status === "downloading"}
                      downloading... {fmtBytes(row.download.bytes)}{row.download.total ? ` / ${fmtBytes(row.download.total)}` : ""}
                    {:else}
                      {row.download.status}
                    {/if}
                  </span>
                </span>
              </div>
              <div class="row-actions">
                <button
                  class="mini"
                  onclick={() => revealDownload(row)}
                  disabled={row.download.status !== "done"}
                  aria-label="Show download in file explorer"
                  title="Show in file explorer"
                ><Icon name="folder" /></button>
                <button class="mini danger" onclick={() => removeDownload(row)} aria-label="Delete downloaded file" title="Delete file"><Icon name="trash" /></button>
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>
  {/if}

  {#if moreCount > 0}
    <button class="more" onclick={() => (visible += PAGE_SIZE)}>
      Show {Math.min(PAGE_SIZE, moreCount)} more
    </button>
  {/if}
</section>

<details class="fold customize">
  <summary>
    <span class="cz-title">Customize</span>
    <span class="cz-hint">colour scheme &amp; theme</span>
  </summary>
  <p class="muted">Pick a colour scheme, or tweak any colour to make it yours. Saved locally.</p>
  <ThemePicker />
</details>

<details class="fold customize">
  <summary>
    <span class="cz-title">Keyboard shortcuts</span>
    <!-- The state rides on the summary so the switch is one click away with the
         fold shut, which is the point of having a switch at all. -->
    <span class="cz-hint">{shortcuts.enabled ? "play, skip, star, from anywhere" : "off"}</span>
  </summary>
  <ShortcutPicker />
</details>

<style>
  /* Local spacing scale: the page is dense by design, so the steps are small. */
  .card {
    --sp-1: 4px;
    --sp-2: 8px;
    --sp-3: 12px;
    max-width: 1100px;
  }
  .head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: 16px;
    flex-wrap: wrap;
  }
  h1 {
    margin: 0;
    font-size: 22px;
  }
  .export {
    display: flex;
    align-items: center;
    gap: 6px;
    flex-wrap: wrap;
    font-size: 12px;
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
  .ghost {
    background: none;
    border: 1px solid var(--c-border);
    color: var(--c-dim);
    border-radius: 8px;
    padding: 5px 10px;
    cursor: pointer;
    font-size: 12px;
  }
  .ghost:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  /* -- Tabs ---------------------------------------------------------------- */
  .tabs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-1);
    border-bottom: 1px solid var(--c-surface2);
    padding-bottom: var(--sp-2);
  }
  .tabs button {
    background: var(--c-surface);
    color: var(--c-dim);
    border: none;
    border-radius: 6px;
    padding: 6px 12px;
    cursor: pointer;
    font-size: 14px;
  }
  .tabs button:hover {
    color: var(--c-text);
  }
  .tabs button.on {
    background: var(--c-accent);
    color: var(--c-surface);
    font-weight: 700;
  }
  .tab-count {
    font-size: 11px;
    opacity: 0.75;
    font-variant-numeric: tabular-nums;
    margin-left: 2px;
  }
  .panel:focus {
    outline: none;
  }

  /* -- Rows ---------------------------------------------------------------- */
  .list {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    background: var(--c-surface);
    border-radius: 8px;
    padding: 5px 10px;
    min-height: 34px;
  }
  .row:hover {
    background: var(--c-surface2);
  }
  /* An outline, not a background: `--c-surface2` is already this row's hover state, so a
     background swap would read as "hovered" rather than "playing". Matches `.ep.current`
     on the show page. */
  .row.now {
    outline: 1px solid var(--c-accent);
  }
  .rank {
    flex: 0 0 22px;
    text-align: right;
    font-size: 12px;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
  }
  .row-main {
    flex: 1 1 auto;
    min-width: 0;
    color: inherit;
    text-decoration: none;
    display: flex;
    align-items: center;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row-sub {
    color: var(--c-dim);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* The bar carries the shape of the ranking; the number beside it carries the value,
     so the list still reads with the colour stripped out. */
  .meter {
    flex: 0 0 90px;
    height: 6px;
    border-radius: 3px;
    background: var(--c-surface2);
    overflow: hidden;
  }
  .row:hover .meter {
    background: var(--c-surface);
  }
  .meter-fill {
    display: block;
    height: 100%;
    border-radius: 3px;
    background: color-mix(in srgb, var(--c-accent) 65%, transparent);
    transition: width 0.2s ease;
  }
  .num {
    flex: 0 0 62px;
    text-align: right;
    font-size: 12px;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
  }
  .num.narrow {
    flex-basis: 40px;
  }
  .row-actions {
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: var(--sp-1);
  }
  .play {
    background: var(--c-surface2);
    border: none;
    color: var(--c-text);
    border-radius: 50%;
    width: 26px;
    height: 26px;
    cursor: pointer;
    flex: 0 0 auto;
  }
  .play:hover:not(:disabled) {
    background: var(--c-accent);
    color: var(--c-surface);
  }
  .play:disabled {
    opacity: 0.35;
  }
  .row:hover .play {
    background: var(--c-surface);
  }
  .row:hover .play:hover:not(:disabled) {
    background: var(--c-accent);
  }
  /* Actions dim rather than disappear: hiding them would put them out of reach of a
     keyboard, and they stay in the tab order either way. */
  .mini {
    background: none;
    border: none;
    color: var(--c-dim);
    cursor: pointer;
    font-size: 15px;
    display: inline-flex;
    align-items: center;
    opacity: 0.55;
  }
  .row:hover .mini,
  .mini:focus-visible,
  .mini.starred {
    opacity: 1;
  }
  .mini.starred {
    color: var(--c-gold);
  }
  .mini:hover:not(:disabled) {
    color: var(--c-accent);
  }
  .mini.starred:hover {
    color: var(--c-danger);
  }
  .mini.danger:hover {
    color: var(--c-danger);
  }
  .mini:disabled {
    opacity: 0.25;
    cursor: default;
  }
  .group-head {
    display: flex;
    align-items: baseline;
    gap: var(--sp-2);
    margin: var(--sp-3) 0 var(--sp-1);
    font-size: 12px;
    font-weight: 700;
    color: var(--c-accent);
  }
  .group-count {
    color: var(--c-dim);
    font-weight: 400;
    font-variant-numeric: tabular-nums;
  }
  .more {
    display: block;
    width: 100%;
    margin-top: var(--sp-2);
    background: none;
    border: 1px dashed var(--c-border);
    color: var(--c-dim);
    border-radius: 8px;
    padding: 8px;
    cursor: pointer;
    font-size: 13px;
  }
  .more:hover {
    border-color: var(--c-accent);
    color: var(--c-accent);
  }

  /* -- Downloads ----------------------------------------------------------- */
  .dl-dir {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    margin: var(--sp-3) 0;
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

  /* -- Folds --------------------------------------------------------------- */
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
  .customize {
    margin: 28px 0 8px;
    border-top: 1px solid var(--c-surface2);
    padding-top: 14px;
  }
  .cz-title {
    font-size: 17px;
    font-weight: 700;
  }
  .cz-hint {
    color: var(--c-dim);
    font-size: 12px;
  }

  @media (max-width: 760px) {
    .meter {
      display: none;
    }
  }
  @media (max-width: 520px) {
    .rank {
      display: none;
    }
    .num.narrow {
      display: none;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .meter-fill,
    .fold summary::before {
      transition: none;
    }
  }
</style>
