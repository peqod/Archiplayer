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
