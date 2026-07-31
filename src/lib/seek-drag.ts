/**
 * Geared scrubbing for the loupe playhead. A press jumps to the pressed point; every
 * pixel of travel after that is worth `1 / LOUPE_ZOOM` of a bar pixel, so the seek
 * resolution matches the magnification the lens is showing.
 */
export const LOUPE_ZOOM = 4;
export const LOUPE_SIZE = 36;
export const MIN_PRECISION_SEEK_WIDTH = 160;

export function precisionSeekAvailable(trackWidth: number): boolean {
  return Number.isFinite(trackWidth) && trackWidth >= MIN_PRECISION_SEEK_WIDTH;
}

function clampToEpisode(sec: number, duration: number): number {
  if (!Number.isFinite(sec)) return 0;
  return Math.max(0, Math.min(sec, duration));
}

/**
 * Seconds under a pointer x. Measure the painted `.hsl-track`, not the row: the track
 * is already inset by half a thumb on each side, so its own box is the travel range.
 */
export function timeAtPointer(
  clientX: number,
  trackLeft: number,
  trackWidth: number,
  duration: number,
): number {
  if (!(trackWidth > 0) || !(duration > 0)) return 0;
  return clampToEpisode(((clientX - trackLeft) / trackWidth) * duration, duration);
}

/** Where a geared drag has reached: travel from the press point, divided by the zoom. */
export function gearedTime(
  anchorSec: number,
  anchorX: number,
  clientX: number,
  trackWidth: number,
  duration: number,
  zoom: number = LOUPE_ZOOM,
): number {
  // A bar with no width, no episode, or travel that is not a number leaves the press
  // where it landed. Holding the anchor is the only safe answer: flinging to the start
  // of the show would be a worse lie than not moving.
  if (
    !(trackWidth > 0) ||
    !(duration > 0) ||
    !(zoom > 0) ||
    !Number.isFinite(clientX - anchorX)
  ) {
    return clampToEpisode(anchorSec, duration);
  }
  const secPerPx = duration / trackWidth;
  return clampToEpisode(anchorSec + ((clientX - anchorX) * secPerPx) / zoom, duration);
}
