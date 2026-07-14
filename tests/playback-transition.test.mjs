import assert from "node:assert/strict";
import {
  isAbortError,
  PlaybackTransitions,
} from "../src/lib/playback-transition.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

test("an active live station toggles without another switch", () => {
  const gate = new PlaybackTransitions();
  const first = gate.requestLive("live:drummer");
  assert.equal(first.action, "switch");
  gate.settle(first.generation);
  assert.deepEqual(gate.requestLive("live:drummer"), {
    action: "toggle",
    generation: first.generation,
  });
});

test("a rapid double click is coalesced while connecting", () => {
  const gate = new PlaybackTransitions();
  const first = gate.requestLive("live:sheena");
  assert.equal(first.action, "switch");
  assert.deepEqual(gate.requestLive("live:sheena"), {
    action: "coalesce",
    generation: first.generation,
  });
});

test("rapid station switches reject stale work", () => {
  const gate = new PlaybackTransitions();
  const drummer = gate.requestLive("live:drummer");
  const sheena = gate.requestLive("live:sheena");
  assert.equal(sheena.action, "switch");
  assert.equal(gate.isCurrent(drummer.generation), false);
  assert.equal(gate.isCurrent(sheena.generation), true);
  gate.settle(drummer.generation);
  assert.equal(gate.requestLive("live:sheena").action, "coalesce");
  gate.settle(sheena.generation);
  assert.equal(gate.requestLive("live:sheena").action, "toggle");
});

test("only stale AbortErrors are intentional transition aborts", () => {
  const gate = new PlaybackTransitions();
  const old = gate.start("archive:1");
  const current = gate.start("live:freeform");
  const abort = { name: "AbortError" };
  assert.equal(isAbortError(abort) && !gate.isCurrent(old), true);
  assert.equal(isAbortError(abort) && !gate.isCurrent(current), false);
});
