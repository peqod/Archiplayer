import { convertFileSrc } from "@tauri-apps/api/core";
import { api, type Episode, type Track } from "./api";

export interface QueueItem {
  episode: Episode;
  showName: string;
}

const TICK_SECONDS = 15;

class Player {
  audio: HTMLAudioElement | null = null;

  queue = $state<QueueItem[]>([]);
  queueIndex = $state(-1);
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
  private sessionSeconds = 0;
  private tickAccum = 0;
  private lastTickTime = 0;
  private pendingSeek: number | null = null;

  current = $derived(this.queueIndex >= 0 ? this.queue[this.queueIndex] : null);

  currentTrackIndex = $derived.by(() => {
    if (!this.tracks.length) return -1;
    let idx = -1;
    for (let i = 0; i < this.tracks.length; i++) {
      const s = this.tracks[i].start_sec;
      if (s !== null && s <= this.currentTime) idx = i;
    }
    return idx;
  });

  attach(el: HTMLAudioElement) {
    this.audio = el;
    const saved = localStorage.getItem("ab2.volume");
    this.volume = saved ? Number(saved) : 1;
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
      this.flushListen(true);
      this.next();
    });
    el.addEventListener("loadedmetadata", () => {
      if (this.pendingSeek !== null) {
        el.currentTime = this.pendingSeek;
        this.pendingSeek = null;
      }
    });
    el.addEventListener("error", () => {
      if (el.src) this.error = "Audio failed to load. Archive may be unavailable.";
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

  private pushListen(completed: boolean) {
    const ep = this.current?.episode;
    if (!ep || !this.sessionId) return;
    const position = Math.round(this.currentTime);
    const duration = Math.round(this.duration);
    // Treat "almost at the end" as completed even if the audio element never fires "ended".
    const done = completed || (duration > 0 && position / duration >= 0.97);
    // Keep the in-memory episode in sync so the row's progress bar / completed badge
    // update immediately without a reload.
    ep.resume_sec = position;
    if (duration > 0) ep.duration_sec = duration;
    if (done) ep.completed = true;
    api
      .recordListen(this.sessionId, ep.id, Math.round(this.sessionSeconds), done, position, duration)
      .catch(() => {});
  }

  async playQueue(items: QueueItem[], index = 0, startSec: number | null = null) {
    if (!items.length) return;
    this.queue = items;
    this.queueIndex = index;
    await this.loadCurrent(startSec);
  }

  async playEpisode(episode: Episode, showName: string, startSec: number | null = null) {
    await this.playQueue([{ episode, showName }], 0, startSec);
  }

  private async loadCurrent(startSec: number | null = null) {
    const item = this.current;
    if (!item || !this.audio) return;
    this.error = null;
    this.loading = true;
    this.flushListen(false);
    this.sessionId = crypto.randomUUID();
    this.sessionSeconds = 0;
    this.tickAccum = 0;
    this.lastTickTime = 0;
    this.tracks = [];
    this.currentTime = 0;
    this.duration = 0;
    try {
      const src = await api.resolveAudio(item.episode.id);
      const url = src.local ? convertFileSrc(src.url) : src.url;
      this.pendingSeek = startSec;
      this.audio.src = url;
      await this.audio.play();
      // Playlist loads lazily after playback starts (may hit network).
      api
        .getPlaylist(item.episode.id)
        .then((t) => {
          if (this.current?.episode.id === item.episode.id) this.tracks = t;
        })
        .catch(() => {});
    } catch (e) {
      this.error = String(e);
      this.playing = false;
    } finally {
      this.loading = false;
    }
  }

  toggle() {
    if (!this.audio || !this.audio.src) return;
    if (this.audio.paused) this.audio.play();
    else this.audio.pause();
  }

  seek(sec: number) {
    if (!this.audio) return;
    this.audio.currentTime = Math.max(0, Math.min(sec, this.duration || sec));
  }

  seekToTrack(track: Track) {
    if (track.start_sec === null) return;
    this.seek(track.start_sec);
    if (this.audio?.paused) this.audio.play();
  }

  setVolume(v: number) {
    this.volume = Math.max(0, Math.min(1, v));
    this.muted = false;
    if (this.audio) this.audio.volume = this.volume;
    localStorage.setItem("ab2.volume", String(this.volume));
  }

  toggleMute() {
    this.muted = !this.muted;
    if (this.muted) {
      this.preMuteVolume = this.volume;
      if (this.audio) this.audio.volume = 0;
    } else {
      this.volume = this.preMuteVolume;
      if (this.audio) this.audio.volume = this.preMuteVolume;
    }
  }

  setEpisodeFavourite(fav: boolean) {
    const item = this.current;
    if (item) item.episode.favourite = fav;
  }

  setTrackFavourite(trackId: number, fav: boolean) {
    const t = this.tracks.find((x) => x.id === trackId);
    if (t) t.favourite = fav;
  }

  async next() {
    if (this.queueIndex < this.queue.length - 1) {
      this.queueIndex += 1;
      await this.loadCurrent();
    } else {
      this.playing = false;
    }
  }

  async prev() {
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
    this.flushListen(false);
    if (this.audio) {
      this.audio.pause();
      this.audio.removeAttribute("src");
      this.audio.load();
    }
    this.queue = [];
    this.queueIndex = -1;
    this.tracks = [];
    this.playing = false;
    this.currentTime = 0;
    this.duration = 0;
  }
}

export const player = new Player();
