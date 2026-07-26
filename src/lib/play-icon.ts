export type PlayGlyph = "play" | "pause" | "playing";

/**
 * Glyph for a play/pause control.
 *
 * The icon reports the player's *state*, not the action a click performs: a solid
 * triangle while this item is playing, pause bars while it is the loaded item but
 * paused, and the hollow triangle when it isn't loaded at all. Every play control
 * in the app (transport, episode rows, track rows, live) shares this rule, so the
 * accompanying title/aria-label stays action-worded ("Pause episode") — that is
 * what assistive tech should announce.
 */
export function playGlyph(loaded: boolean, playing: boolean): PlayGlyph {
  if (!loaded) return "play";
  return playing ? "playing" : "pause";
}
