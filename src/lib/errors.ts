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

// Browser media errors carry developer text ("The play() request was interrupted by a call
// to pause()."), so they map by name, never by message.
const NAMED: Record<string, string> = {
  AbortError: "Playback stopped.",
  NotAllowedError: "Press play to start.",
  NotSupportedError: "This audio can't be played. The archive may be unavailable.",
  SecurityError: "Playback was blocked.",
};

// Known Rust command messages, in match order. Patterns test the lowercased raw text.
const RULES: [RegExp, string][] = [
  [/^http \d+ downloading audio/, "The download failed. Try again later."],
  [/lock poisoned/, "Something went wrong. Restart Archiplayer."],
  [/catalog parse produced no shows/, "Could not read the WFMU show list. Try again later."],
  [/carries no audio source|audio url/, "No audio found for this episode."],
  [/download destination already exists/, "This episode is already downloaded."],
  [
    /could not inspect download destination|os error 5|access is denied/,
    "Could not save the download. Check the folder.",
  ],
  [/show page was empty/, "This show has no episodes yet."],
];

const GENERIC = "Something went wrong. Try again.";

function errorName(e: unknown): string | null {
  if (e && typeof e === "object" && "name" in e) {
    const name = (e as { name: unknown }).name;
    return typeof name === "string" ? name : null;
  }
  return null;
}

// Only short plain sentences reach the user as-is; anything that reads like a log line
// (URLs, OS error codes, call parens, Rust paths, colon chains) does not.
function looksPlain(text: string): boolean {
  if (text.length > 80) return false;
  if (/:\/\/|os error|\(\)|::/i.test(text)) return false;
  return (text.match(/:/g) ?? []).length <= 1;
}

function replaced(raw: string, message: string): string {
  if (raw) console.warn("[archiplayer]", raw);
  return message;
}

/**
 * Short, plain sentence for any command or playback failure. `fallback` replaces the
 * generic line when the raw text is not fit to show.
 */
export function friendlyError(e: unknown, fallback: string = GENERIC): string {
  const raw = errorText(e);
  const name = errorName(e);
  if (name && Object.prototype.hasOwnProperty.call(NAMED, name)) return replaced(raw, NAMED[name]);
  if (isOfflineError(e)) return "No connection.";
  const lower = raw.toLowerCase();
  for (const [pattern, message] of RULES) {
    if (pattern.test(lower)) return replaced(raw, message);
  }
  if (/^http 5\d\d/.test(lower)) return replaced(raw, "WFMU is not responding right now.");
  if (lower.startsWith("http 404")) return replaced(raw, "That page is gone from WFMU.");
  if (/^http \d\d\d/.test(lower)) return replaced(raw, "WFMU refused the request.");
  if (lower.startsWith("db error")) return replaced(raw, "Could not read the local library.");
  const trimmed = raw.replace(/^error:\s*/i, "").trim();
  if (!trimmed) return "Something went wrong.";
  if (!looksPlain(trimmed)) return replaced(raw, fallback);
  const sentence = trimmed[0].toUpperCase() + trimmed.slice(1);
  return /[.!?]$/.test(sentence) ? sentence : `${sentence}.`;
}
