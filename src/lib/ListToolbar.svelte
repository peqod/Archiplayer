<script lang="ts">
  // Filter row shared by every tab of the profile list card: scope chips, a search
  // box, a sort select, an optional grouping select, and the live result count.
  import type { GroupBy, Scope, Sort } from "$lib/profile-lists";
  import { player } from "$lib/player.svelte";

  let {
    query = $bindable(""),
    sort = $bindable("time" as Sort),
    scope = $bindable("all" as Scope),
    group = $bindable("none" as GroupBy),
    sortOptions,
    showScope = false,
    showGroup = false,
    showQueueModes = false,
    count,
    noun,
    placeholder,
  }: {
    query?: string;
    sort?: Sort;
    scope?: Scope;
    group?: GroupBy;
    sortOptions: { value: Sort; label: string }[];
    showScope?: boolean;
    showGroup?: boolean;
    /** Shuffle and repeat, on the tabs whose lists can be played as a queue. */
    showQueueModes?: boolean;
    count: number;
    noun: string;
    placeholder: string;
  } = $props();

  const SCOPES: { value: Scope; label: string }[] = [
    { value: "all", label: "All" },
    { value: "fav", label: "Favourites" },
    { value: "listened", label: "Listened" },
  ];
</script>

<div class="toolbar">
  {#if showScope}
    <div class="chips" role="group" aria-label="Filter by">
      {#each SCOPES as s (s.value)}
        <button
          class="chip"
          class:on={scope === s.value}
          aria-pressed={scope === s.value}
          onclick={() => (scope = s.value)}>{s.label}</button>
      {/each}
    </div>
  {/if}

  <input
    class="search"
    type="search"
    bind:value={query}
    {placeholder}
    aria-label={placeholder} />

  <label class="pick">
    <span>Sort</span>
    <select bind:value={sort}>
      {#each sortOptions as o (o.value)}
        <option value={o.value}>{o.label}</option>
      {/each}
    </select>
  </label>

  {#if showQueueModes}
    <!-- Playback modes, not list filters: they sit here because this is where the order
         of the list is chosen, and shuffle is what overrides that order. -->
    <div class="chips" role="group" aria-label="Playback">
      <button
        class="chip"
        class:on={player.shuffle}
        aria-pressed={player.shuffle}
        title="Play the list in a random order"
        onclick={() => player.setShuffle(!player.shuffle)}>Shuffle</button>
      <button
        class="chip"
        class:on={player.repeat}
        aria-pressed={player.repeat}
        title="Start the list again when it ends"
        onclick={() => player.setRepeat(!player.repeat)}>Repeat</button>
    </div>
  {/if}

  {#if showGroup}
    <label class="pick">
      <span>Group</span>
      <select bind:value={group}>
        <option value="none">None</option>
        <option value="artist">Artist</option>
        <option value="show">Show</option>
      </select>
    </label>
  {/if}

  <span class="count" aria-live="polite">{count} {noun}{count === 1 ? "" : "s"}</span>
</div>

<style>
  .toolbar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 12px 0;
  }
  .chips {
    display: flex;
    gap: 4px;
  }
  .chip {
    background: var(--c-surface2);
    color: var(--c-dim);
    border: none;
    border-radius: 6px;
    padding: 5px 10px;
    cursor: pointer;
    font-size: 13px;
  }
  .chip.on {
    background: var(--c-accent);
    color: var(--c-surface);
    font-weight: 700;
  }
  .search {
    flex: 1 1 180px;
    max-width: 320px;
    background: var(--c-surface2);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    padding: 6px 10px;
    border-radius: 8px;
    font-size: 14px;
  }
  .search:focus {
    outline: none;
    border-color: var(--c-accent);
  }
  .search::-webkit-search-cancel-button {
    -webkit-appearance: none;
    appearance: none;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--c-dim);
  }
  .pick select {
    background: var(--c-surface2);
    border: 1px solid var(--c-border);
    color: var(--c-text);
    border-radius: 6px;
    padding: 5px 6px;
    font-size: 13px;
    cursor: pointer;
  }
  .pick select:focus {
    outline: none;
    border-color: var(--c-accent);
  }
  .count {
    margin-left: auto;
    font-size: 12px;
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  @media (max-width: 520px) {
    .count {
      margin-left: 0;
    }
    .search {
      max-width: none;
    }
  }
</style>
