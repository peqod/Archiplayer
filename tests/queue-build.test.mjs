import assert from "node:assert/strict";
import {
  episodeQueueFrom,
  nextEpisodeIndex,
  nextQueueIndex,
  prevEpisodeIndex,
  prevQueueIndex,
  shuffledFrom,
  songQueueFrom,
} from "../src/lib/queue-build.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

const episode = (id, hasAudio = true) => ({
  id,
  show_id: "WB",
  air_date: null,
  title: null,
  archive_id: null,
  audio_url: null,
  has_audio: hasAudio,
  favourite: true,
  downloaded: false,
  download_path: null,
  track_count: 0,
  resume_sec: null,
  duration_sec: null,
  completed: false,
  offset_sec: null,
  broadcast_duration_sec: null,
});

const song = (id, episodeId, startSec, hasAudio = true) => ({
  id,
  episode: episode(episodeId, hasAudio),
  showName: "Wake N Bake",
  artist: "Sun Ra",
  title: `Song ${id}`,
  startSec,
});

// -- nextQueueIndex / prevQueueIndex ----------------------------------------

test("the queue stops at its last entry unless repeat is on", () => {
  assert.equal(nextQueueIndex(0, 3, false), 1);
  assert.equal(nextQueueIndex(2, 3, false), -1);
  assert.equal(nextQueueIndex(2, 3, true), 0);
  assert.equal(nextQueueIndex(0, 0, true), -1);
});

test("stepping back from the first entry wraps only under repeat", () => {
  assert.equal(prevQueueIndex(2, 3, false), 1);
  assert.equal(prevQueueIndex(0, 3, false), -1);
  assert.equal(prevQueueIndex(0, 3, true), 2);
  assert.equal(prevQueueIndex(0, 0, false), -1);
});

test("a single-entry queue wraps onto itself with repeat", () => {
  assert.equal(nextQueueIndex(0, 1, false), -1);
  assert.equal(nextQueueIndex(0, 1, true), 0);
  assert.equal(prevQueueIndex(0, 1, true), 0);
});

// -- nextEpisodeIndex / prevEpisodeIndex ------------------------------------

test("the episode step skips every remaining song of the current episode", () => {
  const ids = [7, 7, 7, 9, 9, 4];
  assert.equal(nextEpisodeIndex(ids, 0, false), 3);
  assert.equal(nextEpisodeIndex(ids, 2, false), 3);
  assert.equal(nextEpisodeIndex(ids, 4, false), 5);
  assert.equal(nextEpisodeIndex(ids, 5, false), -1);
  assert.equal(nextEpisodeIndex(ids, 5, true), 0);
});

test("stepping back lands on the first song of the previous episode, not its last", () => {
  const ids = [7, 7, 7, 9, 9, 4];
  assert.equal(prevEpisodeIndex(ids, 4, false), 0);
  assert.equal(prevEpisodeIndex(ids, 5, false), 3);
  assert.equal(prevEpisodeIndex(ids, 1, false), -1);
  assert.equal(prevEpisodeIndex(ids, 0, true), 5);
});

test("a queue of one episode has no episode to step to", () => {
  assert.equal(nextEpisodeIndex([3, 3, 3], 0, true), -1);
  assert.equal(prevEpisodeIndex([3, 3, 3], 2, true), -1);
  assert.equal(nextEpisodeIndex([], 0, true), -1);
});

test("interleaved episodes step on every change, not on blocks", () => {
  const ids = [1, 2, 1, 2];
  assert.equal(nextEpisodeIndex(ids, 0, false), 1);
  assert.equal(nextEpisodeIndex(ids, 1, false), 2);
  assert.equal(prevEpisodeIndex(ids, 3, false), 2);
});

// -- shuffledFrom -----------------------------------------------------------

test("shuffle leaves the entry that is playing and everything before it alone", () => {
  const items = [1, 2, 3, 4, 5, 6];
  const out = shuffledFrom(items, 2, () => 0);
  assert.deepEqual(out.slice(0, 3), [1, 2, 3]);
  assert.deepEqual([...out].sort((a, b) => a - b), items);
});

test("shuffle is a permutation and is deterministic under a pinned rand", () => {
  const items = ["a", "b", "c", "d", "e"];
  const seq = [0.9, 0.1, 0.5, 0.7];
  let i = 0;
  const rand = () => seq[i++ % seq.length];
  const once = shuffledFrom(items, -1, rand);
  i = 0;
  const twice = shuffledFrom(items, -1, rand);
  assert.deepEqual(once, twice);
  assert.deepEqual([...once].sort(), [...items].sort());
  assert.equal(once.length, items.length);
});

test("shuffle of a short or fully consumed list is the identity", () => {
  assert.deepEqual(shuffledFrom([], -1, () => 0), []);
  assert.deepEqual(shuffledFrom(["a"], -1, () => 0), ["a"]);
  assert.deepEqual(shuffledFrom(["a", "b", "c"], 2, () => 0), ["a", "b", "c"]);
});

// -- songQueueFrom ----------------------------------------------------------

test("a song queue runs from the clicked row to the end of the list", () => {
  const rows = [song(1, 10, 0), song(2, 10, 120), song(3, 11, 30)];
  const queue = songQueueFrom(rows, 2);
  assert.deepEqual(
    queue.map((q) => [q.episode.id, q.song.id, q.song.startSec]),
    [
      [10, 2, 120],
      [11, 3, 30],
    ],
  );
  assert.equal(queue[0].showName, "Wake N Bake");
  assert.equal(queue[0].song.title, "Song 2");
});

test("rows with no timecode, no audio or no episode drop out of the tail", () => {
  const rows = [
    song(1, 10, 0),
    song(2, 10, null),
    song(3, 11, 30, false),
    { ...song(4, 12, 60), episode: null },
    song(5, 13, 90),
  ];
  assert.deepEqual(
    songQueueFrom(rows, 1).map((q) => q.song.id),
    [1, 5],
  );
});

test("clicking an unplayable row, or one that is not in the list, queues nothing", () => {
  const rows = [song(1, 10, null), song(2, 11, 30, false), song(3, 12, 60)];
  assert.deepEqual(songQueueFrom(rows, 1), []);
  assert.deepEqual(songQueueFrom(rows, 2), []);
  assert.deepEqual(songQueueFrom(rows, 99), []);
});

test("the song queue keeps the order it is given, including a grouped one", () => {
  // Grouped by artist, so the episodes interleave and the timecodes run backwards.
  const rows = [song(3, 11, 300), song(1, 10, 600), song(2, 10, 60)];
  assert.deepEqual(
    songQueueFrom(rows, 3).map((q) => [q.episode.id, q.song.startSec]),
    [
      [11, 300],
      [10, 600],
      [10, 60],
    ],
  );
});

// -- episodeQueueFrom -------------------------------------------------------

test("an episode queue runs from the clicked row and carries no song", () => {
  const rows = [
    { id: 10, episode: episode(10), showName: "A" },
    { id: 11, episode: episode(11), showName: "B" },
    { id: 12, episode: episode(12), showName: "C" },
  ];
  const queue = episodeQueueFrom(rows, 11);
  assert.deepEqual(
    queue.map((q) => [q.episode.id, q.showName]),
    [
      [11, "B"],
      [12, "C"],
    ],
  );
  assert.equal(queue[0].song, undefined);
});

test("episode rows with no stored episode or no audio are left out", () => {
  const rows = [
    { id: 10, episode: null, showName: "A" },
    { id: 11, episode: episode(11, false), showName: "B" },
    { id: 12, episode: episode(12), showName: "C" },
  ];
  // The clicked row has no episode, so the caller resolves it and leads this remainder.
  assert.deepEqual(
    episodeQueueFrom(rows, 10).map((q) => q.episode.id),
    [12],
  );
  assert.deepEqual(episodeQueueFrom(rows, 99), []);
});
