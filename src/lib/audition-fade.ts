export const AUDITION_FADE_SEC = 1.5;

function clampGain(value: number): number {
  if (!Number.isFinite(value)) return 1;
  return Math.max(0, Math.min(1, value));
}

/** Linear gain for the opening ramp, expressed in elapsed playback seconds. */
export function auditionFadeInGain(elapsedSec: number): number {
  return clampGain(elapsedSec / AUDITION_FADE_SEC);
}

/** Linear fade to silence when leaving a live source for another source. */
export function transitionFadeOutGain(
  elapsedSec: number,
  startingGain = 1,
): number {
  if (!Number.isFinite(elapsedSec)) return clampGain(startingGain);
  return clampGain(
    clampGain(startingGain) * (1 - auditionFadeInGain(elapsedSec)),
  );
}

/** Linear gain over the final 1.5 seconds of the logical archive timeline. */
export function auditionFadeOutGain(currentSec: number, endSec: number): number {
  if (!Number.isFinite(currentSec) || !Number.isFinite(endSec) || endSec <= 0) return 1;
  return clampGain((endSec - currentSec) / AUDITION_FADE_SEC);
}
