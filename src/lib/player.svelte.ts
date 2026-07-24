import { convertFileSrc } from "@tauri-apps/api/core";
import {
  api,
  type Episode,
  type LiveSong,
  type LiveStatus,
  type LiveStream,
  type Track,
} from "./api";
import { isAbortError, PlaybackTransitions } from "./playback-transition";
import { normalizeVolume } from "./volume";

export interface QueueItem {
  episode: Episode;
  showName: string;
}

export interface LiveEpisode {
  episode: Episode;
  showName: string;
}

const TICK_SECONDS = 15;

// On a fresh play we start this many seconds before the show's playlist zero so the
// audition jingle stays audible, while skipping the bulk of the archive pre-roll
// (the tail of the previous show). See `offset` below.
const INTRO_LEAD_IN_SEC = 15;

class Player {
  audio: HTMLAudioElement | null = null;

  // Archive pre-roll offset (seconds) for the current episode: audio position of the
  // show's playlist zero. Playlist `start_sec` values are show-relative, so the audio
  // position of a track is `start_sec + offset`. 0 for old archives with no lead-in.
  private offset = 0;

  queue = $state<QueueItem[]>([]);
  queueIndex = $state(-1);
  // Non-null while a WFMU live channel is playing. The episode is the current live
  // playlist context; audio still comes from the station stream rather than the archive.
  live = $state<LiveStream | null>(null);
  liveEpisode = $state<LiveEpisode | null>(null);
  liveSong = $state<LiveSong | null>(null);
  livePlaylistLoading = $state(false);
  livePlaylistError = $state<string | null>(null);
  private liveTrackIndex = $state(-1);
  private liveStatusTimer: ReturnType<typeof setInterval> | null = null;
  private liveRefreshGeneration: number | null = null;
  private livePlaylistLoads = new Map<number, Promise<Track[]>>();
  private transitions = new PlaybackTransitions();
  tracks = $state<Track[]>([]);
  playing = $state(false);
  loading = $state(false);
  currentTime = $state(0);
  duration = $state(0);
  volume = $state(1);
  muted = $state(false);
  private preMuteVolume = 1;
  error = $state<string | null>(null);

  // listen-session bookkeeping
  private sessionId: string | null = null;
  private sessionEpisode: Episode | null = null;
  private sessionSeconds = 0;
  private tickAccum = 0;
  private lastTickTime = 0;
  private pendingSeek: number | null = null;

  current = $derived(this.queueIndex >= 0 ? this.queue[this.queueIndex] : null);

  currentTrackIndex = $derived.by(() => {
    if (!this.tracks.length) return -1;
    if (this.live) return this.liveTrackIndex;
    // Track timestamps are show-relative; compare against show time (audio − offset).
    const showTime = this.currentTime - this.offset;
    let idx = -1;
    for (let i = 0; i < this.tracks.length; i++) {
      const s = this.tracks[i].start_sec;
      if (s !== null && s <= showTime) idx = i;
    }
    return idx;
  });

  attach(el: HTMLAudioElement) {
    if (this.audio === el) return;
    this.audio = el;
    let saved: string | null = null;
    try {
      saved = localStorage.getItem("ab2.volume");
    } catch {
      /* storage can be unavailable in restricted webviews */
    }
    this.volume = normalizeVolume(saved);
    this.preMuteVolume = this.volume > 0 ? this.volume : 1;
    el.volume = this.volume;

    el.addEventListener("timeupdate", () => {
      this.currentTime = el.currentTime;
      this.accumulateListen();
    });
    el.addEventListener("durationchange", () => (this.duration = el.duration || 0));
    el.addEventListener("play", () => {
      this.playing = true;
      this.lastTickTime = performance.now();
    });
    el.addEventListener("pause", () => {
      this.playing = false;
      this.flushListen(false);
    });
    el.addEventListener("ended", () => {
      this.finishSession(true);
      this.nextEpisode();
    });
    el.addEventListener("loadedmetadata", () => {
      if (this.pendingSeek !== null) {
        el.currentTime = this.pendingSeek;
        this.pendingSeek = null;
      }
    });
    el.addEventListener("error", () => {
      if (el.src) {
        this.error = this.live
          ? "Live stream unavailable. Try again shortly."
          : "Audio failed to load. Archive may be unavailable.";
      }
      this.loading = false;
      this.playing = false;
    });
  }

  private accumulateListen() {
    if (!this.playing || !this.sessionId) return;
    const now = performance.now();
    if (this.lastTickTime === 0) {
      this.lastTickTime = now;
      return;
    }
    const delta = (now - this.lastTickTime) / 1000;
    this.lastTickTime = now;
    if (delta > 0 && delta < 5) {
      this.sessionSeconds += delta;
      this.tickAccum += delta;
    }
    if (this.tickAccum >= TICK_SECONDS) {
      this.tickAccum = 0;
      this.pushListen(false);
    }
  }

  private flushListen(completed: boolean) {
    if (this.sessionId && this.sessionSeconds > 1) this.pushListen(completed);
  }

  private finishSession(completed: boolean) {
    this.flushListen(completed);
    this.sessionId = null;
    this.sessionEpisode = null;
    this.sessionSeconds = 0;
    this.tickAccum = 0;
    this.lastTickTime = 0;
  }

  private startSession(episode: Episode) {
    if (this.sessionId) return;
    this.sessionId = crypto.randomUUID();
    this.sessionEpisode = episode;
    this.sessionSeconds = 0;
    this.tickAccum = 0;
    this.lastTickTime = this.playing ? performance.now() : 0;
  }

  private pushListen(completed: boolean) {
    const ep = this.sessionEpisode;
    if (!ep || !this.sessionId) return;
    const isLive = this.live !== null;
    const position = isLive ? 0 : Math.round(this.currentTime);
    const duration = isLive ? 0 : Math.round(this.duration);
    // Treat "almost at the end" as completed even if the audio element never fires "ended".
    const done = !isLive && (completed || (duration > 0 && position / duration >= 0.97));
    // Keep the in-memory episode in sync so the row's progress bar / completed badge
    // update immediately without a reload.
    if (!isLive) {
      ep.resume_sec = position;
      if (duration > 0) ep.duration_sec = duration;
      if (done) ep.completed = true;
    }
    api
      .recordListen(this.sessionId, ep.id, Math.round(this.sessionSeconds), done, position, duration)
      .catch(() => {});
  }

  // `startSec` is an audio-relative resume position; `startTrackSec` is a show-relative
  // playlist timestamp (a clicked song). At most one is non-null. When both are null the
  // player starts fresh at the jingle lead-in (see loadCurrent).
  async playQueue(
    items: QueueItem[],
    index = 0,
    startSec: number | null = null,
    startTrackSec: number | null = null,
  ) {
    if (!items.length) return;
    this.queue = items;
    this.queueIndex = index;
    await this.loadCurrent(startSec, startTrackSec);
  }

  async playEpisode(
    episode: Episode,
    showName: string,
    startSec: number | null = null,
    startTrackSec: number | null = null,
  ) {
    await this.playQueue([{ episode, showName }], 0, startSec, startTrackSec);
  }

  private async loadCurrent(
    startSec: number | null = null,
    startTrackSec: number | null = null,
  ) {
    const item = this.current;
    if (!item || !this.audio) return;
    const generation = this.transitions.start(`archive:${item.episode.id}`);
    this.error = null;
    this.loading = true;
    this.finishSession(false);
    this.audio.pause();
    this.playing = false;
    this.clearLiveStatusPolling();
    this.tracks = [];
    this.currentTime = 0;
    this.duration = 0;
    this.offset = 0;
    this.live = null;
    this.liveEpisode = null;
    this.liveSong = null;
    this.livePlaylistLoading = false;
    this.livePlaylistError = null;
    this.liveTrackIndex = -1;
    try {
      const src = await api.resolveAudio(item.episode.id);
      if (!this.transitions.isCurrent(generation)) return;
      const url = src.local ? convertFileSrc(src.url) : src.url;
      this.offset = src.offset_sec ?? 0;
      // Resolve the initial seek (all in audio time):
      //  • a clicked song → its show-relative timestamp + offset
      //  • a resume position → already audio-relative, use as-is
      //  • fresh play → the jingle lead-in, just before the show's playlist zero
      this.pendingSeek =
        startTrackSec !== null
          ? startTrackSec + this.offset
          : startSec !== null
            ? startSec
            : Math.max(0, this.offset - INTRO_LEAD_IN_SEC);
      this.audio.src = url;
      await this.audio.play();
      if (!this.transitions.isCurrent(generation)) return;
      this.startSession(item.episode);
      // Playlist loads lazily after playback starts (may hit network).
      api
        .getPlaylist(item.episode.id)
        .then((t) => {
          if (
            this.transitions.isCurrent(generation) &&
            this.current?.episode.id === item.episode.id
          )
            this.tracks = t;
        })
        .catch(() => {});
    } catch (e) {
      if (!this.transitions.isCurrent(generation)) return;
      this.error = String(e);
      this.playing = false;
    } finally {
      if (this.transitions.isCurrent(generation)) {
        this.loading = false;
        this.transitions.settle(generation);
      }
    }
  }

  // Play a WFMU live channel. The stream starts immediately; its current playlist is
  // loaded independently so a slow station page never delays live audio.
  async playLive(stream: LiveStream) {
    if (!this.audio) return;
    const request = this.transitions.requestLive(`live:${stream.id}`);
    if (request.action === "toggle") {
      this.toggle();
      return;
    }
    if (request.action === "coalesce") return;
    const generation = request.generation;
    this.finishSession(false);
    this.clearLiveStatusPolling();
    this.error = null;
    this.loading = true;
    this.queue = [];
    this.queueIndex = -1;
    this.tracks = [];
    this.offset = 0;
    this.currentTime = 0;
    this.duration = 0;
    this.pendingSeek = null;
    this.liveEpisode = null;
    this.liveSong = null;
    this.liveTrackIndex = -1;
    this.livePlaylistError = null;
    this.livePlaylistLoading = true;
    this.live = stream;
    try {
      this.audio.src = stream.url;
      void this.refreshLiveStatus(stream, generation, true);
      this.liveStatusTimer = setInterval(() => {
        void this.refreshLiveStatus(stream, generation, false);
      }, 5_000);
      await this.audio.play();
      if (!this.transitions.isCurrent(generation)) return;
    } catch (e) {
      if (!this.transitions.isCurrent(generation) && isAbortError(e)) return;
      if (!this.transitions.isCurrent(generation)) return;
      this.error = String(e);
      this.playing = false;
      this.livePlaylistLoading = false;
    } finally {
      if (this.transitions.isCurrent(generation)) {
        this.loading = false;
        this.transitions.settle(generation);
      }
    }
  }

  private clearLiveStatusPolling() {
    if (this.liveStatusTimer !== null) {
      clearInterval(this.liveStatusTimer);
      this.liveStatusTimer = null;
    }
    this.liveRefreshGeneration = null;
  }

  private async refreshLiveStatus(
    stream: LiveStream,
    generation: number,
    initial: boolean,
  ) {
    if (
      this.liveRefreshGeneration === generation ||
      !this.transitions.isCurrent(generation) ||
      this.live?.id !== stream.id
    )
      return;
    this.liveRefreshGeneration = generation;
    if (initial) this.livePlaylistLoading = true;
    try {
      const status = await api.getLiveStatus(
        stream.id,
        stream.status_source,
        stream.name,
      );
      if (!this.transitions.isCurrent(generation) || this.live?.id !== stream.id) return;

      const previousEpisodeId = this.liveEpisode?.episode.id ?? null;
      if (previousEpisodeId !== null && previousEpisodeId !== status.episode.id) {
        // Attribute accumulated seconds to the show that just ended before moving the
        // continuing stream to its newly discovered live episode.
        this.finishSession(false);
      }
      this.liveEpisode = { episode: status.episode, showName: status.show_name };
      this.liveSong = status.current_song;
      this.tracks = status.tracks;
      this.liveTrackIndex = this.findLiveTrack(status);
      this.livePlaylistError = null;
      this.startSession(status.episode);
      if (status.playlist_needs_load) {
        this.livePlaylistLoading = true;
        void this.loadHostedPlaylist(status.episode.id, generation, status.current_song);
      } else {
        this.livePlaylistLoading = false;
      }
    } catch (e) {
      if (
        this.transitions.isCurrent(generation) &&
        this.live?.id === stream.id &&
        (initial || !this.liveEpisode)
      ) {
        this.livePlaylistError = String(e);
      }
    } finally {
      if (this.liveRefreshGeneration === generation) this.liveRefreshGeneration = null;
    }
  }

  private findLiveTrack(status: LiveStatus): number {
    if (status.current_track_id !== null) {
      const byId = status.tracks.findIndex((track) => track.id === status.current_track_id);
      if (byId >= 0) return byId;
    }
    return this.findSongIndex(status.tracks, status.current_song);
  }

  private findSongIndex(tracks: Track[], song: LiveSong | null): number {
    if (song) {
      for (let index = tracks.length - 1; index >= 0; index -= 1) {
        if (tracks[index].artist === song.artist && tracks[index].title === song.title)
          return index;
      }
    }
    return tracks.length - 1;
  }

  private async loadHostedPlaylist(
    episodeId: number,
    generation: number,
    currentSong: LiveSong | null,
  ) {
    let request = this.livePlaylistLoads.get(episodeId);
    if (!request) {
      // get_live_status only requests this load for a missing or stale hosted
      // playlist, so bypass the archive cache and fetch the latest appended rows.
      request = api.getPlaylist(episodeId, true);
      this.livePlaylistLoads.set(episodeId, request);
    }
    try {
      const tracks = await request;
      if (
        !this.transitions.isCurrent(generation) ||
        this.liveEpisode?.episode.id !== episodeId
      )
        return;
      this.tracks = tracks;
      this.liveTrackIndex = this.findSongIndex(tracks, currentSong);
      this.livePlaylistError = null;
    } catch (error) {
      if (
        this.transitions.isCurrent(generation) &&
        this.liveEpisode?.episode.id === episodeId
      )
        this.livePlaylistError = String(error);
    } finally {
      if (this.livePlaylistLoads.get(episodeId) === request)
        this.livePlaylistLoads.delete(episodeId);
      if (
        this.transitions.isCurrent(generation) &&
        this.liveEpisode?.episode.id === episodeId
      )
        this.livePlaylistLoading = false;
    }
  }

  toggle() {
    if (!this.audio || !this.audio.src) return;
    if (this.audio.paused) this.resumeAudio();
    else this.audio.pause();
  }

  private resumeAudio() {
    if (!this.audio) return;
    void this.audio.play().catch((error) => {
      this.error = String(error);
      this.playing = false;
    });
  }

  seek(sec: number) {
    if (!this.audio) return;
    this.audio.currentTime = Math.max(0, Math.min(sec, this.duration || sec));
  }

  seekToTrack(track: Track) {
    if (track.start_sec === null) return;
    this.seek(track.start_sec + this.offset);
    if (this.audio?.paused) this.resumeAudio();
  }

  // Jump the playhead by a relative amount (e.g. −15 / +15 seconds).
  skip(delta: number) {
    if (!this.audio) return;
    this.seek(this.currentTime + delta);
  }

  // Skip to the start of the next timestamped track in the current episode.
  // Track timestamps are show-relative, so add the archive offset when seeking.
  nextTrack() {
    if (!this.tracks.length) return;
    for (let i = this.currentTrackIndex + 1; i < this.tracks.length; i++) {
      const s = this.tracks[i].start_sec;
      if (s !== null) {
        this.seek(s + this.offset);
        if (this.audio?.paused) this.resumeAudio();
        return;
      }
    }
  }

  // Skip to the previous track start. If we're already a few seconds into the
  // current track, restart it instead (mirrors the prev-episode "restart" feel).
  prevTrack() {
    if (!this.tracks.length) return;
    const idx = this.currentTrackIndex;
    const curStart = idx >= 0 ? this.tracks[idx].start_sec : null;
    const showTime = this.currentTime - this.offset;
    if (curStart !== null && showTime - curStart > 3) {
      this.seek(curStart + this.offset);
      return;
    }
    for (let i = idx - 1; i >= 0; i--) {
      const s = this.tracks[i].start_sec;
      if (s !== null) {
        this.seek(s + this.offset);
        if (this.audio?.paused) this.resumeAudio();
        return;
      }
    }
    this.seek((curStart ?? 0) + this.offset);
  }

  setVolume(v: number) {
    this.volume = normalizeVolume(v, this.volume);
    this.muted = false;
    if (this.volume > 0) this.preMuteVolume = this.volume;
    if (this.audio) this.audio.volume = this.volume;
    try {
      localStorage.setItem("ab2.volume", String(this.volume));
    } catch {
      /* volume still applies for this session */
    }
  }

  toggleMute() {
    this.muted = !this.muted;
    if (this.muted) {
      if (this.volume > 0) this.preMuteVolume = this.volume;
      if (this.audio) this.audio.volume = 0;
    } else {
      this.volume = normalizeVolume(this.preMuteVolume);
      if (this.audio) this.audio.volume = this.volume;
    }
  }

  setEpisodeFavourite(fav: boolean) {
    const item = this.current;
    if (item) item.episode.favourite = fav;
  }

  setLiveEpisodeFavourite(fav: boolean) {
    if (this.liveEpisode) this.liveEpisode.episode.favourite = fav;
  }

  setTrackFavourite(trackId: number, fav: boolean) {
    const t = this.tracks.find((x) => x.id === trackId);
    if (t) t.favourite = fav;
  }

  async nextEpisode() {
    if (this.queueIndex < this.queue.length - 1) {
      this.queueIndex += 1;
      await this.loadCurrent();
    } else {
      this.playing = false;
    }
  }

  async prevEpisode() {
    if (this.currentTime > 10) {
      this.seek(0);
      return;
    }
    if (this.queueIndex > 0) {
      this.queueIndex -= 1;
      await this.loadCurrent();
    }
  }

  stop() {
    this.transitions.reset();
    this.finishSession(false);
    this.clearLiveStatusPolling();
    if (this.audio) {
      this.audio.pause();
      this.audio.removeAttribute("src");
      this.audio.load();
    }
    this.queue = [];
    this.queueIndex = -1;
    this.live = null;
    this.liveEpisode = null;
    this.liveSong = null;
    this.livePlaylistLoading = false;
    this.livePlaylistError = null;
    this.liveTrackIndex = -1;
    this.tracks = [];
    this.playing = false;
    this.currentTime = 0;
    this.duration = 0;
  }
}

export const player = new Player();
