import assert from "node:assert/strict";
import { selectRandomPlayback } from "../src/lib/random-show.ts";

function show(id) {
  return {
    id,
    name: `Show ${id}`,
    dj: null,
    description: null,
    on_air: false,
    episode_count: 0,
    favourite: false,
    last_scraped: null,
  };
}

function episode(id, showId, hasAudio = true) {
  return {
    id,
    show_id: showId,
    air_date: null,
    title: null,
    archive_id: hasAudio ? id : null,
    audio_url: null,
    has_audio: hasAudio,
    favourite: false,
    downloaded: false,
    download_path: null,
    track_count: 0,
    resume_sec: null,
    duration_sec: null,
    completed: false,
    offset_sec: null,
  };
}

function details(entries) {
  return async (candidate) => ({
    show: candidate,
    episodes: entries[candidate.id] ?? [],
  });
}

async function test(name, body) {
  await body();
  process.stdout.write(`ok - ${name}\n`);
}

await test("starts at a random episode instead of the oldest queue entry", async () => {
  const a = show("a");
  const result = await selectRandomPlayback(
    [a],
    details({ a: [episode(3, "a"), episode(2, "a"), episode(1, "a")] }),
    null,
    null,
    () => 0.5,
  );

  assert.deepEqual(result?.episodes.map((entry) => entry.id), [1, 2, 3]);
  assert.equal(result?.index, 1);
  assert.equal(result?.episodes[result.index].id, 2);
});

await test("skips shows without playable archives", async () => {
  const a = show("a");
  const b = show("b");
  const result = await selectRandomPlayback(
    [a, b],
    details({ a: [episode(1, "a", false)], b: [episode(2, "b")] }),
    null,
    null,
    () => 0.999,
  );

  assert.equal(result?.show.id, "b");
});

await test("avoids the current show when a playable alternative exists", async () => {
  const current = show("current");
  const other = show("other");
  const result = await selectRandomPlayback(
    [current, other],
    details({ current: [episode(1, "current")], other: [episode(2, "other")] }),
    "current",
    1,
    () => 0,
  );

  assert.equal(result?.show.id, "other");
});

await test("falls back to the current show but avoids its current episode", async () => {
  const current = show("current");
  const other = show("other");
  const result = await selectRandomPlayback(
    [current, other],
    details({
      current: [episode(2, "current"), episode(1, "current")],
      other: [episode(3, "other", false)],
    }),
    "current",
    1,
    () => 0,
  );

  assert.equal(result?.show.id, "current");
  assert.equal(result?.episodes[result.index].id, 2);
});

await test("returns null when no playable archive exists", async () => {
  const a = show("a");
  const result = await selectRandomPlayback(
    [a],
    details({ a: [episode(1, "a", false)] }),
  );
  assert.equal(result, null);
});

await test("returns null for an empty catalog without loading details", async () => {
  let loadCount = 0;
  const result = await selectRandomPlayback([], async () => {
    loadCount += 1;
    throw new Error("should not load");
  });

  assert.equal(result, null);
  assert.equal(loadCount, 0);
});

await test("propagates catalog loading failures", async () => {
  const a = show("a");
  await assert.rejects(
    selectRandomPlayback([a], async () => {
      throw new Error("offline");
    }),
    /offline/,
  );
});
