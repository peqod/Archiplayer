import assert from "node:assert/strict";
import {
  canPlayExactTrack,
  canPlayFavouriteSong,
  effectiveStart,
  hasExactTrackTimestamp,
  parseClock,
  segmentEndSec,
  songEnd,
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

test("a song ends where the next timecoded track starts", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }, { start_sec: 400 }];
  assert.equal(segmentEndSec(tracks, 0), 190);
  assert.equal(segmentEndSec(tracks, 190), 400);
});

test("the last timecoded song runs to the end of the episode", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }];
  assert.equal(segmentEndSec(tracks, 190), null);
  assert.equal(segmentEndSec([], 190), null);
});

test("the song end ignores untimecoded tracks and anything at or behind the song", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }, { start_sec: null }, { start_sec: 400 }];
  assert.equal(segmentEndSec(tracks, 190), 400);
  assert.equal(segmentEndSec([{ start_sec: null }], 0), null);
  assert.equal(segmentEndSec([{ start_sec: 190 }], 190), null);
});

test("the song end is the nearest boundary ahead, whatever order the playlist is in", () => {
  const tracks = [{ start_sec: 400 }, { start_sec: 190 }, { start_sec: 250 }];
  assert.equal(segmentEndSec(tracks, 190), 250);
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

test("a manual song end wins over every derived end", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }];
  assert.deepEqual(songEnd(tracks, 0, 170, 300), { endSec: 170, source: "manual" });
  assert.deepEqual(songEnd(tracks, 190, 420, null), { endSec: 420, source: "manual" });
});

test("a manual end at or before the start is ignored", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }];
  assert.deepEqual(songEnd(tracks, 100, 100, null), { endSec: 190, source: "next" });
  assert.deepEqual(songEnd(tracks, 100, 50, null), { endSec: 190, source: "next" });
});

test("the looked-up length cuts DJ talk before the next timecode", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 400 }];
  assert.deepEqual(songEnd(tracks, 0, null, 180), { endSec: 185, source: "length" });
});

test("the next timecode wins when it comes before the looked-up length", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 150 }];
  assert.deepEqual(songEnd(tracks, 0, null, 180), { endSec: 150, source: "next" });
});

test("the last timecoded song stops by its length when one is known", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }, { start_sec: null }];
  assert.deepEqual(songEnd(tracks, 190, null, 200), { endSec: 395, source: "length" });
});

test("with no mark, length or later timecode the song runs to the episode end", () => {
  const tracks = [{ start_sec: 0 }, { start_sec: 190 }];
  assert.deepEqual(songEnd(tracks, 190, null, null), { endSec: null, source: "episode" });
  assert.deepEqual(songEnd([], 190, undefined, undefined), { endSec: null, source: "episode" });
});

test("a start of the listener's own wins, then the timecode, then the episode start", () => {
  assert.deepEqual(effectiveStart(120, 300), { sec: 300, known: true, manual: true });
  assert.deepEqual(effectiveStart(120, null), { sec: 120, known: true, manual: false });
  assert.deepEqual(effectiveStart(null, 0), { sec: 0, known: true, manual: true });
  assert.deepEqual(effectiveStart(null, undefined), { sec: 0, known: false, manual: false });
});

test("a favourite song needs only archived audio", () => {
  assert.equal(canPlayFavouriteSong(true), true);
  assert.equal(canPlayFavouriteSong(false), false);
});

test("the 0:00 fallback start ignores timecodes and length, but not the listener's end", () => {
  const tracks = [{ start_sec: null }, { start_sec: null }];
  assert.deepEqual(songEnd(tracks, 0, null, 200, false), { endSec: null, source: "episode" });
  assert.deepEqual(songEnd([{ start_sec: 90 }], 0, null, null, false), {
    endSec: null,
    source: "episode",
  });
  assert.deepEqual(songEnd(tracks, 0, 240, 200, false), { endSec: 240, source: "manual" });
});

test("clock text reads back what fmtTime writes", () => {
  assert.equal(parseClock("3:05"), 185);
  assert.equal(parseClock(" 1:02:03 "), 3723);
  assert.equal(parseClock("75:00"), 4500);
});

test("malformed clock text is rejected, not guessed", () => {
  for (const bad of ["", "185", "3:5x", "3:60", "1:2:3:4", "-1:00", "3.5:00"]) {
    assert.equal(parseClock(bad), null, bad);
  }
});
