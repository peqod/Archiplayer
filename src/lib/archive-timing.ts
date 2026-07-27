/** Keep closing speech, jingles, and slightly late playlist timing inside the episode. */
export const ARCHIVE_END_GRACE_SEC = 30;

/** User-facing archive length, including pre-roll and end grace but excluding spill. */
export function effectiveArchiveDuration(
  mediaDuration: number,
  offsetSec: number,
  broadcastDurationSec: number | null,
): number {
  if (mediaDuration <= 0 || Number.isNaN(mediaDuration)) return 0;
  if (
    broadcastDurationSec === null ||
    !Number.isFinite(broadcastDurationSec) ||
    broadcastDurationSec <= 0
  )
    return mediaDuration;
  const scheduledEnd =
    Math.max(0, offsetSec) + broadcastDurationSec + ARCHIVE_END_GRACE_SEC;
  return Math.min(mediaDuration, scheduledEnd);
}

/** True only for a scheduled cutoff before the physical end of the media file. */
export function scheduledArchiveEndReached(
  currentTime: number,
  mediaDuration: number,
  effectiveDuration: number,
): boolean {
  return (
    Number.isFinite(currentTime) &&
    Number.isFinite(mediaDuration) &&
    Number.isFinite(effectiveDuration) &&
    effectiveDuration > 0 &&
    effectiveDuration < mediaDuration &&
    currentTime >= effectiveDuration
  );
}
