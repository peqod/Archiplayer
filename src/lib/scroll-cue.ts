// When the nav brand stops being a wordmark and becomes a scroll-to-top control.
//
// The show and live views already say "you have scrolled" once: the inline
// "← All shows" link leaves the top of <main> and its floating pill takes over
// (see BackButton). The brand flips on the same travel so the two read as one
// change of state rather than two unrelated ones — but by measurement rather
// than by that link, because the catalog and profile views have no back link
// and the brand still has to offer the ride back up there.
//
// <main>'s own top padding, from .app main in +layout.svelte.
export const MAIN_PAD_TOP = 20;
// One line of the inline back link: 15px body text at the WebView's normal
// line-height. It sits directly under that padding with nothing above it.
export const BACK_LINE_H = 18;
// The link's bottom edge has just cleared the top of the scroll area, which is
// the exact scrollTop at which BackButton's IntersectionObserver hands off.
export const SCROLL_CUE_AT = MAIN_PAD_TOP + BACK_LINE_H;

/**
 * Whether the nav brand should be showing its scroll-to-top cue.
 *
 * One threshold with no hysteresis band, deliberately: a band would put the
 * brand and the back pill on different edges and let them disagree over a
 * stretch of scroll, which is the one thing this is trying to avoid.
 */
export function scrollCueVisible(scrollTop: number): boolean {
  return Number.isFinite(scrollTop) && scrollTop >= SCROLL_CUE_AT;
}
