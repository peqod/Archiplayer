<script lang="ts">
  import { fmtTime, type Track } from "$lib/api";
  import Icon from "$lib/Icon.svelte";

  let {
    track,
    current = false,
    playing = false,
    playable = true,
    timeLabel = null,
    onplay,
    onfavourite,
    onshare,
  }: {
    track: Track;
    current?: boolean;
    playing?: boolean;
    playable?: boolean;
    timeLabel?: string | null;
    onplay?: () => void;
    onfavourite: () => void;
    onshare?: () => void;
  } = $props();
</script>

<div class="track" class:now={current} aria-current={current ? "true" : undefined}>
  {#if playable && onplay && track.start_sec !== null}
    <button
      class="tplay"
      onclick={onplay}
      title={current
        ? playing ? "Pause song" : "Resume song"
        : `Play at ${fmtTime(track.start_sec)}`}
    ><Icon name={current && playing ? "pause" : "play"} /></button>
  {:else}
    <span class="tplay-spacer" aria-hidden="true"></span>
  {/if}
  <span class="ttime">{timeLabel ?? (track.start_sec !== null ? fmtTime(track.start_sec) : "–")}</span>
  <span class="tartist ellipsis">{track.artist ?? ""}</span>
  <span class="ttitle ellipsis">{track.title ?? ""}</span>
  <span class="talbum ellipsis">{track.album ?? ""}</span>
  <div class="tactions">
    <button
      class="mini"
      class:on={track.favourite}
      onclick={onfavourite}
      title="Star song"
    ><Icon name="star" filled={track.favourite} /></button>
    {#if onshare}
      <button class="mini" onclick={onshare} title="Share song"><Icon name="share" /></button>
    {/if}
  </div>
</div>

<style>
  .track {
    display: grid;
    grid-template-columns: 30px 56px minmax(100px, 220px) minmax(120px, 1fr) minmax(80px, 220px) auto;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 6px;
    font-size: 13px;
    width: 100%;
    min-width: 0;
  }
  /* Zebra striping: every other row 15% darker than the surface behind it.
     A black overlay darkens regardless of page/theme; hover + .now rules follow
     so they still override. */
  .track:nth-child(even) {
    background: rgba(0, 0, 0, 0.15);
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
  .ttime {
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
    min-width: 0;
  }
  .tartist {
    font-weight: 600;
  }
  .talbum {
    color: var(--c-dim);
    text-align: right;
  }
  .tactions {
    display: flex;
    align-items: center;
    gap: 2px;
    justify-self: end;
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
  .mini:hover {
    color: var(--c-accent);
    background: var(--c-surface2);
  }
  .mini.on {
    color: var(--c-gold);
  }

  @media (max-width: 760px) {
    .track {
      grid-template-columns: 28px 50px minmax(0, 1fr) auto;
      grid-template-areas:
        "play time artist actions"
        "play time title actions"
        "play time album actions";
      column-gap: 6px;
      row-gap: 1px;
      padding: 5px 2px;
    }
    .tplay {
      grid-area: play;
      align-self: center;
    }
    .ttime {
      grid-area: time;
      align-self: center;
    }
    .tartist {
      grid-area: artist;
    }
    .ttitle {
      grid-area: title;
      color: var(--c-dim);
    }
    .talbum {
      grid-area: album;
      text-align: left;
      font-size: 12px;
    }
    .tactions {
      grid-area: actions;
      align-self: center;
      flex-direction: column;
      gap: 0;
    }
    .mini {
      padding: 4px;
    }
  }
</style>
