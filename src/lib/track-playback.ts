export function hasExactTrackTimestamp(startSec: number | null): startSec is number {
  return startSec !== null;
}

export function canPlayExactTrack(
  episodeHasAudio: boolean,
  startSec: number | null,
): boolean {
  return episodeHasAudio && hasExactTrackTimestamp(startSec);
}
