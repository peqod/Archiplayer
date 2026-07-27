import assert from "node:assert/strict";
import {
  AUDITION_FADE_SEC,
  auditionFadeInGain,
  auditionFadeOutGain,
} from "../src/lib/audition-fade.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("audition opening ramps from silence to full gain over 1.5 seconds", () => {
  assert.equal(AUDITION_FADE_SEC, 1.5);
  assert.equal(auditionFadeInGain(0), 0);
  assert.equal(auditionFadeInGain(0.75), 0.5);
  assert.equal(auditionFadeInGain(1.5), 1);
  assert.equal(auditionFadeInGain(3), 1);
});

test("audition ending ramps down over the final 1.5 seconds", () => {
  assert.equal(auditionFadeOutGain(98, 100), 1);
  assert.equal(auditionFadeOutGain(98.5, 100), 1);
  assert.equal(auditionFadeOutGain(99.25, 100), 0.5);
  assert.equal(auditionFadeOutGain(100, 100), 0);
  assert.equal(auditionFadeOutGain(101, 100), 0);
});

test("unknown end timing leaves playback at full gain", () => {
  assert.equal(auditionFadeOutGain(10, 0), 1);
  assert.equal(auditionFadeOutGain(10, Infinity), 1);
});
