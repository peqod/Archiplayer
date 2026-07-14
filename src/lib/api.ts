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
  offset_sec: number | null;
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
  source_id: string | null;
  played_at: number | null;
  favourite: boolean;
}

export interface ShowDetail {
  show: Show;
  episodes: Episode[];
}

export interface AudioSource {
  url: string;
  local: boolean;
  offset_sec: number;
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

export interface LiveStream {
  id: string;
  name: string;
  tagline: string;
  url: string;
  status_source: LiveStatusSource;
}

export type LiveStatusSource =
  | { kind: "channel"; channel_id: number }
  | { kind: "homepage" };

export interface LiveSong {
  artist: string | null;
  title: string | null;
}

export interface LiveStatus {
  episode: Episode;
  show_name: string;
  current_song: LiveSong | null;
  tracks: Track[];
  current_track_id: number | null;
  playlist_needs_load: boolean;
}

export interface LiveProgram {
  show_id: string | null;
  name: string;
  host: string | null;
  description: string | null;
  starts_at: string | null;
  ends_at: string | null;
}

export interface LivePage {
  tracks: Track[];
  current_track_id: number | null;
  current_show: LiveProgram | null;
  upcoming_shows: LiveProgram[];
  history_source: "radio_rethink" | "local_cache";
  warning: string | null;
  updated_at: number;
}

// WFMU's 24/7 live channels. URLs come from wfmu.org's .pls playlists; the https
// form works and is already covered by the app CSP (media-src https://*.wfmu.org),
// so they play through the same <audio> element as archives — no backend round-trip.
export const LIVE_STREAMS: LiveStream[] = [
  {
    id: "freeform",
    name: "WFMU 91.1",
    tagline: "Freeform radio the way it oughta be",
    url: "https://stream0.wfmu.org/freeform-128k",
    status_source: { kind: "homepage" },
  },
  {
    id: "drummer",
    name: "Give the Drummer Radio",
    tagline: "WFMU's eclectic web-only channel",
    url: "https://stream0.wfmu.org/drummer",
    status_source: { kind: "channel", channel_id: 4 },
  },
  {
    id: "rocknsoul",
    name: "Rock'n'Soul Ichiban",
    tagline: "Vintage rock & soul 45s, round the clock",
    url: "https://stream0.wfmu.org/rocknsoul",
    status_source: { kind: "channel", channel_id: 6 },
  },
  {
    id: "sheena",
    name: "Sheena's Jungle Room",
    tagline: "Wild rock'n'roll, garage & exotica",
    url: "https://stream0.wfmu.org/sheena",
    status_source: { kind: "channel", channel_id: 8 },
  },
];

export const api = {
  getCatalog: (refresh = false) => invoke<Show[]>("get_catalog", { refresh }),
  getShow: (showId: string, refresh = false) =>
    invoke<ShowDetail>("get_show", { showId, refresh }),
  getPlaylist: (episodeId: number, refresh = false) =>
    invoke<Track[]>("get_playlist", { episodeId, refresh }),
  getLiveStatus: (
    streamId: string,
    statusSource: LiveStatusSource,
    fallbackName: string,
  ) =>
    invoke<LiveStatus>("get_live_status", {
      streamId,
      statusSource,
      fallbackName,
    }),
  getLivePage: (streamId: string, refresh = false) =>
    invoke<LivePage>("get_live_page", { streamId, refresh }),
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
