import assert from "node:assert/strict";
import {
  canPlayExactTrack,
  hasExactTrackTimestamp,
} from "../src/lib/track-playback.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("timestamped tracks expose exact-song playback when episode audio exists", () => {
  assert.equal(hasExactTrackTimestamp(0), true);
  assert.equal(hasExactTrackTimestamp(125), true);
  assert.equal(canPlayExactTrack(true, 125), true);
});

test("untimestamped tracks never expose exact-song playback", () => {
  assert.equal(hasExactTrackTimestamp(null), false);
  assert.equal(canPlayExactTrack(true, null), false);
});

test("timestamped tracks remain unavailable when the episode has no audio", () => {
  assert.equal(canPlayExactTrack(false, 125), false);
});
