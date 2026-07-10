import { invoke } from "@tauri-apps/api/core";

export interface Show {
  id: string;
  name: string;
  dj: string | null;
  description: string | null;
  on_air: boolean;
  episode_count: number;
  favourite: boolean;
  last_scraped: number | null;
}

export interface Episode {
  id: number;
  show_id: string;
  air_date: string | null;
  title: string | null;
  archive_id: number | null;
  audio_url: string | null;
  has_audio: boolean;
  favourite: boolean;
  downloaded: boolean;
  download_path: string | null;
  track_count: number;
  resume_sec: number | null;
  duration_sec: number | null;
  completed: boolean;
}

export interface Track {
  id: number;
  episode_id: number;
  seq: number;
  artist: string | null;
  title: string | null;
  album: string | null;
  label: string | null;
  comments: string | null;
  start_sec: number | null;
  favourite: boolean;
}

export interface ShowDetail {
  show: Show;
  episodes: Episode[];
}

export interface AudioSource {
  url: string;
  local: boolean;
}

export interface TrackHit {
  track: Track;
  show_id: string;
  show_name: string;
  air_date: string | null;
}

export interface SearchResults {
  shows: Show[];
  tracks: TrackHit[];
}

export interface FavouriteShow {
  show: Show;
  added_at: number;
}
export interface FavouriteEpisode {
  episode: Episode;
  show_name: string;
  added_at: number;
}
export interface FavouriteTrack {
  track: Track;
  show_id: string;
  show_name: string;
  air_date: string | null;
  added_at: number;
}
export interface Favourites {
  shows: FavouriteShow[];
  episodes: FavouriteEpisode[];
  tracks: FavouriteTrack[];
}

export interface ShowStat {
  show_id: string;
  show_name: string;
  seconds: number;
  plays: number;
}
export interface EpisodeStat {
  episode_id: number;
  show_name: string;
  air_date: string | null;
  title: string | null;
  seconds: number;
  plays: number;
}
export interface Stats {
  total_seconds: number;
  total_sessions: number;
  shows: ShowStat[];
  episodes: EpisodeStat[];
}

export interface Download {
  episode_id: number;
  path: string;
  bytes: number;
  total: number;
  status: string;
}

export interface DownloadRow {
  download: Download;
  show_id: string | null;
  show_name: string | null;
  air_date: string | null;
  title: string | null;
  has_audio: boolean;
}

export const api = {
  getCatalog: (refresh = false) => invoke<Show[]>("get_catalog", { refresh }),
  getShow: (showId: string, refresh = false) =>
    invoke<ShowDetail>("get_show", { showId, refresh }),
  getPlaylist: (episodeId: number, refresh = false) =>
    invoke<Track[]>("get_playlist", { episodeId, refresh }),
  resolveAudio: (episodeId: number) =>
    invoke<AudioSource>("resolve_audio", { episodeId }),
  toggleFavourite: (kind: "show" | "episode" | "track", refId: string) =>
    invoke<boolean>("toggle_favourite", { kind, refId }),
  listFavourites: () => invoke<Favourites>("list_favourites"),
  search: (query: string) => invoke<SearchResults>("search", { query }),
  recordListen: (
    sessionId: string,
    episodeId: number,
    seconds: number,
    completed: boolean,
    position: number,
    duration: number,
  ) =>
    invoke<void>("record_listen", {
      sessionId,
      episodeId,
      seconds,
      completed,
      position,
      duration,
    }),
  getStats: () => invoke<Stats>("get_stats"),
  listDownloads: () => invoke<DownloadRow[]>("list_downloads"),
  deleteDownload: (episodeId: number) =>
    invoke<void>("delete_download", { episodeId }),
  downloadEpisode: (episodeId: number) =>
    invoke<string>("download_episode", { episodeId }),
  getDownloadDir: () => invoke<string>("get_download_dir"),
  setDownloadDir: (dir: string) => invoke<void>("set_download_dir", { dir }),
  exportCsv: (kind: "favourites" | "listens" | "stats", dest: string) =>
    invoke<string>("export_csv", { kind, dest }),
};

export function fmtTime(totalSec: number): string {
  if (!isFinite(totalSec) || totalSec < 0) return "0:00";
  const s = Math.floor(totalSec);
  const h = Math.floor(s / 3600);
  const m = Math.floor((s % 3600) / 60);
  const sec = s % 60;
  return h > 0
    ? `${h}:${String(m).padStart(2, "0")}:${String(sec).padStart(2, "0")}`
    : `${m}:${String(sec).padStart(2, "0")}`;
}

export function fmtHours(seconds: number): string {
  const h = seconds / 3600;
  return h >= 1 ? `${h.toFixed(1)} h` : `${Math.round(seconds / 60)} min`;
}
