import assert from "node:assert/strict";
import {
  effectiveArchiveDuration,
  scheduledArchiveEndReached,
} from "../src/lib/archive-timing.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("episode 166789 excludes the following show's spillover", () => {
  const mediaDuration = 11_758.251938;
  const duration = effectiveArchiveDuration(mediaDuration, 378, 3 * 60 * 60);
  assert.equal(duration, 11_208);
  assert.equal(scheduledArchiveEndReached(11_207.9, mediaDuration, duration), false);
  assert.equal(scheduledArchiveEndReached(11_208, mediaDuration, duration), true);
});

test("pre-roll and thirty seconds of end grace remain playable", () => {
  assert.equal(effectiveArchiveDuration(12_000, 370, 3_600), 4_000);
});

test("unknown schedules retain the physical media duration", () => {
  assert.equal(effectiveArchiveDuration(7_000, 300, null), 7_000);
  assert.equal(scheduledArchiveEndReached(7_000, 7_000, 7_000), false);
});

test("short media is never extended to the scheduled boundary", () => {
  assert.equal(effectiveArchiveDuration(3_500, 300, 3_600), 3_500);
});

test("live-style infinite media remains uncapped without a schedule", () => {
  assert.equal(effectiveArchiveDuration(Infinity, 0, null), Infinity);
});
