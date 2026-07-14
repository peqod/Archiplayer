<script lang="ts">
  import { fmtTime, type Track } from "$lib/api";
  import Icon from "$lib/Icon.svelte";

  let {
    track,
    current = false,
    playable = true,
    timeLabel = null,
    onplay,
    onfavourite,
  }: {
    track: Track;
    current?: boolean;
    playable?: boolean;
    timeLabel?: string | null;
    onplay?: () => void;
    onfavourite: () => void;
  } = $props();
</script>

<div class="track" class:now={current} aria-current={current ? "true" : undefined}>
  <button
    class="tplay"
    onclick={onplay}
    disabled={!playable || !onplay}
    title={playable && track.start_sec !== null
      ? `Play at ${fmtTime(track.start_sec)}`
      : playable
        ? "Play episode (no timestamp)"
        : "Live track"}
  ><Icon name="play" /></button>
  <span class="ttime">{timeLabel ?? (track.start_sec !== null ? fmtTime(track.start_sec) : "–")}</span>
  <span class="tartist">{track.artist ?? ""}</span>
  <span class="ttitle">{track.title ?? ""}</span>
  <span class="talbum">{track.album ?? ""}</span>
  <button
    class="mini"
    class:on={track.favourite}
    onclick={onfavourite}
    title="Star song"
  ><Icon name="star" filled={track.favourite} /></button>
</div>

<style>
  .track {
    display: grid;
    grid-template-columns: 30px 64px 220px 1fr 220px 30px;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 6px;
    font-size: 13px;
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
  .tplay:disabled {
    opacity: 0.3;
  }
  .ttime {
    color: var(--c-dim);
    font-variant-numeric: tabular-nums;
    font-size: 12px;
  }
  .tartist {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ttitle {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .talbum {
    color: var(--c-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: right;
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
</style>
