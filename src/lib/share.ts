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

export function wfmuEpisodeUrl(archiveId: number): string {
  return "https://wfmu.org/playlists/shows/" + archiveId;
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

export function shareShow(show: Show): Promise<void> {
  return shareContent({
    title: show.name,
    text: `${show.name}${show.dj ? " with " + show.dj : ""} on WFMU`,
    url: wfmuShowUrl(show.id),
  });
}

export function shareEpisode(showName: string, ep: Episode): Promise<void> {
  const url =
    ep.archive_id != null ? wfmuEpisodeUrl(ep.archive_id) : wfmuShowUrl(ep.show_id);
  return shareContent({
    title: showName,
    text: `${showName} — ${ep.air_date ?? ""}${ep.title ? " · " + ep.title : ""}`,
    url,
  });
}

export function shareTrack(
  track: Track,
  showName: string,
  airDate: string | null,
  epUrl: string,
): Promise<void> {
  return shareContent({
    title: `${track.artist ?? "?"} – ${track.title ?? "?"}`,
    text: `${track.artist ?? "?"} – ${track.title ?? "?"}\nfrom ${showName}${airDate ? ", " + airDate : ""}`,
    url: epUrl,
  });
}
