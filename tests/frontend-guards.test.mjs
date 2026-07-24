import assert from "node:assert/strict";
import { LatestRequest } from "../src/lib/request-gate.ts";
import { normalizeVolume } from "../src/lib/volume.ts";

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
