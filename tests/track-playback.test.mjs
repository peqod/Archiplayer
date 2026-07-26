import assert from "node:assert/strict";
import {
  canPlayExactTrack,
  hasExactTrackTimestamp,
  trackMarkSeconds,
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

test("scrubber marks shift show-relative timecodes into audio time", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }];
  assert.deepEqual(trackMarkSeconds(tracks, 30, 3600), [30, 220]);
  assert.deepEqual(trackMarkSeconds(tracks, 0, 3600), [0, 190]);
});

test("scrubber marks skip untimestamped tracks and collapse duplicates", () => {
  const tracks = [{ start_sec: 190 }, { start_sec: null }, { start_sec: 190 }];
  assert.deepEqual(trackMarkSeconds(tracks, 0, 3600), [190]);
});

test("scrubber marks stay inside the episode and sort ascending", () => {
  const tracks = [{ start_sec: 500 }, { start_sec: 4000 }, { start_sec: 100 }];
  assert.deepEqual(trackMarkSeconds(tracks, 0, 3600), [100, 500]);
  assert.deepEqual(trackMarkSeconds([{ start_sec: 10 }], -60, 3600), []);
});

test("scrubber marks are empty until the duration is known", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }];
  assert.deepEqual(trackMarkSeconds(tracks, 0, 0), []);
  assert.deepEqual(trackMarkSeconds(tracks, 0, Number.NaN), []);
  assert.deepEqual(trackMarkSeconds([], 0, 3600), []);
});
