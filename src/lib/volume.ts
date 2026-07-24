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
