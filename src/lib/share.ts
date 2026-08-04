// Share helpers. Prefer the native OS share sheet (navigator.share) when the
// webview exposes it; otherwise copy a link to the clipboard and flash a toast.
// On desktop WebView2 navigator.share is usually absent, so the clipboard path
// is what runs there. No Tauri plugin needed — both APIs work in the app's
// secure context on a user gesture.
import type { Episode, Show, Track } from "$lib/api";
import { toast } from "$lib/toaster.svelte";

export function wfmuShowUrl(id: string): string {
  return "https://wfmu.org/playlists/" + id;
}

// WFMU playlist pages are keyed by the episode (playlist) id — the same id the backend
// uses to fetch a playlist — not by archive_id (which keys the archive audio player).
export function wfmuEpisodeUrl(episodeId: number): string {
  return "https://wfmu.org/playlists/shows/" + episodeId;
}

interface ShareData {
  title?: string;
  text?: string;
  url?: string;
}

export async function shareContent(data: ShareData): Promise<void> {
  if (
    typeof navigator.share === "function" &&
    (!navigator.canShare || navigator.canShare(data))
  ) {
    try {
      await navigator.share(data);
      return;
    } catch (e) {
      // User dismissed the sheet — done, no fallback.
      if ((e as Error).name === "AbortError") return;
      // Any other failure falls through to the clipboard copy below.
    }
  }
  const copy = data.url ?? data.text ?? "";
  try {
    await navigator.clipboard.writeText(copy);
    toast("Link copied");
  } catch {
    toast("Copy failed");
  }
}

// The *Ref forms take only the fields a share needs, for callers holding a merged row
// rather than a whole record (the profile lists). The record forms delegate to them.
export function shareShowRef(
  id: string,
  name: string,
  dj: string | null,
): Promise<void> {
  return shareContent({
    title: name,
    text: `${name}${dj ? " with " + dj : ""} on WFMU`,
    url: wfmuShowUrl(id),
  });
}

export function shareShow(show: Show): Promise<void> {
  return shareShowRef(show.id, show.name, show.dj);
}

export function shareEpisodeRef(
  showName: string,
  episodeId: number,
  airDate: string | null,
  title: string | null,
): Promise<void> {
  return shareContent({
    title: showName,
    text: `${showName} — ${airDate ?? ""}${title ? " · " + title : ""}`,
    url: wfmuEpisodeUrl(episodeId),
  });
}

export function shareEpisode(showName: string, ep: Episode): Promise<void> {
  return shareEpisodeRef(showName, ep.id, ep.air_date, ep.title);
}

export function shareTrackRef(
  artist: string | null,
  title: string | null,
  showName: string,
  airDate: string | null,
  epUrl: string,
): Promise<void> {
  return shareContent({
    title: `${artist ?? "?"} – ${title ?? "?"}`,
    text: `${artist ?? "?"} – ${title ?? "?"}\nfrom ${showName}${airDate ? ", " + airDate : ""}`,
    url: epUrl,
  });
}

export function shareTrack(
  track: Track,
  showName: string,
  airDate: string | null,
  epUrl: string,
): Promise<void> {
  return shareTrackRef(track.artist, track.title, showName, airDate, epUrl);
}
