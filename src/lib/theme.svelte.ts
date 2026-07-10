// Design-token theming. Presets set CSS custom properties on :root; each preset can be
// customized per-token via colour pickers, persisted in localStorage.

export type TokenKey =
  | "bg"
  | "surface"
  | "surface2"
  | "border"
  | "text"
  | "dim"
  | "accent"
  | "onAccent"
  | "gold"
  | "danger"
  | "line";

export type Palette = Record<TokenKey, string>;

export const TOKENS: { key: TokenKey; label: string }[] = [
  { key: "accent", label: "Accent" },
  { key: "gold", label: "Star / gold" },
  { key: "bg", label: "Background" },
  { key: "surface", label: "Panel" },
  { key: "surface2", label: "Hover" },
  { key: "border", label: "Border" },
  { key: "text", label: "Text" },
  { key: "dim", label: "Muted text" },
  { key: "onAccent", label: "On accent" },
  { key: "danger", label: "Danger" },
  { key: "line", label: "Brand line" },
];

export interface Preset {
  id: string;
  name: string;
  note: string;
  palette: Palette;
}

// Derived from the Archiplayer brandbook (teal / cream / navy / maroon).
export const PRESETS: Preset[] = [
  {
    id: "archiplayer",
    name: "Archiplayer",
    note: "Brandbook — teal, cream & navy",
    palette: {
      bg: "#141A2E",
      surface: "#1E2440",
      surface2: "#2B3355",
      border: "#38406A",
      text: "#F3EDC6",
      dim: "#A6A2B4",
      accent: "#12A594",
      onAccent: "#04231F",
      gold: "#FFDD87",
      danger: "#C4453C",
      line: "#D8483F",
    },
  },
  {
    id: "amber",
    name: "Midnight Amber",
    note: "The original — warm amber on ink",
    palette: {
      bg: "#141419",
      surface: "#1D1D25",
      surface2: "#2A2A35",
      border: "#33333F",
      text: "#E8E4DC",
      dim: "#8A8694",
      accent: "#FF9933",
      onAccent: "#1D1D25",
      gold: "#FFD24D",
      danger: "#FF5555",
      line: "#FF3333",
    },
  },
  {
    id: "paper",
    name: "Cream Paper",
    note: "Light — ink on paper, teal accent",
    palette: {
      bg: "#F4F0E2",
      surface: "#FFFFFF",
      surface2: "#EAE4D2",
      border: "#D7CFBB",
      text: "#20242F",
      dim: "#6B6656",
      accent: "#0E9E8E",
      onAccent: "#FFFFFF",
      gold: "#D79A00",
      danger: "#B23A2E",
      line: "#0E9E8E",
    },
  },
  {
    id: "dusk",
    name: "Neon Dusk",
    note: "Trending — magenta & cyan on plum",
    palette: {
      bg: "#16131F",
      surface: "#201B2E",
      surface2: "#2E2742",
      border: "#3D3459",
      text: "#EDE7F6",
      dim: "#9A90B0",
      accent: "#E85D9E",
      onAccent: "#1A0E14",
      gold: "#5BE1E6",
      danger: "#FF5C7A",
      line: "#E85D9E",
    },
  },
  {
    id: "forest",
    name: "Forest Gold",
    note: "Deep green & antique gold",
    palette: {
      bg: "#10201A",
      surface: "#172A22",
      surface2: "#21382E",
      border: "#2E4A3C",
      text: "#EDE8D0",
      dim: "#93A08F",
      accent: "#E1B84B",
      onAccent: "#181F16",
      gold: "#E1B84B",
      danger: "#C4553C",
      line: "#E1B84B",
    },
  },
];

const cssVar: Record<TokenKey, string> = {
  bg: "--c-bg",
  surface: "--c-surface",
  surface2: "--c-surface2",
  border: "--c-border",
  text: "--c-text",
  dim: "--c-dim",
  accent: "--c-accent",
  onAccent: "--c-on-accent",
  gold: "--c-gold",
  danger: "--c-danger",
  line: "--c-line",
};

const LS_ACTIVE = "ap.theme.active";
const LS_OVERRIDES = "ap.theme.overrides";

function presetById(id: string): Preset {
  return PRESETS.find((p) => p.id === id) ?? PRESETS[0];
}

class ThemeStore {
  activeId = $state("archiplayer");
  // per-preset token overrides
  overrides = $state<Record<string, Partial<Palette>>>({});

  get active(): Preset {
    return presetById(this.activeId);
  }

  /** Effective palette = preset defaults merged with the user's overrides for it. */
  get palette(): Palette {
    return { ...this.active.palette, ...(this.overrides[this.activeId] ?? {}) };
  }

  isLight(): boolean {
    // luminance of the background
    const hex = this.palette.bg.replace("#", "");
    const r = parseInt(hex.slice(0, 2), 16);
    const g = parseInt(hex.slice(2, 4), 16);
    const b = parseInt(hex.slice(4, 6), 16);
    return (0.299 * r + 0.587 * g + 0.114 * b) / 255 > 0.55;
  }

  load() {
    try {
      const a = localStorage.getItem(LS_ACTIVE);
      if (a && PRESETS.some((p) => p.id === a)) this.activeId = a;
      const o = localStorage.getItem(LS_OVERRIDES);
      if (o) this.overrides = JSON.parse(o);
    } catch {
      /* ignore */
    }
    this.apply();
  }

  apply() {
    if (typeof document === "undefined") return;
    const root = document.documentElement;
    const pal = this.palette;
    for (const key of Object.keys(cssVar) as TokenKey[]) {
      root.style.setProperty(cssVar[key], pal[key]);
    }
    root.style.colorScheme = this.isLight() ? "light" : "dark";
  }

  select(id: string) {
    this.activeId = id;
    localStorage.setItem(LS_ACTIVE, id);
    this.apply();
  }

  setToken(key: TokenKey, value: string) {
    const cur = { ...(this.overrides[this.activeId] ?? {}) };
    cur[key] = value;
    this.overrides = { ...this.overrides, [this.activeId]: cur };
    localStorage.setItem(LS_OVERRIDES, JSON.stringify(this.overrides));
    this.apply();
  }

  resetActive() {
    const { [this.activeId]: _drop, ...rest } = this.overrides;
    this.overrides = rest;
    localStorage.setItem(LS_OVERRIDES, JSON.stringify(this.overrides));
    this.apply();
  }

  hasOverrides(): boolean {
    return Object.keys(this.overrides[this.activeId] ?? {}).length > 0;
  }
}

export const theme = new ThemeStore();
