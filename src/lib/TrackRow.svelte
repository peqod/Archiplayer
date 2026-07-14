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
  }: {
    track: Track;
    current?: boolean;
    playing?: boolean;
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
    title={current
      ? playing ? "Pause song" : "Resume song"
      : playable && track.start_sec !== null
        ? `Play at ${fmtTime(track.start_sec)}`
        : playable
          ? "Play episode (no timestamp)"
          : "Live track"}
  ><Icon name={current && playing ? "pause" : "play"} /></button>
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
    grid-template-columns: 30px 56px minmax(100px, 220px) minmax(120px, 1fr) minmax(80px, 220px) 30px;
    align-items: center;
    gap: 8px;
    padding: 3px 6px;
    border-radius: 6px;
    font-size: 13px;
    width: 100%;
    min-width: 0;
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
    min-width: 0;
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

  @media (max-width: 760px) {
    .track {
      grid-template-columns: 28px 50px minmax(0, 1fr) 28px;
      grid-template-areas:
        "play time artist favourite"
        "play time title favourite"
        "play time album favourite";
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
    .mini {
      grid-area: favourite;
      align-self: center;
      justify-self: end;
      padding: 4px;
    }
  }
</style>
