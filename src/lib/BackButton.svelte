<script lang="ts">
  import { restingAnchor, type Point } from "$lib/back-anchor";
  import { onMount } from "svelte";

  // "← All shows" for the show and live views. Playlists run long, so once the
  // inline link scrolls out of <main> a fixed copy takes over at the exact spot
  // the inline one occupied at rest — the two states read as one element that
  // stopped moving rather than a control that jumped to a corner.
  let backEl: HTMLElement;
  let floating = $state(false);
  let anchor = $state<Point>({ left: 0, top: 0 });

  // Must match .back-pill's padding, otherwise the label shifts on hand-off.
  const PAD: Point = { left: 12, top: 6 };

  onMount(() => {
    // The shell never scrolls; <main> is the only scroll container (see +layout).
    const main = document.getElementById("main-content");
    if (!main) return;

    function measure() {
      if (!backEl || !main) return;
      anchor = restingAnchor(
        backEl.getBoundingClientRect(),
        main.getBoundingClientRect(),
        main.scrollTop,
        PAD,
      );
    }

    // The inline link stays mounted and in flow at all times. Hiding it would
    // reflow the page, which moves the observed element, which flips the
    // observer back — a flicker loop.
    const seen = new IntersectionObserver(
      ([entry]) => {
        floating = !entry.isIntersecting;
      },
      { root: main, threshold: 0 },
    );
    seen.observe(backEl);

    // Breakpoint reflows (760/420px) and the Tauri player-only collapse both
    // move the resting spot, so re-measure whenever <main> changes size.
    const resized = new ResizeObserver(measure);
    resized.observe(main);
    window.addEventListener("resize", measure);
    measure();

    return () => {
      seen.disconnect();
      resized.disconnect();
      window.removeEventListener("resize", measure);
    };
  });
</script>

<a bind:this={backEl} href="/" class="back">← All shows</a>

{#if floating}
  <!-- Decorative duplicate: the inline link above is the one keyboard and
       screen-reader users get, and focusing it scrolls it back into view. -->
  <a
    href="/"
    class="back-pill"
    style="left: {anchor.left}px; top: {anchor.top}px;"
    aria-hidden="true"
    tabindex="-1">← All shows</a
  >
{/if}

<style>
  .back {
    display: inline-block;
    margin-bottom: 12px;
    color: var(--c-dim);
  }
  /* Sits above CatalogNav (z 5) so a pinned search bar can't cover it. */
  .back-pill {
    position: fixed;
    z-index: 6;
    padding: 6px 12px;
    background: var(--c-surface);
    border: 1px solid var(--c-border);
    border-radius: 8px;
    color: var(--c-dim);
    box-shadow: 0 4px 14px rgb(0 0 0 / 0.35);
    animation: back-pill-in 140ms ease-out;
  }
  .back-pill:hover {
    color: var(--c-accent);
    text-decoration: none;
  }
  @keyframes back-pill-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
</style>
