import type { Episode } from "$lib/api";

/**
 * The player's queue is a list of things to play in order. An entry is either a whole
 * episode or one song inside an episode, which is what makes a list of favourite songs
 * playable: consecutive entries can name the same audio file at different positions, or
 * different files entirely.
 *
 * The types live here rather than in `player.svelte.ts` so the pure queue helpers, and
 * the node tests that exercise them, never have to load a module full of runes.
 */
export interface QueueSong {
  /** Playlist track id, the same id the favourites table stores. */
  id: number;
  /** Show-relative timecode. The audio position is `startSec + offset`. */
  startSec: number;
  artist: string | null;
  title: string | null;
}

export interface QueueItem {
  episode: Episode;
  showName: string;
  /** Present when the entry is one song; absent when it is the whole episode. */
  song?: QueueSong | null;
}

// -- Order ------------------------------------------------------------------
//
// All four return -1 for "nowhere to go", matching the player's own `queueIndex = -1`
// idiom for an empty queue.

export function nextQueueIndex(index: number, length: number, repeat: boolean): number {
  if (length <= 0) return -1;
  if (index < length - 1) return index + 1;
  return repeat ? 0 : -1;
}

export function prevQueueIndex(index: number, length: number, repeat: boolean): number {
  if (length <= 0) return -1;
  if (index > 0) return index - 1;
  return repeat ? length - 1 : -1;
}

/**
 * First entry of the next run of entries sharing an episode. A song queue sorted by
 * artist can interleave episodes, so this walks until the id actually changes rather
 * than assuming one episode is one contiguous block.
 */
export function nextEpisodeIndex(
  episodeIds: number[],
  index: number,
  repeat: boolean,
): number {
  const n = episodeIds.length;
  if (n === 0 || index < 0 || index >= n) return -1;
  const from = episodeIds[index];
  for (let step = 1; step < n; step++) {
    const at = index + step;
    if (at >= n && !repeat) return -1;
    const i = at % n;
    if (episodeIds[i] !== from) return i;
  }
  return -1;
}

/**
 * Start of the previous run, not its end: stepping back should open that episode at its
 * first queued song, the same way stepping forward does.
 */
export function prevEpisodeIndex(
  episodeIds: number[],
  index: number,
  repeat: boolean,
): number {
  const n = episodeIds.length;
  if (n === 0 || index < 0 || index >= n) return -1;
  const from = episodeIds[index];
  let found = -1;
  for (let step = 1; step < n; step++) {
    const at = index - step;
    if (at < 0 && !repeat) return -1;
    const i = ((at % n) + n) % n;
    if (episodeIds[i] !== from) {
      found = i;
      break;
    }
  }
  if (found < 0) return -1;
  const target = episodeIds[found];
  let start = found;
  while (start > 0 && episodeIds[start - 1] === target) start--;
  return start;
}

/**
 * Fisher-Yates over the entries after `keepIndex`; everything up to and including it
 * keeps its place. Shuffling mid-run must not move what is playing, nor rewrite the
 * history behind the playhead. `keepIndex = -1` permutes the whole list.
 *
 * `rand` is injected so tests can pin the permutation.
 */
export function shuffledFrom<T>(
  items: T[],
  keepIndex: number,
  rand: () => number = Math.random,
): T[] {
  const out = [...items];
  const from = Math.max(keepIndex + 1, 0);
  for (let i = out.length - 1; i > from; i--) {
    const j = from + Math.floor(rand() * (i - from + 1));
    const swap = out[i];
    out[i] = out[j];
    out[j] = swap;
  }
  return out;
}

// -- Builders ---------------------------------------------------------------

export interface SongQueueRow {
  id: number;
  /** Null for a row whose episode could not be resolved; such a row cannot be queued. */
  episode: Episode | null;
  showName: string;
  artist: string | null;
  title: string | null;
  startSec: number | null;
}

export interface EpisodeQueueRow {
  id: number;
  episode: Episode | null;
  showName: string;
}

function playableSong(row: SongQueueRow): boolean {
  return row.episode !== null && row.episode.has_audio && row.startSec !== null;
}

/**
 * The clicked row and everything below it, in the order the list renders. Rows with no
 * timecode, no audio, or no resolvable episode drop out of the tail; if the clicked row
 * itself is one of those, the whole thing is empty and the caller does nothing.
 */
export function songQueueFrom(rows: SongQueueRow[], fromId: number): QueueItem[] {
  const at = rows.findIndex((r) => r.id === fromId);
  if (at < 0 || !playableSong(rows[at])) return [];
  const out: QueueItem[] = [];
  for (const row of rows.slice(at)) {
    if (!playableSong(row)) continue;
    out.push({
      episode: row.episode as Episode,
      showName: row.showName,
      song: {
        id: row.id,
        startSec: row.startSec as number,
        artist: row.artist,
        title: row.title,
      },
    });
  }
  return out;
}

/**
 * The episode counterpart. A listened-only row carries no stored `Episode`, so it is
 * dropped here and the caller resolves the clicked one on its own.
 */
export function episodeQueueFrom(
  rows: EpisodeQueueRow[],
  fromId: number,
): QueueItem[] {
  const at = rows.findIndex((r) => r.id === fromId);
  if (at < 0) return [];
  const out: QueueItem[] = [];
  for (const row of rows.slice(at)) {
    if (!row.episode || !row.episode.has_audio) continue;
    out.push({ episode: row.episode, showName: row.showName });
  }
  return out;
}
