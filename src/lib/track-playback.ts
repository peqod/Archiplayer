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
