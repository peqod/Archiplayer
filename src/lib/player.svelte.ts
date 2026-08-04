import { convertFileSrc } from "@tauri-apps/api/core";
import {
  api,
  type Episode,
  type LiveSong,
  type LiveStatus,
  type LiveStream,
  type PlaylistPayload,
  type Track,
} from "./api";
import {
  effectiveArchiveDuration,
  scheduledArchiveEndReached,
} from "./archive-timing";
import {
  AUDITION_FADE_SEC,
  auditionFadeInGain,
  auditionFadeOutGain,
  transitionFadeOutGain,
} from "./audition-fade";
import { isAbortError, PlaybackTransitions } from "./playback-transition";
import {
  nextEpisodeIndex,
  nextQueueIndex,
  prevEpisodeIndex,
  prevQueueIndex,
  shuffledFrom,
  type QueueItem,
} from "./queue-build";
import { shouldRetryStaleSource, staleResumeAt } from "./stale-source";
import { segmentEndSec, trackMarkSeconds } from "./track-playback";
import { normalizeVolume } from "./volume";

export type { QueueItem, QueueSong } from "./queue-build";

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
  private offset = $state(0);

  queue = $state<QueueItem[]>([]);
  queueIndex = $state(-1);
  shuffle = $state(false);
  repeat = $state(false);
  // The queue in the order it was handed over, kept so turning shuffle back off restores
  // the list the listener actually chose rather than leaving them with a permutation.
  private queueSource: QueueItem[] = [];
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
  private livePlaylistLoads = new Map<number, Promise<PlaylistPayload>>();
  private transitions = new PlaybackTransitions();
  tracks = $state<Track[]>([]);
  playing = $state(false);
  loading = $state(false);
  currentTime = $state(0);
  private mediaDuration = $state(0);
  private broadcastDuration = $state<number | null>(null);
  duration = $derived(
    this.live
      ? this.mediaDuration
      : effectiveArchiveDuration(
          this.mediaDuration,
          this.offset,
          this.broadcastDuration,
        ),
  );
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
  private completionHandled = false;

  // Bookkeeping for the stale-source self-heal (see `beginStaleSourceRetry`).
  private sourceEpisodeId: number | null = null;
  private sourceLocal = false;
  private sourceGeneration = 0;
  private staleRetryEpisodeId: number | null = null;
  private staleRetryActive = false;
  private fadeGain = 1;
  private fadeFrame: number | null = null;
  private fadeMode: "in" | "out" | "live-out" | null = null;
  private fadeInPending = false;
  private liveFadePromise: Promise<boolean> | null = null;
  private liveFadeResolve: ((completed: boolean) => void) | null = null;
  private liveFadeTimer: ReturnType<typeof setTimeout> | null = null;

  current = $derived(this.queueIndex >= 0 ? this.queue[this.queueIndex] : null);

  /** True while the queue is a list of songs rather than a list of whole episodes. */
  songMode = $derived(!!this.current?.song);

  /**
   * Audio-time position where the current entry has to stop, or null when the episode's
   * own end is the boundary. Derived from the loaded playlist, so it is null during a
   * transition (`loadCurrent` empties `tracks`) and the check below stays inert until
   * there is something to compare against.
   */
  private segmentEnd = $derived.by(() => {
    const song = this.current?.song;
    if (this.live || !song) return null;
    const end = segmentEndSec(this.tracks, song.startSec);
    return end === null ? null : end + this.offset;
  });

  /** Where the closing fade belongs: the song's end in song mode, the episode's otherwise. */
  private fadeEnd = $derived(this.segmentEnd ?? this.duration);

  private episodeIds = $derived(this.queue.map((item) => item.episode.id));

  canPrevTrack = $derived(
    this.songMode ? this.queue.length > 0 : this.tracks.length > 0,
  );
  canNextTrack = $derived(
    this.songMode
      ? nextQueueIndex(this.queueIndex, this.queue.length, this.repeat) >= 0
      : this.tracks.length > 0,
  );
  canPrevEpisode = $derived(!!this.current);
  canNextEpisode = $derived(
    this.songMode
      ? nextEpisodeIndex(this.episodeIds, this.queueIndex, this.repeat) >= 0
      : nextQueueIndex(this.queueIndex, this.queue.length, this.repeat) >= 0,
  );

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

  // Audio-time positions of the playlist entries the scrubber marks. Live has no
  // seekable timeline, so it never marks.
  trackMarks = $derived(
    this.live ? [] : trackMarkSeconds(this.tracks, this.offset, this.duration),
  );

  attach(el: HTMLAudioElement) {
    if (this.audio === el) return;
    this.audio = el;
    let saved: string | null = null;
    try {
      saved = localStorage.getItem("ab2.volume");
      this.shuffle = localStorage.getItem("ab2.shuffle") === "1";
      this.repeat = localStorage.getItem("ab2.repeat") === "1";
    } catch {
      /* storage can be unavailable in restricted webviews */
    }
    this.volume = normalizeVolume(saved);
    this.preMuteVolume = this.volume > 0 ? this.volume : 1;
    this.applyAudioVolume();

    el.addEventListener("timeupdate", () => {
      this.currentTime = el.currentTime;
      this.accumulateListen();
      this.updateEndFade();
      this.advanceAtSegmentEndIfNeeded();
      this.finishAtScheduledEndIfNeeded();
    });
    el.addEventListener("durationchange", () => {
      this.mediaDuration = el.duration || 0;
      this.updateEndFade();
      this.advanceAtSegmentEndIfNeeded();
      this.finishAtScheduledEndIfNeeded();
    });
    el.addEventListener("play", () => {
      this.playing = true;
      this.lastTickTime = performance.now();
      if (this.fadeInPending) {
        this.fadeInPending = false;
        this.startFadeIn();
      } else {
        this.updateEndFade();
      }
    });
    el.addEventListener("pause", () => {
      this.playing = false;
      if (this.fadeMode === "in") {
        this.stopFadeAnimation();
        this.setFadeGain(1);
      } else {
        this.stopFadeAnimation();
      }
      this.flushListen(false);
    });
    el.addEventListener("ended", () => {
      if (this.completionHandled) return;
      this.completionHandled = true;
      this.finishSession(true);
      void this.advanceOne();
    });
    el.addEventListener("loadedmetadata", () => {
      if (this.pendingSeek !== null) {
        el.currentTime = this.pendingSeek;
        this.pendingSeek = null;
      }
    });
    el.addEventListener("error", () => {
      if (el.src) {
        if (this.beginStaleSourceRetry(el)) return;
        this.error = this.live
          ? "Live stream unavailable. Try again shortly."
          : "Audio failed to load. Archive may be unavailable.";
      }
      this.loading = false;
      this.playing = false;
    });
  }

  /**
   * One forced re-resolve per episode when the media element rejects an archive source.
   * WFMU rolls its 128k mp3s off mp3archives.wfmu.org after a few weeks and its own player
   * falls back to the permanent S3 mp4, which leaves our cached URL answering 404. Re-scraping
   * the AccuPlayer page picks up the new backend and playback resumes where it stopped.
   *
   * Returns true when a retry took over the failure. Live streams, local downloads and a
   * second failure on the same episode fall through to the error message instead.
   */
  private beginStaleSourceRetry(el: HTMLAudioElement): boolean {
    const episodeId = this.sourceEpisodeId;
    const retry = shouldRetryStaleSource({
      live: this.live !== null,
      local: this.sourceLocal,
      sourceEpisodeId: episodeId,
      retriedEpisodeId: this.staleRetryEpisodeId,
      currentEpisodeId: this.current?.episode.id ?? null,
      sourceIsCurrent: this.transitions.isCurrent(this.sourceGeneration),
    });
    if (!retry || episodeId === null) return false;
    this.staleRetryEpisodeId = episodeId;
    this.staleRetryActive = true;
    void this.retryStaleSource(el, episodeId, this.sourceGeneration);
    return true;
  }

  private async retryStaleSource(
    el: HTMLAudioElement,
    episodeId: number,
    generation: number,
  ) {
    const item = this.current;
    const resumeAt = staleResumeAt(
      el.currentTime,
      this.pendingSeek,
      this.offset,
      INTRO_LEAD_IN_SEC,
    );
    this.loading = true;
    this.playing = false;
    let reloaded = false;
    try {
      const src = await api.resolveAudio(episodeId, true);
      if (!this.transitions.isCurrent(generation)) return;
      this.error = null;
      this.offset = src.offset_sec ?? 0;
      this.sourceLocal = src.local;
      this.pendingSeek = resumeAt;
      el.src = src.local ? convertFileSrc(src.url) : src.url;
      reloaded = true;
      await el.play();
      if (!this.transitions.isCurrent(generation) || !item) return;
      this.afterPlaybackStarted(item, generation);
    } catch (e) {
      if (!this.transitions.isCurrent(generation)) return;
      // The replacement source failing is already reported by the error listener, which by
      // then has spent this episode's retry. Anything else is ours to surface.
      if (!reloaded || !el.error) this.error = String(e);
      this.playing = false;
    } finally {
      this.staleRetryActive = false;
      if (this.transitions.isCurrent(generation)) this.loading = false;
    }
  }

  /** Session bookkeeping and lazy playlist load, once audio is actually rolling. */
  private afterPlaybackStarted(item: QueueItem, generation: number) {
    this.startSession(item.episode);
    // Playlist loads lazily after playback starts (may hit network).
    api
      .getPlaylist(item.episode.id)
      .then((playlist) => {
        if (
          this.transitions.isCurrent(generation) &&
          this.current?.episode.id === item.episode.id
        ) {
          this.tracks = playlist.tracks;
          this.broadcastDuration = playlist.broadcast_duration_sec;
          item.episode.broadcast_duration_sec = playlist.broadcast_duration_sec;
          this.updateEndFade();
          this.finishAtScheduledEndIfNeeded();
        }
      })
      .catch(() => {});
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
    this.queueSource = items;
    if (this.shuffle && items.length > 1) {
      // Pin what was clicked at the front, permute the rest: shuffle changes what comes
      // next, never what the listener just asked for.
      const pinned = [items[index], ...items.filter((_, i) => i !== index)];
      this.queue = shuffledFrom(pinned, 0);
      this.queueIndex = 0;
    } else {
      this.queue = items;
      this.queueIndex = index;
    }
    await this.loadCurrent(startSec, startTrackSec);
  }

  setShuffle(on: boolean) {
    this.shuffle = on;
    if (on) this.queue = shuffledFrom(this.queue, this.queueIndex);
    else this.restoreQueueOrder();
    this.persistQueueModes();
  }

  setRepeat(on: boolean) {
    this.repeat = on;
    this.persistQueueModes();
  }

  /** Back to the order the queue was built in, with the playhead still on its entry. */
  private restoreQueueOrder() {
    const item = this.current;
    if (!this.queueSource.length || !item) return;
    const at = this.queueSource.findIndex(
      (q) => q.episode.id === item.episode.id && (q.song?.id ?? null) === (item.song?.id ?? null),
    );
    if (at < 0) return;
    this.queue = [...this.queueSource];
    this.queueIndex = at;
  }

  private persistQueueModes() {
    try {
      localStorage.setItem("ab2.shuffle", this.shuffle ? "1" : "0");
      localStorage.setItem("ab2.repeat", this.repeat ? "1" : "0");
    } catch {
      /* the modes still apply for this session */
    }
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
    this.clearLiveStatusPolling();
    await this.fadeOutLive();
    if (!this.transitions.isCurrent(generation)) return;
    this.finishSession(false);
    this.audio.pause();
    this.playing = false;
    this.tracks = [];
    this.currentTime = 0;
    this.mediaDuration = 0;
    this.broadcastDuration = item.episode.broadcast_duration_sec;
    this.completionHandled = false;
    this.offset = 0;
    this.live = null;
    this.liveEpisode = null;
    this.liveSong = null;
    this.livePlaylistLoading = false;
    this.livePlaylistError = null;
    this.liveTrackIndex = -1;
    this.sourceEpisodeId = null;
    this.staleRetryEpisodeId = null;
    this.staleRetryActive = false;
    this.prepareFadeIn();
    let installed = false;
    try {
      const src = await api.resolveAudio(item.episode.id);
      if (!this.transitions.isCurrent(generation)) return;
      const url = src.local ? convertFileSrc(src.url) : src.url;
      this.offset = src.offset_sec ?? 0;
      // Resolve the initial seek (all in audio time):
      //  • a clicked song, or a song-granular queue entry → its show-relative timestamp + offset
      //  • a resume position → already audio-relative, use as-is
      //  • fresh play → the jingle lead-in, just before the show's playlist zero
      const trackSec = startTrackSec ?? item.song?.startSec ?? null;
      this.pendingSeek =
        trackSec !== null
          ? trackSec + this.offset
          : startSec !== null
            ? startSec
            : Math.max(0, this.offset - INTRO_LEAD_IN_SEC);
      this.sourceEpisodeId = item.episode.id;
      this.sourceLocal = src.local;
      this.sourceGeneration = generation;
      this.audio.src = url;
      installed = true;
      await this.audio.play();
      if (!this.transitions.isCurrent(generation)) return;
      this.afterPlaybackStarted(item, generation);
    } catch (e) {
      if (!this.transitions.isCurrent(generation)) return;
      // A failure of the source we just installed belongs to the `error` listener, which may be
      // re-resolving a stale URL right now. Don't clobber its message or its retry with the
      // play() rejection. Failures before that (a dead resolve) are still ours to report.
      if (installed && this.audio?.error) return;
      this.error = String(e);
      this.playing = false;
    } finally {
      if (this.transitions.isCurrent(generation)) {
        if (!this.staleRetryActive) this.loading = false;
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
    this.error = null;
    this.loading = true;
    await this.fadeOutLive();
    if (!this.transitions.isCurrent(generation)) return;
    this.finishSession(false);
    this.clearLiveStatusPolling();
    this.audio.pause();
    this.queue = [];
    this.queueIndex = -1;
    this.queueSource = [];
    this.tracks = [];
    this.offset = 0;
    this.currentTime = 0;
    this.mediaDuration = 0;
    this.broadcastDuration = null;
    this.completionHandled = false;
    this.pendingSeek = null;
    this.liveEpisode = null;
    this.liveSong = null;
    this.liveTrackIndex = -1;
    this.livePlaylistError = null;
    this.livePlaylistLoading = true;
    this.live = stream;
    this.sourceEpisodeId = null;
    this.staleRetryActive = false;
    this.prepareFadeIn();
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
      this.resetFade();
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
      const playlist = await request;
      if (
        !this.transitions.isCurrent(generation) ||
        this.liveEpisode?.episode.id !== episodeId
      )
        return;
      this.tracks = playlist.tracks;
      this.liveTrackIndex = this.findSongIndex(playlist.tracks, currentSong);
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
    if (this.audio.paused) {
      if (this.live) this.prepareFadeIn();
      this.resumeAudio();
    } else {
      // Clicks, keyboard shortcuts and media controls pause immediately. Live fade-out
      // is reserved for a source change, where there is new audio to hand off to.
      this.audio.pause();
    }
  }

  private resumeAudio() {
    if (!this.audio) return;
    if (!this.live && this.completionHandled && this.current) {
      this.completionHandled = false;
      // Replay what actually finished: the song for a song entry, the broadcast otherwise.
      this.seek(this.entryStart(this.current));
      this.startSession(this.current.episode);
      this.prepareFadeIn();
    }
    void this.audio.play().catch((error) => {
      this.error = String(error);
      this.playing = false;
      if (this.live) this.resetFade();
    });
  }

  seek(sec: number) {
    if (!this.audio) return;
    const at = Math.max(0, Math.min(sec, this.duration || sec));
    this.audio.currentTime = at;
    // Paint the new position at once. `timeupdate` only fires a few times a second, so
    // waiting for the element would stutter the scrubber under the pointer and snap it
    // back the moment a drag lets go.
    this.currentTime = at;
    this.updateEndFade();
  }

  private applyAudioVolume() {
    if (!this.audio) return;
    this.audio.volume = this.muted ? 0 : this.volume * this.fadeGain;
  }

  private setFadeGain(gain: number) {
    this.fadeGain = Math.max(0, Math.min(1, gain));
    this.applyAudioVolume();
  }

  private stopFadeAnimation() {
    if (this.fadeFrame !== null) cancelAnimationFrame(this.fadeFrame);
    if (this.liveFadeTimer !== null) clearTimeout(this.liveFadeTimer);
    this.fadeFrame = null;
    this.fadeMode = null;
    this.liveFadeTimer = null;
    this.liveFadePromise = null;
    const resolve = this.liveFadeResolve;
    this.liveFadeResolve = null;
    resolve?.(false);
  }

  private finishLiveFade(completed: boolean) {
    if (this.fadeFrame !== null) cancelAnimationFrame(this.fadeFrame);
    if (this.liveFadeTimer !== null) clearTimeout(this.liveFadeTimer);
    this.fadeFrame = null;
    this.fadeMode = null;
    this.liveFadeTimer = null;
    this.liveFadePromise = null;
    const resolve = this.liveFadeResolve;
    this.liveFadeResolve = null;
    resolve?.(completed);
  }

  private resetFade() {
    this.stopFadeAnimation();
    this.fadeInPending = false;
    this.setFadeGain(1);
  }

  private prepareFadeIn() {
    this.stopFadeAnimation();
    this.fadeInPending = true;
    this.setFadeGain(0);
  }

  private startFadeIn() {
    this.stopFadeAnimation();
    this.fadeMode = "in";
    const startedAt = performance.now();
    const frame = (now: number) => {
      if (this.fadeMode !== "in") return;
      const gain = auditionFadeInGain((now - startedAt) / 1000);
      this.setFadeGain(gain);
      if (gain < 1 && this.audio && !this.audio.paused) {
        this.fadeFrame = requestAnimationFrame(frame);
      } else {
        this.fadeFrame = null;
        this.fadeMode = null;
      }
    };
    this.fadeFrame = requestAnimationFrame(frame);
  }

  private startFadeOut() {
    this.stopFadeAnimation();
    this.fadeMode = "out";
    const frame = () => {
      if (this.fadeMode !== "out" || !this.audio || this.audio.paused || this.live) return;
      const gain = auditionFadeOutGain(this.audio.currentTime, this.fadeEnd);
      this.setFadeGain(Math.min(this.fadeGain, gain));
      if (gain > 0) {
        this.fadeFrame = requestAnimationFrame(frame);
      } else {
        this.fadeFrame = null;
        this.fadeMode = null;
      }
    };
    frame();
  }

  private fadeOutLive(): Promise<boolean> {
    if (!this.live || !this.audio || this.audio.paused) {
      return Promise.resolve(true);
    }
    if (this.fadeMode === "live-out" && this.liveFadePromise) {
      return this.liveFadePromise;
    }

    this.stopFadeAnimation();
    const startedAt = performance.now();
    const startingGain = this.fadeGain;
    this.fadeMode = "live-out";
    this.liveFadePromise = new Promise<boolean>((resolve) => {
      this.liveFadeResolve = resolve;
      const frame = (now: number) => {
        if (this.fadeMode !== "live-out") return;
        if (!this.audio || this.audio.paused || !this.live) {
          this.finishLiveFade(true);
          return;
        }
        const gain = transitionFadeOutGain(
          (now - startedAt) / 1000,
          startingGain,
        );
        this.setFadeGain(gain);
        if (gain > 0) {
          this.fadeFrame = requestAnimationFrame(frame);
        } else {
          this.finishLiveFade(true);
        }
      };
      this.fadeFrame = requestAnimationFrame(frame);
      // requestAnimationFrame can pause in a minimized webview. The timeout keeps
      // media-key pauses and source switches bounded when the window is hidden.
      this.liveFadeTimer = setTimeout(() => {
        if (this.fadeMode !== "live-out") return;
        this.setFadeGain(0);
        this.finishLiveFade(true);
      }, AUDITION_FADE_SEC * 1000);
    });
    return this.liveFadePromise;
  }

  private updateEndFade() {
    if (this.live || !this.audio) return;
    const gain = auditionFadeOutGain(this.currentTime, this.fadeEnd);
    if (gain < 1) {
      if (this.fadeMode !== "out" && !this.audio.paused) this.startFadeOut();
    } else if (
      this.fadeMode === "out" ||
      (this.fadeGain < 1 && this.fadeMode !== "in" && !this.fadeInPending)
    ) {
      this.stopFadeAnimation();
      this.setFadeGain(1);
    }
  }

  private finishAtScheduledEndIfNeeded() {
    if (
      this.live ||
      this.completionHandled ||
      !scheduledArchiveEndReached(
        this.currentTime,
        this.mediaDuration,
        this.duration,
      )
    )
      return;
    this.completionHandled = true;
    this.currentTime = this.duration;
    this.finishSession(true);
    this.audio?.pause();
    void this.advanceOne();
  }

  /**
   * Stop a song entry at its own end and hand over to the next queue entry. Deliberately
   * not `finishSession(true)`: a song that ends 20% into a broadcast is not a completed
   * episode, and marking it so would write a bogus `resume_sec` and completion badge.
   *
   * `completionHandled` is the same one-shot latch the two episode-end paths use, so
   * exactly one of the three fires per crossing.
   */
  private advanceAtSegmentEndIfNeeded() {
    if (this.live || this.completionHandled) return;
    const end = this.segmentEnd;
    if (end === null || this.currentTime < end) return;
    this.completionHandled = true;
    void this.advanceOne();
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
    // In a song queue the next song is the next queue entry, not the next timecode in
    // this episode: the queue is the list the listener chose.
    if (this.songMode) {
      void this.advanceOne("stay");
      return;
    }
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

  // Song queue: step back one entry. Episode: skip to the previous track start, or if
  // we're already a few seconds into the current track, restart it instead.
  prevTrack() {
    if (this.songMode) {
      const item = this.current;
      if (!item) return;
      // A playlist steps a whole entry, symmetrically with next. No "restart first" rule
      // here: next advances on the first press, so back has to retreat on the first press
      // too. Restarting the song stays available on its row and on the scrubber.
      const prev = prevQueueIndex(this.queueIndex, this.queue.length, this.repeat);
      // Nothing before this entry, so the only thing left to go back to is its own start.
      if (prev >= 0) void this.advanceTo(prev);
      else this.seek(this.entryStart(item));
      return;
    }
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
    this.applyAudioVolume();
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
    } else {
      this.volume = normalizeVolume(this.preMuteVolume);
    }
    this.applyAudioVolume();
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

  /** Audio position an entry starts at: its song, or the episode's jingle lead-in. */
  private entryStart(item: QueueItem): number {
    return item.song
      ? item.song.startSec + this.offset
      : Math.max(0, this.offset - INTRO_LEAD_IN_SEC);
  }

  /**
   * One step through the queue. This, not `nextEpisode`, is what the end of a song and
   * the end of an episode hand over to: in a song queue sorted by artist the next entry
   * can be an *earlier* song of the episode that just finished, and that has to play.
   */
  private async advanceOne(atEnd: "halt" | "stay" = "halt") {
    const next = nextQueueIndex(this.queueIndex, this.queue.length, this.repeat);
    if (next < 0) {
      // Reaching the end by playing stops; being asked for a next that does not exist
      // leaves what is playing alone.
      if (atEnd === "halt") this.haltAtQueueEnd();
      return;
    }
    // A shuffled queue that wraps plays a different order the second time round.
    if (next === 0 && this.shuffle && this.queue.length > 1) {
      this.queue = shuffledFrom(this.queue, -1);
    }
    await this.advanceTo(next);
  }

  private haltAtQueueEnd() {
    this.playing = false;
    // A song entry ends mid-file, where nothing else stops the element.
    this.audio?.pause();
  }

  /**
   * Move to `index`. Two consecutive entries in the same episode are two positions in one
   * audio file, so that case seeks instead of re-resolving the source: no network, no
   * reload gap, and the listen session keeps running because it belongs to the episode.
   */
  private async advanceTo(index: number) {
    if (index < 0 || index >= this.queue.length) return;
    const to = this.queue[index];
    const sameFile =
      !this.live &&
      !!this.audio?.src &&
      this.sourceEpisodeId === to.episode.id &&
      this.transitions.isCurrent(this.sourceGeneration) &&
      !this.staleRetryActive;
    this.queueIndex = index;
    if (!sameFile) {
      await this.loadCurrent();
      return;
    }
    // Clear the latch before seeking: the new entry brings a new boundary, and the check
    // runs on the very next `timeupdate`.
    this.completionHandled = false;
    // A mid-episode boundary leaves the session running and this is a no-op; arriving
    // from the episode's own end means `finishSession` already closed it.
    this.startSession(to.episode);
    this.seek(this.entryStart(to));
    if (this.audio?.paused) {
      this.prepareFadeIn();
      this.resumeAudio();
    } else {
      // Seeking fires no `play` event, so the opening ramp has to be started by hand.
      this.stopFadeAnimation();
      this.setFadeGain(0);
      this.startFadeIn();
    }
  }

  async nextEpisode() {
    const next = this.songMode
      ? nextEpisodeIndex(this.episodeIds, this.queueIndex, this.repeat)
      : nextQueueIndex(this.queueIndex, this.queue.length, this.repeat);
    // Nothing to step to: leave what is playing alone. The button is disabled here, so
    // this is the media-key and shortcut path.
    if (next < 0) return;
    await this.advanceTo(next);
  }

  async prevEpisode() {
    const start = this.current ? this.entryStart(this.current) : 0;
    if (this.songMode) {
      // Same rule as prev-song: in a playlist, back means back one, on the first press.
      const prev = prevEpisodeIndex(this.episodeIds, this.queueIndex, this.repeat);
      if (prev >= 0) await this.advanceTo(prev);
      else this.seek(start);
      return;
    }
    // Episode mode keeps the long-standing "restart the broadcast you are deep into"
    // behaviour: an episode is an hour or more, so a mis-press is expensive.
    if (this.currentTime - start > 10) {
      this.seek(start);
      return;
    }
    const prev = prevQueueIndex(this.queueIndex, this.queue.length, this.repeat);
    if (prev >= 0) await this.advanceTo(prev);
    else this.seek(start);
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
    this.queueSource = [];
    this.live = null;
    this.liveEpisode = null;
    this.liveSong = null;
    this.livePlaylistLoading = false;
    this.livePlaylistError = null;
    this.liveTrackIndex = -1;
    this.tracks = [];
    this.playing = false;
    this.currentTime = 0;
    this.mediaDuration = 0;
    this.broadcastDuration = null;
    this.completionHandled = false;
    this.resetFade();
  }
}

export const player = new Player();
