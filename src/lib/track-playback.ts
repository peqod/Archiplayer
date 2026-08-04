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
