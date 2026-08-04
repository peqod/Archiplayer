/**
 * Bring the row carrying `id` on `attribute` into view.
 *
 * `block` is the caller's call because the two uses want opposite things. Arriving at a
 * list wants the row centred, since there is no previous scroll position to respect.
 * A list that follows a playhead wants "nearest", which does nothing at all while the row
 * is already on screen, so an advancing queue never yanks the page out from under someone
 * who is reading it.
 *
 * Returns false when the row is not rendered, which is the normal case for a row behind a
 * filter or below the visible cap.
 */
export function centerRow(
  container: ParentNode | null,
  attribute: string,
  id: number,
  reducedMotion: boolean,
  block: ScrollLogicalPosition = "center",
): boolean {
  if (!container || !Number.isSafeInteger(id) || id <= 0) return false;

  const row = container.querySelector<HTMLElement>(`[${attribute}="${id}"]`);
  if (!row) return false;

  row.scrollIntoView({
    behavior: reducedMotion ? "auto" : "smooth",
    block,
    inline: "nearest",
  });
  return true;
}

export function centerEpisodeRow(
  container: ParentNode | null,
  episodeId: number,
  reducedMotion: boolean,
): boolean {
  return centerRow(container, "data-episode-id", episodeId, reducedMotion);
}
