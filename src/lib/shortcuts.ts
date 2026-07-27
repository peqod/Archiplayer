// Key bindings, kept away from the DOM and the store so the parsing rules can be
// tested on their own. Two tiers share one action list: `local` fires from the
// window's keydown while Archiplayer has focus, `global` is handed to the OS via
// tauri-plugin-global-shortcut and fires from anywhere.
//
// Accelerators are written in Tauri's own syntax ("Ctrl+Alt+M", "Space",
// "BracketRight") so a stored string can go straight to `register()` without a
// second dialect in between.

export type ActionId =
  | "play-pause"
  | "next-track"
  | "prev-track"
  | "next-episode"
  | "prev-episode"
  | "mute"
  | "fav-song"
  | "save-show"
  | "random";

export type Scope = "local" | "global";

export interface ActionDef {
  id: ActionId;
  /** Matches the header button's wording, so the same act has one name. */
  label: string;
}

export const ACTIONS: readonly ActionDef[] = [
  { id: "play-pause", label: "Play / pause" },
  { id: "prev-track", label: "Previous song" },
  { id: "next-track", label: "Next song" },
  { id: "prev-episode", label: "Previous episode" },
  { id: "next-episode", label: "Next episode" },
  { id: "mute", label: "Mute" },
  { id: "fav-song", label: "Star this song" },
  { id: "save-show", label: "Save this episode" },
  { id: "random", label: "Random audition" },
];

export type Bindings = Record<ActionId, string | null>;

/** Bare keys are fine here: nothing outside the focused window ever sees them. */
export const DEFAULT_LOCAL: Bindings = {
  "play-pause": "Space",
  "prev-track": "J",
  "next-track": "K",
  "prev-episode": "Comma",
  "next-episode": "Period",
  mute: "M",
  "fav-song": "F",
  "save-show": "S",
  random: "R",
};

// Transport is left unbound on purpose. mediaSession already carries play/pause and
// song stepping to the OS, and a global grab on the media keys would starve it.
export const DEFAULT_GLOBAL: Bindings = {
  "play-pause": null,
  "prev-track": null,
  "next-track": null,
  "prev-episode": "Ctrl+Alt+BracketLeft",
  "next-episode": "Ctrl+Alt+BracketRight",
  mute: "Ctrl+Alt+M",
  "fav-song": "Ctrl+Alt+F",
  "save-show": "Ctrl+Alt+S",
  random: "Ctrl+Alt+R",
};

export function defaultsFor(scope: Scope): Bindings {
  return { ...(scope === "global" ? DEFAULT_GLOBAL : DEFAULT_LOCAL) };
}

const ACTION_IDS = new Set<string>(ACTIONS.map((a) => a.id));

// Written in the order they are emitted, which is also the order they are read.
const MODIFIERS = ["Ctrl", "Alt", "Shift", "Super"] as const;
type Modifier = (typeof MODIFIERS)[number];

const MODIFIER_ALIASES: Record<string, Modifier> = {
  ctrl: "Ctrl",
  control: "Ctrl",
  alt: "Alt",
  option: "Alt",
  shift: "Shift",
  super: "Super",
  cmd: "Super",
  command: "Super",
  meta: "Super",
  win: "Super",
  commandorcontrol: "Ctrl",
  cmdorctrl: "Ctrl",
};

const PUNCTUATION: Record<string, string> = {
  Space: "Space",
  Comma: "Comma",
  Period: "Period",
  Slash: "Slash",
  Backslash: "Backslash",
  Semicolon: "Semicolon",
  Quote: "Quote",
  Backquote: "Backquote",
  BracketLeft: "BracketLeft",
  BracketRight: "BracketRight",
  Minus: "Minus",
  Equal: "Equal",
};

const NAVIGATION: Record<string, string> = {
  ArrowUp: "ArrowUp",
  ArrowDown: "ArrowDown",
  ArrowLeft: "ArrowLeft",
  ArrowRight: "ArrowRight",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  Insert: "Insert",
  Delete: "Delete",
  Enter: "Enter",
  Tab: "Tab",
  Backspace: "Backspace",
};

const MEDIA: Record<string, string> = {
  MediaPlayPause: "MediaPlayPause",
  MediaTrackNext: "MediaTrackNext",
  MediaTrackPrevious: "MediaTrackPrevious",
  MediaStop: "MediaStop",
};

/**
 * The key half of an accelerator, derived from `KeyboardEvent.code` so a binding
 * means the same physical key on every layout. Returns null for keys Tauri cannot
 * parse and for the modifiers themselves, which are never a binding on their own.
 */
export function keyTokenFromCode(code: string): string | null {
  if (/^Key[A-Z]$/.test(code)) return code.slice(3);
  if (/^Digit[0-9]$/.test(code)) return code.slice(5);
  if (/^Numpad[0-9]$/.test(code)) return code;
  if (/^F([1-9]|1[0-9]|2[0-4])$/.test(code)) return code;
  return PUNCTUATION[code] ?? NAVIGATION[code] ?? MEDIA[code] ?? null;
}

const KEY_ALIASES: Record<string, string> = {
  // Both halves of what a person might type into a settings file by hand.
  esc: "Escape",
  escape: "Escape",
  return: "Enter",
  spacebar: "Space",
  "[": "BracketLeft",
  "]": "BracketRight",
  ",": "Comma",
  ".": "Period",
  "/": "Slash",
  "\\": "Backslash",
  ";": "Semicolon",
  "'": "Quote",
  "`": "Backquote",
  "-": "Minus",
  "=": "Equal",
};

function canonicalKey(token: string): string | null {
  const alias = KEY_ALIASES[token.toLowerCase()] ?? KEY_ALIASES[token];
  const raw = alias ?? token;
  if (/^[a-z]$/i.test(raw)) return raw.toUpperCase();
  if (/^[0-9]$/.test(raw)) return raw;
  const cased = [
    ...Object.keys(PUNCTUATION),
    ...Object.keys(NAVIGATION),
    ...Object.keys(MEDIA),
  ].find((k) => k.toLowerCase() === raw.toLowerCase());
  if (cased) return cased;
  if (/^numpad[0-9]$/i.test(raw)) return "Numpad" + raw.slice(-1);
  if (/^f([1-9]|1[0-9]|2[0-4])$/i.test(raw)) return "F" + raw.slice(1);
  return null;
}

export interface Accel {
  mods: Modifier[];
  key: string;
}

function join(mods: Modifier[], key: string): string {
  const ordered = MODIFIERS.filter((m) => mods.includes(m));
  return [...ordered, key].join("+");
}

/** Parse an accelerator into its parts, or null if it is not one we can honour. */
export function parseAccel(accel: string | null | undefined): Accel | null {
  if (typeof accel !== "string") return null;
  const parts = accel.split("+").map((p) => p.trim()).filter((p) => p.length > 0);
  if (!parts.length) return null;
  const mods: Modifier[] = [];
  for (const part of parts.slice(0, -1)) {
    const mod = MODIFIER_ALIASES[part.toLowerCase()];
    if (!mod || mods.includes(mod)) return null;
    mods.push(mod);
  }
  const key = canonicalKey(parts[parts.length - 1]);
  if (!key || key === "Escape") return null;
  return { mods: MODIFIERS.filter((m) => mods.includes(m)), key };
}

/** Re-emit an accelerator in canonical form, or null if it cannot be parsed. */
export function normalizeAccel(accel: string | null | undefined): string | null {
  const parsed = parseAccel(accel);
  return parsed ? join(parsed.mods, parsed.key) : null;
}

/**
 * The accelerator a keypress stands for, or null when there is nothing to bind:
 * a modifier held on its own, Escape (the recorder's cancel), or a key Tauri
 * cannot name.
 */
export function accelFromEvent(e: {
  code: string;
  ctrlKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  metaKey: boolean;
}): string | null {
  const key = keyTokenFromCode(e.code);
  if (!key) return null;
  const mods: Modifier[] = [];
  if (e.ctrlKey) mods.push("Ctrl");
  if (e.altKey) mods.push("Alt");
  if (e.shiftKey) mods.push("Shift");
  if (e.metaKey) mods.push("Super");
  return join(mods, key);
}

/** Does this keypress fire this binding? */
export function matchesAccel(
  e: {
    code: string;
    ctrlKey: boolean;
    altKey: boolean;
    shiftKey: boolean;
    metaKey: boolean;
  },
  accel: string | null | undefined,
): boolean {
  const want = parseAccel(accel);
  if (!want) return false;
  const got = accelFromEvent(e);
  return got !== null && got === join(want.mods, want.key);
}

/**
 * An OS-wide binding takes the key away from every other application, so it has to
 * carry a modifier no one types by accident. Media keys pass on their own: they
 * exist for exactly this.
 */
export function isGlobalSafe(accel: string | null | undefined): boolean {
  const parsed = parseAccel(accel);
  if (!parsed) return false;
  if (parsed.key in MEDIA) return true;
  return parsed.mods.some((m) => m === "Ctrl" || m === "Alt" || m === "Super");
}

/**
 * Accelerators bound to more than one action in the same tier. The two tiers are
 * free to overlap, since only one of them is listening at a time.
 */
export function duplicates(bindings: Partial<Bindings>): string[] {
  const seen = new Map<string, number>();
  for (const accel of Object.values(bindings)) {
    const norm = normalizeAccel(accel);
    if (!norm) continue;
    seen.set(norm, (seen.get(norm) ?? 0) + 1);
  }
  return [...seen.entries()]
    .filter(([, n]) => n > 1)
    .map(([accel]) => accel)
    .sort();
}

const PRETTY: Record<string, string> = {
  Space: "Space",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Backquote: "`",
  BracketLeft: "[",
  BracketRight: "]",
  Minus: "-",
  Equal: "=",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  MediaPlayPause: "Media play/pause",
  MediaTrackNext: "Media next",
  MediaTrackPrevious: "Media previous",
  MediaStop: "Media stop",
  Super: "Win",
};

/** How a binding reads in the settings fold. Empty string for an unbound action. */
export function accelLabel(accel: string | null | undefined): string {
  const parsed = parseAccel(accel);
  if (!parsed) return "";
  const mods = parsed.mods.map((m) => PRETTY[m] ?? m);
  return [...mods, PRETTY[parsed.key] ?? parsed.key].join(" + ");
}

// Elements that own the keyboard while they are focused. Letting a binding through
// here is what turns typing "kraftwerk" into a skipped song.
const TEXT_ENTRY = new Set(["INPUT", "TEXTAREA", "SELECT"]);
// Space and Enter activate the focused control natively; anything else is ours.
const ACTIVATION_KEYS = new Set(["Space", "Enter"]);
const ACTIVATABLE = new Set(["BUTTON", "A", "SUMMARY", "DETAILS"]);

/**
 * Should a keypress on this element be left to the browser? Text fields swallow
 * everything (including the hidden range inputs behind the seek and volume tracks,
 * where the arrows are the only way to seek by keyboard). Buttons and links swallow
 * only the keys that activate them, so `M` still mutes right after a click.
 */
export function shouldIgnoreKey(target: unknown, accel: string | null): boolean {
  const el = target as { tagName?: unknown; isContentEditable?: unknown } | null;
  const tag = typeof el?.tagName === "string" ? el.tagName.toUpperCase() : "";
  if (el?.isContentEditable === true) return true;
  if (TEXT_ENTRY.has(tag)) return true;
  const parsed = parseAccel(accel);
  if (!parsed) return false;
  return ACTIVATABLE.has(tag) && !parsed.mods.length && ACTIVATION_KEYS.has(parsed.key);
}

export interface StoredShortcuts {
  enabled: boolean;
  local: Bindings;
  global: Bindings;
}

function readTier(raw: unknown, scope: Scope): Bindings {
  const out = defaultsFor(scope);
  if (!raw || typeof raw !== "object") return out;
  for (const [id, accel] of Object.entries(raw as Record<string, unknown>)) {
    // Actions that no longer exist are dropped; ones not mentioned keep their
    // default, so shipping a new action does not invalidate a saved file.
    if (!ACTION_IDS.has(id)) continue;
    out[id as ActionId] = normalizeAccel(typeof accel === "string" ? accel : null);
  }
  return out;
}

/**
 * Read the persisted blob. Anything unreadable falls back to the defaults rather
 * than throwing, and a blob from before the master switch existed reads as on.
 */
export function parseStored(raw: string | null | undefined): StoredShortcuts {
  let data: unknown = null;
  if (typeof raw === "string" && raw.length) {
    try {
      data = JSON.parse(raw);
    } catch {
      data = null;
    }
  }
  const obj = (data && typeof data === "object" ? data : {}) as Record<string, unknown>;
  return {
    enabled: obj.enabled !== false,
    local: readTier(obj.local, "local"),
    global: readTier(obj.global, "global"),
  };
}

export function serializeShortcuts(state: StoredShortcuts): string {
  return JSON.stringify({ v: 1, enabled: state.enabled, local: state.local, global: state.global });
}
