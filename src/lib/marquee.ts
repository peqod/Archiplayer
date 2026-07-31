export const MARQUEE_GAP = 32;
export const MARQUEE_SPEED = 30;

export type MarqueeMetrics = {
  overflowing: boolean;
  distance: number;
  duration: number;
};

export function sameMarqueeMetrics(a: MarqueeMetrics, b: MarqueeMetrics): boolean {
  return (
    a.overflowing === b.overflowing &&
    a.distance === b.distance &&
    a.duration === b.duration
  );
}

/**
 * Keep marquee motion at a constant reading speed regardless of title length.
 * A one-pixel tolerance prevents fractional layout rounding from starting a loop.
 */
export function marqueeMetrics(
  viewportWidth: number,
  contentWidth: number,
  gap = MARQUEE_GAP,
  speed = MARQUEE_SPEED,
): MarqueeMetrics {
  if (
    !Number.isFinite(viewportWidth) ||
    !Number.isFinite(contentWidth) ||
    !Number.isFinite(gap) ||
    !Number.isFinite(speed) ||
    viewportWidth <= 0 ||
    contentWidth <= 0 ||
    gap < 0 ||
    speed <= 0 ||
    contentWidth <= viewportWidth + 1
  ) {
    return { overflowing: false, distance: 0, duration: 0 };
  }

  const distance = contentWidth + gap;
  return {
    overflowing: true,
    distance,
    duration: distance / speed,
  };
}
