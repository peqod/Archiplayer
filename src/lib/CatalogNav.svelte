<script lang="ts">
  import { api, type Show } from "$lib/api";
  import { player, type QueueItem } from "$lib/player.svelte";
  import { selectRandomPlayback } from "$lib/random-show";
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";

  // Persistent catalog bar for views that have no show list of their own (e.g. the
  // show/playlist page). Search + alphabet don't filter in place — they jump to the
  // home catalog with the matching filter applied (?q= / ?letter= / ?fav=1).
  // `reverse` is owned by the host view (bound) so the toggle sits in the alpha bar
  // without moving the affordance elsewhere.
  let { reverse = $bindable(false) }: { reverse?: boolean } = $props();

  let shows = $state<Show[]>([]);
  let query = $state("");
  let randomBusy = $state(false);

  onMount(() => {
    let active = true;
    void api
      .getCatalog()
      .then((catalog) => {
        if (active) shows = catalog;
      })
      .catch(() => {
        if (active) shows = [];
      });
    return () => {
      active = false;
    };
  });

  // Same first-letter bucketing the home catalog uses, so the two alphabets match.
  const letters = $derived.by(() => {
    const set = new Set<string>();
    for (const s of shows) {
      const c = s.name.replace(/^the\s+/i, "").charAt(0).toUpperCase();
      set.add(/[A-Z]/.test(c) ? c : "#");
    }
    return [...set].sort((a, b) => (a === "#" ? 1 : b === "#" ? -1 : a.localeCompare(b)));
  });
  const favCount = $derived(shows.filter((s) => s.favourite).length);

  function submitSearch(e: SubmitEvent) {
    e.preventDefault();
    const q = query.trim();
    goto(q ? "/?q=" + encodeURIComponent(q) : "/");
  }

  // Same random-show + random-episode jump as the home dice; opens the show and plays.
  async function randomShow() {
    if (!shows.length || randomBusy) return;
    randomBusy = true;
    try {
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
      await player.playQueue(items, selection.index);
    } catch {
      /* leave the bar quiet on failure */
    } finally {
      randomBusy = false;
    }
  }
</script>

<div class="catnav">
  <form class="search-wrap" onsubmit={submitSearch}>
    <input
      class="search"
      type="search"
      aria-label="Search shows, DJs, and songs"
      placeholder="Search shows, DJs, songs…"
      bind:value={query}
    />
    <button
      class="dice"
      type="button"
      onclick={randomShow}
      disabled={!shows.length || randomBusy}
      title="Play a random show and episode"
      aria-label="Play a random show and episode"
    >{randomBusy ? "…" : "🎲"}</button>
  </form>
  <div class="alpha" role="group" aria-label="Catalog navigation and episode order">
    <button
      class="fav-filter"
      onclick={() => goto("/?fav=1")}
      title="Show only favourited shows"
    >★ Favourites{favCount ? ` (${favCount})` : ""}</button>
    <span class="alpha-sep" aria-hidden="true"></span>
    <button onclick={() => goto("/")}>All</button>
    {#each letters as l}
      <button onclick={() => goto("/?letter=" + encodeURIComponent(l))}>{l}</button>
    {/each}
    <span class="alpha-gap" aria-hidden="true"></span>
    <button
      class="rev"
      class:on={reverse}
      onclick={() => (reverse = !reverse)}
      aria-pressed={reverse}
      aria-label="Reverse episode order"
      title={reverse ? "Show newest first" : "Reverse order (oldest first)"}
    >⇅</button>
  </div>
</div>

<style>
  /* Sticks to the top of the scrolling show view so it stays reachable while the
     episode list scrolls. Background matches the page so rows don't bleed through. */
  .catnav {
    position: sticky;
    top: -20px; /* cancel <main>'s top padding so it pins flush */
    z-index: 5;
    background: var(--c-bg);
    padding: 12px 0;
    margin-bottom: 4px;
  }
  .search-wrap {
    position: relative;
    max-width: 520px;
    margin-bottom: 10px;
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
  .alpha {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
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
  .alpha button:hover {
    color: var(--c-accent);
  }
  .fav-filter {
    color: var(--c-gold) !important;
    font-weight: 600;
    white-space: nowrap;
  }
  .alpha-sep {
    width: 1px;
    align-self: stretch;
    background: var(--c-border);
    margin: 0 4px;
  }
  /* 16px breather then the reverse-order toggle, mirroring the home catalog bar. */
  .alpha-gap {
    width: 16px;
    flex: 0 0 auto;
  }
  .rev {
    font-size: 15px;
    line-height: 1;
  }
  .rev.on {
    background: var(--c-accent);
    color: var(--c-surface);
    font-weight: 700;
  }
</style>
