/**
 * Printed scale for the volume sliders: a 10% grid, ends called out, midpoint largest.
 * `size` is the relative weight of a tick; the sliders notch the bar by half of it,
 * across whichever axis they run, so the desktop bar and the mini popover read alike.
 */
export const VOLUME_TICKS: { pct: number; size: number }[] = [
  0, 10, 20, 30, 40, 50, 60, 70, 80, 90, 100,
].map((pct) => ({ pct, size: pct === 50 ? 6 : pct % 100 === 0 ? 4 : 2 }));

/** Convert persisted or UI volume input into a safe HTMLMediaElement volume. */
export function normalizeVolume(value: unknown, fallback = 1): number {
  const safeFallback = Number.isFinite(fallback)
    ? Math.max(0, Math.min(1, fallback))
    : 1;
  const parsed =
    typeof value === "number"
      ? value
      : typeof value === "string" && value.trim() !== ""
        ? Number(value)
        : Number.NaN;
  if (!Number.isFinite(parsed)) return safeFallback;
  return Math.max(0, Math.min(1, parsed));
}
