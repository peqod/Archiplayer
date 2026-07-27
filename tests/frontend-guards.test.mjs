import assert from "node:assert/strict";
import { LatestRequest } from "../src/lib/request-gate.ts";
import { normalizeVolume, VOLUME_TICKS } from "../src/lib/volume.ts";
import { restingAnchor } from "../src/lib/back-anchor.ts";
import { playGlyph } from "../src/lib/play-icon.ts";
import { LOUPE_SIZE, LOUPE_ZOOM, gearedTime, timeAtPointer } from "../src/lib/seek-drag.ts";
import {
  BACK_LINE_H,
  MAIN_PAD_TOP,
  SCROLL_CUE_AT,
  scrollCueVisible,
} from "../src/lib/scroll-cue.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("starting newer async work makes an older completion stale", () => {
  const requests = new LatestRequest();
  const first = requests.begin();
  const second = requests.begin();

  assert.equal(requests.isCurrent(first), false);
  assert.equal(requests.isCurrent(second), true);
});

test("cleanup invalidates only the generation it owns", () => {
  const requests = new LatestRequest();
  const first = requests.begin();
  const second = requests.begin();

  requests.invalidate(first);
  assert.equal(requests.isCurrent(second), true);

  requests.invalidate(second);
  assert.equal(requests.isCurrent(second), false);
});

test("persisted volume is finite and clamped to the media range", () => {
  assert.equal(normalizeVolume("0.42"), 0.42);
  assert.equal(normalizeVolume("-2"), 0);
  assert.equal(normalizeVolume("4"), 1);
  assert.equal(normalizeVolume("not-a-number", 0.6), 0.6);
  assert.equal(normalizeVolume("not-a-number", Number.NaN), 1);
  assert.equal(normalizeVolume(null), 1);
});

test("the volume scale is a 10% grid with the midpoint called out", () => {
  assert.deepEqual(
    VOLUME_TICKS.map((t) => t.pct),
    [0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100],
  );
  const size = (pct) => VOLUME_TICKS.find((t) => t.pct === pct).size;
  assert.equal(size(50), 6);
  assert.equal(size(0), 4);
  assert.equal(size(100), 4);
  // Every other tick is the small one, so 50% reads as the middle at a glance.
  for (const pct of [10, 20, 30, 40, 60, 70, 80, 90]) assert.equal(size(pct), 2);
});

test("a press on the seek bar lands on the pointed second", () => {
  assert.equal(timeAtPointer(400, 100, 600, 3600), 1800);
  // Past either end of the track clamps rather than seeking outside the episode.
  assert.equal(timeAtPointer(0, 100, 600, 3600), 0);
  assert.equal(timeAtPointer(9999, 100, 600, 3600), 3600);
  // Nothing loaded yet means there is nowhere to seek to.
  assert.equal(timeAtPointer(400, 100, 600, 0), 0);
});

test("the loupe gears pointer travel down by its magnification", () => {
  // 600px over an hour is 6s per pixel, so at 4x a 40px drag is worth 10px of bar.
  assert.equal(gearedTime(1800, 400, 440, 600, 3600), 1860);
  assert.equal(gearedTime(1800, 400, 360, 600, 3600), 1740);
  // Ungeared, the same travel would carry four times as far.
  assert.equal(gearedTime(1800, 400, 440, 600, 3600, 1), 2040);
});

test("a geared drag stays inside the episode and survives a degenerate bar", () => {
  assert.equal(gearedTime(10, 400, 0, 600, 3600), 0);
  assert.equal(gearedTime(3590, 400, 4000, 600, 3600), 3600);
  assert.equal(gearedTime(1800, 400, 440, 0, 3600), 1800);
  assert.equal(gearedTime(1800, 400, 440, 600, 0), 0);
  // The track is measured on mount, so a press in the first frame can see no width at
  // all. Every unusable input holds the press rather than flinging it to the start.
  assert.equal(gearedTime(1800, 400, 440, Number.NaN, 3600), 1800);
  assert.equal(gearedTime(1800, 400, Number.NaN, 600, 3600), 1800);
  assert.equal(timeAtPointer(400, 100, Number.NaN, 3600), 0);
  assert.equal(timeAtPointer(Number.NaN, 100, 600, 3600), 0);
});

test("the gearing divisor is the magnification, so the loupe cannot lie", () => {
  // The whole feature rests on this: moving the hand by one lens pixel has to be worth
  // exactly one bar pixel of time, or the lens shows a position the drag cannot reach.
  const secPerBarPx = 3600 / 600;
  assert.equal(gearedTime(0, 0, LOUPE_ZOOM, 600, 3600), secPerBarPx);
  assert.equal(LOUPE_ZOOM, 4);
  assert.equal(LOUPE_SIZE, 36);
});

// <main> spans y 120..800; the back link rests 20px into it.
const MAIN = { top: 120, left: 0, bottom: 800 };
const PAD = { left: 12, top: 6 };

test("the floating back pill lands where the inline link's text was", () => {
  const back = { top: 140, left: 20, bottom: 160 };

  assert.deepEqual(restingAnchor(back, MAIN, 0, PAD), { left: 8, top: 134 });
  assert.deepEqual(restingAnchor(back, MAIN, 0, { left: 0, top: 0 }), { left: 20, top: 140 });
});

test("the resting anchor is the same however far <main> has scrolled", () => {
  const atRest = restingAnchor({ top: 140, left: 20, bottom: 160 }, MAIN, 0, PAD);
  // Scrolled 600px: the link has left the viewport, so its rect moved up by 600.
  const scrolled = restingAnchor({ top: -460, left: 20, bottom: -440 }, MAIN, 600, PAD);

  assert.deepEqual(scrolled, atRest);
});

test("the anchor stays inside the scroll area", () => {
  // A link above <main> (impossible in flow, but reachable mid-resize) clamps down.
  assert.equal(restingAnchor({ top: 0, left: 20, bottom: 20 }, MAIN, 0, PAD).top, 120);
  // A link near the bottom keeps a 48px guard so the pill is never half off-screen.
  assert.equal(restingAnchor({ top: 790, left: 20, bottom: 810 }, MAIN, 0, PAD).top, 752);
  // A viewport shorter than the guard collapses to the top edge, never above it.
  const squashed = { top: 120, left: 0, bottom: 150 };
  assert.equal(restingAnchor({ top: 140, left: 20, bottom: 160 }, squashed, 0, PAD).top, 120);
});

test("the brand cue and the back pill change on the same scroll travel", () => {
  // The pill takes over when the inline link's bottom edge clears the top of <main>,
  // which is its own height below <main>'s padding. Drifting from that is what makes
  // the two changes read as two events instead of one.
  assert.equal(SCROLL_CUE_AT, MAIN_PAD_TOP + BACK_LINE_H);
  assert.equal(scrollCueVisible(SCROLL_CUE_AT - 1), false);
  assert.equal(scrollCueVisible(SCROLL_CUE_AT), true);
  // Unscrolled, and a rubber-banded overscroll past the top, both keep the wordmark.
  assert.equal(scrollCueVisible(0), false);
  assert.equal(scrollCueVisible(-120), false);
  // <main> is read straight off the DOM, so a torn-down element must not cue.
  assert.equal(scrollCueVisible(Number.NaN), false);
});

test("play controls show player state, not the click action", () => {
  assert.equal(playGlyph(true, true), "playing");
  assert.equal(playGlyph(true, false), "pause");
  // Rows that aren't the loaded item stay on the plain triangle even while
  // something else is playing — otherwise every idle row would show pause bars.
  assert.equal(playGlyph(false, true), "play");
  assert.equal(playGlyph(false, false), "play");
});
