export function centerEpisodeRow(
  container: ParentNode | null,
  episodeId: number,
  reducedMotion: boolean,
): boolean {
  if (!container || !Number.isSafeInteger(episodeId) || episodeId <= 0) return false;

  const row = container.querySelector<HTMLElement>(`[data-episode-id="${episodeId}"]`);
  if (!row) return false;

  row.scrollIntoView({
    behavior: reducedMotion ? "auto" : "smooth",
    block: "center",
    inline: "nearest",
  });
  return true;
}
