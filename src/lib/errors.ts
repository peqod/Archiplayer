// Rust commands reject with their raw error string, so a dropped connection reaches the UI
// as `request failed: error sending request for url (https://wfmu.org/playlists/): error
// trying to connect: dns error: failed to lookup address information`. That reads like a
// stack trace in a red banner. Everything user-facing goes through here instead: the
// connection family collapses to one short line, and it is shown floating rather than in
// the page flow so a flaky network never shoves the layout around (see `reportError` in
// toaster.svelte.ts, which owns that floating channel). No imports here: the tests load this
// module straight through node --experimental-strip-types.

const OFFLINE_MARKS = [
  "request failed",
  "error sending request",
  "dns error",
  "failed to lookup address",
  "connection refused",
  "connection reset",
  "connection closed",
  "connect error",
  "error trying to connect",
  "timed out",
  "timeout",
  "network is unreachable",
  "network error",
  "networkerror",
  "failed to fetch",
  "load failed",
  "err_internet_disconnected",
  "err_name_not_resolved",
  "err_connection",
  // Winsock: 11001 host not found, 11002 non-authoritative host not found.
  "os error 11001",
  "os error 11002",
];

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

export function isOfflineError(e: unknown): boolean {
  const raw = errorText(e).toLowerCase();
  if (OFFLINE_MARKS.some((mark) => raw.includes(mark))) return true;
  // A browser-side failure with the machine already known to be offline needs no marker.
  return typeof navigator !== "undefined" && navigator.onLine === false && raw.length > 0;
}

/** Short, plain sentence for any command or playback failure. */
export function friendlyError(e: unknown): string {
  const raw = errorText(e);
  if (isOfflineError(e)) return "No connection.";
  const lower = raw.toLowerCase();
  if (/^http 5\d\d/.test(lower)) return "WFMU is not responding right now.";
  if (lower.startsWith("http 404")) return "That page is gone from WFMU.";
  if (/^http \d\d\d/.test(lower)) return "WFMU refused the request.";
  if (lower.startsWith("db error")) return "Could not read the local library.";
  const trimmed = raw.replace(/^error:\s*/i, "").trim();
  if (!trimmed) return "Something went wrong.";
  const sentence = trimmed[0].toUpperCase() + trimmed.slice(1);
  return /[.!?]$/.test(sentence) ? sentence : `${sentence}.`;
}
