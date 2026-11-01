export function hasExactTrackTimestamp(startSec: number | null): startSec is number {
  return startSec !== null;
}

export function canPlayExactTrack(
  episodeHasAudio: boolean,
  startSec: number | null,
): boolean {
  return episodeHasAudio && hasExactTrackTimestamp(startSec);
}

/**
 * A favourite song plays when its episode has audio. Without a timecode or a start of the
 * listener's own it starts at the top of the episode (see `effectiveStart`).
 */
export function canPlayFavouriteSong(episodeHasAudio: boolean): boolean {
  return episodeHasAudio;
}

/**
 * Show-relative second a favourite song starts at: the listener's own start, else the
 * playlist timecode, else 0 (the episode start). `known` is false only for that last
 * fallback, which says nothing about where the song really is.
 */
export function effectiveStart(
  startSec: number | null | undefined,
  manualStartSec: number | null | undefined,
): { sec: number; known: boolean; manual: boolean } {
  if (manualStartSec != null && manualStartSec >= 0) {
    return { sec: manualStartSec, known: true, manual: true };
  }
  if (startSec != null) return { sec: startSec, known: true, manual: false };
  return { sec: 0, known: false, manual: false };
}

/**
 * Show-relative second at which a queued song ends: the next timecoded playlist entry
 * strictly after it. Null means "run to the end of the episode", which covers both the
 * last timecoded song and a playlist that has not loaded yet.
 *
 * Matched on `start_sec` rather than on position in the list, so a playlist whose `seq`
 * and timecodes disagree still yields a boundary ahead of the song rather than behind it.
 */
export function segmentEndSec(
  tracks: { start_sec: number | null }[],
  startSec: number,
): number | null {
  let end: number | null = null;
  for (const track of tracks) {
    const at = track.start_sec;
    if (at === null || at <= startSec) continue;
    if (end === null || at < end) end = at;
  }
  return end;
}

/** Slack after a looked-up length, for a slow fade-in or a DJ's late cue. */
export const SONG_LENGTH_GRACE_SEC = 5;

/** A manual end closer than this to the start is a misclick, not a song. */
export const MIN_SONG_SEC = 10;

export type SongEndSource = "manual" | "length" | "next" | "episode";

/**
 * Show-relative second a queued song stops at, and why. Order:
 * 1. the listener's own mark, when it lies after the start;
 * 2. the earlier of the next timecode and `start + length + grace`, so a song followed by
 *    DJ talk stops when the record does;
 * 3. whichever of those two exists;
 * 4. null: the episode's own end.
 *
 * With `startKnown` false the start is only the 0:00 fallback, so neither the next
 * timecode nor the length says anything about this song: only the listener's own mark
 * or the episode end apply.
 */
export function songEnd(
  tracks: { start_sec: number | null }[],
  startSec: number,
  manualEndSec: number | null | undefined,
  lengthSec: number | null | undefined,
  startKnown = true,
): { endSec: number | null; source: SongEndSource } {
  if (manualEndSec != null && manualEndSec >= startSec + MIN_SONG_SEC) {
    return { endSec: manualEndSec, source: "manual" };
  }
  if (!startKnown) return { endSec: null, source: "episode" };
  const next = segmentEndSec(tracks, startSec);
  const byLength =
    lengthSec != null && lengthSec > 0 ? startSec + lengthSec + SONG_LENGTH_GRACE_SEC : null;
  if (next !== null && (byLength === null || next <= byLength)) {
    return { endSec: next, source: "next" };
  }
  if (byLength !== null) return { endSec: byLength, source: "length" };
  return { endSec: null, source: "episode" };
}

/**
 * "m:ss" or "h:mm:ss" (what `fmtTime` writes) to seconds. Null for anything else, so a
 * typo is reported instead of saved as a wrong end.
 */
export function parseClock(text: string): number | null {
  const parts = text.trim().split(":");
  if (parts.length < 2 || parts.length > 3) return null;
  if (!parts.every((p) => /^\d+$/.test(p))) return null;
  const nums = parts.map(Number);
  // Every field after the first is a 0-59 count.
  if (nums.slice(1).some((n) => n > 59)) return null;
  return nums.reduce((total, n) => total * 60 + n, 0);
}

/**
 * Audio-time seconds of the playlist entries that carry a timecode, deduped, sorted,
 * and clamped to the episode. Playlist `start_sec` is show-relative, so the audio
 * position is `start_sec + offset` — the same translation playback seeking uses.
 * Empty when nothing is timecoded or the duration is not known yet.
 */
export function trackMarkSeconds(
  tracks: { start_sec: number | null }[],
  offset: number,
  duration: number,
): number[] {
  if (!Number.isFinite(duration) || duration <= 0) return [];
  const seen = new Set<number>();
  for (const track of tracks) {
    if (!hasExactTrackTimestamp(track.start_sec)) continue;
    const at = track.start_sec + offset;
    if (at < 0 || at > duration) continue;
    seen.add(at);
  }
  return [...seen].sort((a, b) => a - b);
}
