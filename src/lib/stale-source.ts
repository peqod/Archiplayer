// Recovery rules for an archive URL that has gone dead since it was cached.
//
// WFMU keeps its 128k mp3s on mp3archives.wfmu.org for a few weeks only; once one rolls off,
// the station's own player falls back to the permanent s3.amazonaws.com/arch.wfmu.org mp4 and
// the mp3 answers 404. A URL cached before that switch never plays again, so a failed media
// source earns exactly one forced re-resolve.

export interface StaleSourceState {
  /** A live stream is playing: its failures are outages, not stale archive URLs. */
  live: boolean;
  /** The source is a local download: a missing file is not fixed by re-scraping WFMU. */
  local: boolean;
  /** Episode whose source the media element is currently holding. */
  sourceEpisodeId: number | null;
  /** Episode already re-resolved once, so a genuinely dead archive can't loop. */
  retriedEpisodeId: number | null;
  /** Episode the player is on now, which a skip during the failure may have changed. */
  currentEpisodeId: number | null;
  /** False once a newer load has superseded the one that installed the source. */
  sourceIsCurrent: boolean;
}

export function shouldRetryStaleSource(state: StaleSourceState): boolean {
  if (state.live || state.local) return false;
  if (state.sourceEpisodeId === null) return false;
  if (state.retriedEpisodeId === state.sourceEpisodeId) return false;
  if (state.currentEpisodeId !== state.sourceEpisodeId) return false;
  return state.sourceIsCurrent;
}

/**
 * Where playback resumes on the replacement source, in audio time: the position the dead
 * source reached, else the seek it never got to, else the fresh-play jingle lead-in.
 */
export function staleResumeAt(
  currentTime: number,
  pendingSeek: number | null,
  offset: number,
  leadInSec: number,
): number {
  if (currentTime > 0) return currentTime;
  if (pendingSeek !== null) return pendingSeek;
  return Math.max(0, offset - leadInSec);
}
