export interface Point {
  left: number;
  top: number;
}

export interface RectLike {
  top: number;
  left: number;
  bottom: number;
}

// Minimum room to leave below the pill so it never sits flush against the
// bottom edge of the scroll area.
const BOTTOM_GUARD = 48;

/**
 * Viewport coordinates the inline back link occupies when <main> is unscrolled.
 *
 * Valid because the shell never scrolls (.app is overflow:clip) — only <main>
 * does, so adding back its scrollTop undoes the only offset in play.
 *
 * `pad` shifts the anchor so the floating pill's *text* lands exactly where the
 * plain link's text was, cancelling the pill's own padding. Without it the two
 * states would jump by the padding on hand-off.
 */
export function restingAnchor(
  backRect: RectLike,
  mainRect: RectLike,
  mainScrollTop: number,
  pad: Point,
): Point {
  const top = backRect.top + mainScrollTop - pad.top;
  const lowest = Math.max(mainRect.top, mainRect.bottom - BOTTOM_GUARD);
  return {
    left: backRect.left - pad.left,
    top: Math.min(Math.max(top, mainRect.top), lowest),
  };
}
