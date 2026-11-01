import { api } from "./api";
import { toast } from "./toaster.svelte";

/**
 * Songs starred from a live stream have no audio until WFMU archives the broadcast. The
 * backend looks for the archived copy and moves the favourite there (see
 * `relink_live_favourites`). It takes a while, so it runs in the background, and pages that
 * list favourites watch `version` to reload when something moved.
 */
export const liveRelink = $state({ running: false, version: 0 });

let inFlight: Promise<void> | null = null;

/** Start a relink, or join the one already running. Never throws. */
export function runLiveRelink(): Promise<void> {
  if (inFlight) return inFlight;
  liveRelink.running = true;
  inFlight = api
    .relinkLiveFavourites()
    .then((moved) => {
      if (moved > 0) {
        liveRelink.version += 1;
        toast(
          moved === 1
            ? "1 live favourite now plays from the archive."
            : `${moved} live favourites now play from the archive.`,
        );
      }
    })
    .catch((e) => console.warn("[archiplayer]", "live favourite relink failed", e))
    .finally(() => {
      liveRelink.running = false;
      inFlight = null;
    });
  return inFlight;
}
