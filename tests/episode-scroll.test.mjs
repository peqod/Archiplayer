import assert from "node:assert/strict";
import { centerEpisodeRow } from "../src/lib/episode-scroll.ts";

function harness(found = true) {
  const calls = [];
  const row = {
    scrollIntoView(options) {
      calls.push(options);
    },
  };
  const selectors = [];
  const container = {
    querySelector(selector) {
      selectors.push(selector);
      return found ? row : null;
    },
  };
  return { calls, container, selectors };
}

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("centers the requested episode with smooth scrolling", () => {
  const { calls, container, selectors } = harness();

  assert.equal(centerEpisodeRow(container, 42, false), true);
  assert.deepEqual(selectors, ['[data-episode-id="42"]']);
  assert.deepEqual(calls, [
    { behavior: "smooth", block: "center", inline: "nearest" },
  ]);
});

test("honours reduced-motion preferences", () => {
  const { calls, container } = harness();

  assert.equal(centerEpisodeRow(container, 7, true), true);
  assert.equal(calls[0].behavior, "auto");
});

test("does not scroll when the episode row is missing", () => {
  const { calls, container } = harness(false);

  assert.equal(centerEpisodeRow(container, 99, false), false);
  assert.deepEqual(calls, []);
});

test("rejects invalid episode IDs without querying the view", () => {
  const { container, selectors } = harness();

  assert.equal(centerEpisodeRow(container, 0, false), false);
  assert.deepEqual(selectors, []);
});
