import type { Episode, Show, ShowDetail } from "./api";

export type RandomSource = () => number;

export interface RandomPlaybackSelection {
  show: Show;
  episodes: Episode[];
  index: number;
}

function randomIndex(length: number, random: RandomSource): number {
  if (length <= 1) return 0;
  const value = random();
  return Math.min(length - 1, Math.max(0, Math.floor(value * length)));
}

function shuffled<T>(items: T[], random: RandomSource): T[] {
  const result = [...items];
  for (let i = result.length - 1; i > 0; i -= 1) {
    const j = randomIndex(i + 1, random);
    [result[i], result[j]] = [result[j], result[i]];
  }
  return result;
}

/**
 * Choose a random playable show and a random starting episode. The API returns
 * episodes newest-first, so the returned queue is reversed for chronological
 * next/previous navigation.
 */
export async function selectRandomPlayback(
  shows: Show[],
  loadShow: (show: Show) => Promise<ShowDetail>,
  currentShowId: string | null = null,
  currentEpisodeId: number | null = null,
  random: RandomSource = Math.random,
): Promise<RandomPlaybackSelection | null> {
  if (!shows.length) return null;

  const alternatives = shows.filter((show) => show.id !== currentShowId);
  const current = shows.filter((show) => show.id === currentShowId);
  const candidateGroups = alternatives.length
    ? [shuffled(alternatives, random), shuffled(current, random)]
    : [shuffled(shows, random)];

  for (const candidates of candidateGroups) {
    for (const candidate of candidates) {
      const detail = await loadShow(candidate);
      const episodes = detail.episodes.filter((episode) => episode.has_audio).reverse();
      if (!episodes.length) continue;

      const nonRepeatingIndexes = episodes
        .map((episode, index) => ({ episode, index }))
        .filter(({ episode }) => episode.id !== currentEpisodeId)
        .map(({ index }) => index);
      const eligibleIndexes = nonRepeatingIndexes.length
        ? nonRepeatingIndexes
        : episodes.map((_, index) => index);
      const index = eligibleIndexes[randomIndex(eligibleIndexes.length, random)];

      return { show: detail.show, episodes, index };
    }
  }

  return null;
}
