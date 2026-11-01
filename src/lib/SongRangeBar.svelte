<script lang="ts">
  /**
   * Range selector that replaces the seek bar while the listener edits a song's start
   * and end. Two native range inputs share one painted track; only their thumbs take the
   * pointer, so either handle can be grabbed wherever it sits. Values are audio seconds,
   * the same clock as the seek bar, so the handles line up with the playhead.
   */
  let {
    duration,
    position,
    start = $bindable(),
    end = $bindable(),
    minGap = 10,
  }: {
    duration: number;
    /** Playhead, audio seconds, drawn as a thin line for orientation. */
    position: number;
    start: number;
    end: number;
    minGap?: number;
  } = $props();

  const pct = (sec: number) => (duration > 0 ? `${(sec / duration) * 100}%` : "0%");

  function onStart(e: Event) {
    const v = Number((e.target as HTMLInputElement).value);
    start = Math.max(0, Math.min(v, end - minGap));
    (e.target as HTMLInputElement).value = String(start);
  }

  function onEnd(e: Event) {
    const v = Number((e.target as HTMLInputElement).value);
    end = Math.min(duration, Math.max(v, start + minGap));
    (e.target as HTMLInputElement).value = String(end);
  }
</script>

<div
  class="srb"
  style={`--srb-start: ${pct(start)}; --srb-end: ${pct(end)}; --srb-pos: ${pct(position)}`}
>
  <span class="srb-track" aria-hidden="true">
    <span class="srb-span"></span>
    <span class="srb-pos"></span>
  </span>
  <input
    class="srb-input"
    type="range"
    min="0"
    max={duration}
    step="1"
    value={start}
    oninput={onStart}
    aria-label="Song start"
  />
  <input
    class="srb-input"
    type="range"
    min="0"
    max={duration}
    step="1"
    value={end}
    oninput={onEnd}
    aria-label="Song end"
  />
</div>

<style>
  .srb {
    position: relative;
    flex: 1 1 auto;
    min-width: 0;
    height: var(--hsl-h, 18px);
    --srb-thumb: var(--hsl-thumb, 12px);
    --srb-inset: calc(var(--srb-thumb) / 2);
  }
  .srb-track {
    position: absolute;
    top: 50%;
    right: var(--srb-inset);
    left: var(--srb-inset);
    height: var(--hsl-track, 4px);
    transform: translateY(-50%);
    background: var(--c-border);
    border-radius: 999px;
    pointer-events: none;
  }
  .srb-span {
    position: absolute;
    top: -2px;
    bottom: -2px;
    left: var(--srb-start);
    right: calc(100% - var(--srb-end));
    background: var(--c-accent);
    border-radius: 2px;
  }
  .srb-pos {
    position: absolute;
    top: -4px;
    bottom: -4px;
    left: var(--srb-pos);
    width: 2px;
    transform: translateX(-50%);
    background: var(--c-text);
    opacity: 0.6;
  }
  .srb-input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    margin: 0;
    background: transparent;
    appearance: none;
    -webkit-appearance: none;
    pointer-events: none;
  }
  .srb-input::-webkit-slider-runnable-track {
    background: transparent;
  }
  .srb-input::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: var(--srb-thumb);
    height: calc(var(--srb-thumb) + 6px);
    border-radius: 3px;
    background: var(--c-accent);
    border: 2px solid var(--c-surface);
    box-shadow: 0 0 0 1px var(--c-border);
    cursor: ew-resize;
    pointer-events: auto;
  }
  .srb-input::-moz-range-thumb {
    width: var(--srb-thumb);
    height: calc(var(--srb-thumb) + 6px);
    border-radius: 3px;
    background: var(--c-accent);
    border: 2px solid var(--c-surface);
    cursor: ew-resize;
    pointer-events: auto;
  }
  .srb-input:focus-visible::-webkit-slider-thumb {
    outline: 2px solid var(--c-accent);
    outline-offset: 2px;
  }
</style>
