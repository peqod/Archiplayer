import { friendlyError, isOfflineError } from "$lib/errors";

// Tiny global toast. One message at a time; auto-clears. Mirrors the floating
// `.fav-error` pattern in +layout.svelte. Rendered once by Toast.svelte.
let msg = $state<string | null>(null);
let timer: ReturnType<typeof setTimeout> | undefined;

export const toastState = {
  get msg() {
    return msg;
  },
};

export function toast(text: string, ms = 1600) {
  msg = text;
  clearTimeout(timer);
  timer = setTimeout(() => (msg = null), ms);
}

/**
 * Surface a failure. Connection trouble goes to the floating toast, out of the page flow;
 * anything else the user can act on stays in the caller's inline banner.
 */
export function reportError(e: unknown, inline: (msg: string | null) => void): void {
  const friendly = friendlyError(e);
  if (isOfflineError(e)) {
    toast(friendly, 3200);
    inline(null);
    return;
  }
  inline(friendly);
}
