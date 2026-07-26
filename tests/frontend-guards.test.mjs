import assert from "node:assert/strict";
import { LatestRequest } from "../src/lib/request-gate.ts";
import { normalizeVolume } from "../src/lib/volume.ts";
import { restingAnchor } from "../src/lib/back-anchor.ts";
import { playGlyph } from "../src/lib/play-icon.ts";

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

test("play controls show player state, not the click action", () => {
  assert.equal(playGlyph(true, true), "playing");
  assert.equal(playGlyph(true, false), "pause");
  // Rows that aren't the loaded item stay on the plain triangle even while
  // something else is playing — otherwise every idle row would show pause bars.
  assert.equal(playGlyph(false, true), "play");
  assert.equal(playGlyph(false, false), "play");
});
