<script lang="ts">
  import { onMount } from "svelte";
  import {
    MARQUEE_GAP,
    marqueeMetrics,
    sameMarqueeMetrics,
    type MarqueeMetrics,
  } from "./marquee";

  type Props = {
    text: string;
    href?: string;
    tone?: "default" | "danger";
  };

  let { text, href, tone = "default" }: Props = $props();
  let viewport: HTMLSpanElement;
  let probe: HTMLSpanElement;
  let overflowing = $state(false);
  let distance = $state(0);
  let duration = $state(0);
  let measureFrame = 0;

  function measure() {
    measureFrame = 0;
    const metrics = marqueeMetrics(
      viewport?.clientWidth ?? 0,
      probe?.scrollWidth ?? 0,
    );
    const current: MarqueeMetrics = { overflowing, distance, duration };
    if (sameMarqueeMetrics(metrics, current)) return;
    overflowing = metrics.overflowing;
    distance = metrics.distance;
    duration = metrics.duration;
  }

  function scheduleMeasure() {
    if (measureFrame) return;
    measureFrame = requestAnimationFrame(measure);
  }

  onMount(() => {
    scheduleMeasure();
    if (typeof ResizeObserver === "undefined") {
      return () => {
        if (measureFrame) cancelAnimationFrame(measureFrame);
      };
    }

    const observer = new ResizeObserver(scheduleMeasure);
    observer.observe(viewport);
    observer.observe(probe);
    return () => {
      observer.disconnect();
      if (measureFrame) cancelAnimationFrame(measureFrame);
    };
  });
</script>

<span
  class="marquee"
  class:overflowing
  class:danger={tone === "danger"}
  bind:this={viewport}
  title={overflowing ? text : undefined}
  style={`--marquee-gap: ${MARQUEE_GAP}px; --marquee-shift: -${distance}px; --marquee-duration: ${duration}s`}
>
  <span class="track">
    <span class="copy">
      {#if href}<a {href}>{text}</a>{:else}{text}{/if}
    </span>
    <span class="copy duplicate" aria-hidden="true">{text}</span>
  </span>
  <!-- The probe is always intrinsic and out of flow, so toggling the marquee cannot
       resize the thing its observer is measuring. -->
  <span class="probe" bind:this={probe} aria-hidden="true">{text}</span>
</span>

<style>
  .marquee {
    position: relative;
    display: block;
    min-width: 0;
    overflow: hidden;
    /* Shrink-wrap, never stretch: the row packs left, so whatever follows the text
       (air date, queue counter) sits right next to it instead of at the far edge.
       A long title still shrinks, so the clip-and-scroll measurement is unchanged. */
    flex: 0 1 auto;
    white-space: nowrap;
  }
  .track,
  .copy {
    display: block;
    min-width: 0;
  }
  .copy {
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .duplicate {
    display: none;
  }
  .probe {
    position: absolute;
    inset: 0 auto auto 0;
    width: max-content;
    visibility: hidden;
    pointer-events: none;
  }
  .danger {
    color: var(--c-danger);
  }

  @keyframes marquee-forward {
    to {
      transform: translateX(var(--marquee-shift));
    }
  }

  @media (min-width: 521px) and (max-width: 760px) {
    .marquee.overflowing .track {
      display: flex;
      width: max-content;
      gap: var(--marquee-gap);
      animation: marquee-forward var(--marquee-duration) linear infinite;
      will-change: transform;
    }
    .marquee.overflowing .copy {
      width: max-content;
      overflow: visible;
      flex: 0 0 auto;
      text-overflow: clip;
    }
    .marquee.overflowing .duplicate {
      display: block;
    }
    .marquee.overflowing:hover .track {
      animation-play-state: paused;
    }
    /* Reset a moving link when it receives keyboard focus so its focus ring cannot
       stop outside the clipped viewport. */
    .marquee.overflowing:focus-within .track {
      animation: none;
      transform: none;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .marquee.overflowing .track {
      display: block;
      width: 100%;
      animation: none !important;
      transform: none !important;
    }
    .marquee.overflowing .copy {
      width: 100%;
      overflow: hidden;
      flex: 0 1 auto;
      text-overflow: ellipsis;
    }
    .marquee.overflowing .duplicate {
      display: none !important;
    }
  }
</style>
