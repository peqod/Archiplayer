import type {
  DayStat,
  Episode,
  EpisodeStat,
  EraStat,
  FavouriteEpisode,
  FavouriteShow,
  FavouriteTrack,
  ShowStat,
} from "$lib/api";

/**
 * The profile page shows one row per entity. Listening stats and favourites arrive as
 * two separate lists that overlap heavily, so they are merged here: a show you both
 * favourited and listened to is one row carrying both facts, not two rows in two columns.
 *
 * `label` is what an A-Z sort orders on; `search` is the wider text a query matches.
 */
export interface ShowRow {
  id: string;
  name: string;
  dj: string | null;
  seconds: number;
  plays: number;
  favourite: boolean;
  addedAt: number | null;
  label: string;
  search: string;
}

export interface EpisodeRow {
  id: number;
  showId: string;
  showName: string;
  airDate: string | null;
  title: string | null;
  seconds: number;
  plays: number;
  favourite: boolean;
  addedAt: number | null;
  /** Present only for favourites; listened-only rows resolve the episode on demand. */
  episode: Episode | null;
  label: string;
  search: string;
}

export interface TrackRowData {
  id: number;
  episodeId: number;
  showId: string;
  showName: string;
  airDate: string | null;
  artist: string | null;
  title: string | null;
  startSec: number | null;
  addedAt: number;
  label: string;
  search: string;
}

export type Scope = "all" | "fav" | "listened";
export type Sort = "time" | "plays" | "name" | "recent";
export type GroupBy = "none" | "show" | "artist";

function text(...parts: (string | null | undefined)[]): string {
  return parts.filter((p) => typeof p === "string" && p !== "").join(" ").toLowerCase();
}

const MONTH_NAMES = [
  "january", "february", "march", "april", "may", "june",
  "july", "august", "september", "october", "november", "december",
];

/**
 * "July 9, 2026" (the form the scraper stores, see `mdy_from_dotted` in wfmu.rs) to
 * "2026-07-09". ISO input passes through, which is what live episodes carry. Anything
 * unreadable returns null rather than a guess.
 *
 * This is the TypeScript twin of `air_date_to_iso` in src-tauri/src/db.rs. Both exist
 * because the backend needs it to bucket in SQL and the frontend needs it to sort rows
 * that only ever carry the raw text.
 */
export function parseAirDate(raw: string | null | undefined): string | null {
  if (!raw) return null;
  const s = raw.trim();
  if (/^\d{4}-\d{2}-\d{2}$/.test(s)) return s;
  const m = /^([A-Za-z]+)\s+(\d{1,2}),\s*(\d{4})$/.exec(s);
  if (!m) return null;
  const month = MONTH_NAMES.indexOf(m[1].toLowerCase());
  const day = Number(m[2]);
  const year = Number(m[3]);
  if (month < 0 || day < 1 || day > 31 || year < 1900 || year > 2100) return null;
  return `${year}-${String(month + 1).padStart(2, "0")}-${String(day).padStart(2, "0")}`;
}

export function mergeShows(stats: ShowStat[], favs: FavouriteShow[]): ShowRow[] {
  const byId = new Map<string, ShowRow>();
  for (const s of stats) {
    byId.set(s.show_id, {
      id: s.show_id,
      name: s.show_name,
      dj: null,
      seconds: s.seconds,
      plays: s.plays,
      favourite: false,
      addedAt: null,
      label: s.show_name.toLowerCase(),
      search: text(s.show_name),
    });
  }
  for (const f of favs) {
    // The favourite carries the richer record (DJ, canonical name); the stat row carries
    // the only listening numbers. Keep both rather than letting one shadow the other.
    const row = byId.get(f.show.id) ?? {
      id: f.show.id,
      name: f.show.name,
      dj: f.show.dj,
      seconds: 0,
      plays: 0,
      favourite: true,
      addedAt: f.added_at,
      label: "",
      search: "",
    };
    row.name = f.show.name;
    row.dj = f.show.dj;
    row.favourite = true;
    row.addedAt = f.added_at;
    row.label = f.show.name.toLowerCase();
    row.search = text(f.show.name, f.show.dj);
    byId.set(f.show.id, row);
  }
  return [...byId.values()];
}

export function mergeEpisodes(
  stats: EpisodeStat[],
  favs: FavouriteEpisode[],
): EpisodeRow[] {
  const byId = new Map<number, EpisodeRow>();
  for (const e of stats) {
    byId.set(e.episode_id, {
      id: e.episode_id,
      showId: e.show_id,
      showName: e.show_name,
      airDate: e.air_date,
      title: e.title,
      seconds: e.seconds,
      plays: e.plays,
      favourite: false,
      addedAt: null,
      episode: null,
      label: text(e.show_name, e.air_date),
      search: text(e.show_name, e.air_date, e.title),
    });
  }
  for (const f of favs) {
    const row = byId.get(f.episode.id) ?? {
      id: f.episode.id,
      showId: f.episode.show_id,
      showName: f.show_name,
      airDate: f.episode.air_date,
      title: f.episode.title,
      seconds: 0,
      plays: 0,
      favourite: true,
      addedAt: f.added_at,
      episode: f.episode,
      label: "",
      search: "",
    };
    row.showId = f.episode.show_id;
    row.showName = f.show_name;
    row.airDate = f.episode.air_date;
    row.title = f.episode.title;
    row.favourite = true;
    row.addedAt = f.added_at;
    row.episode = f.episode;
    row.label = text(f.show_name, f.episode.air_date);
    row.search = text(f.show_name, f.episode.air_date, f.episode.title);
    byId.set(f.episode.id, row);
  }
  return [...byId.values()];
}

export function toTrackRows(favs: FavouriteTrack[]): TrackRowData[] {
  return favs.map((f) => ({
    id: f.track.id,
    episodeId: f.track.episode_id,
    showId: f.show_id,
    showName: f.show_name,
    airDate: f.air_date,
    artist: f.track.artist,
    title: f.track.title,
    startSec: f.track.start_sec,
    addedAt: f.added_at,
    label: text(f.track.artist, f.track.title),
    search: text(f.track.artist, f.track.title, f.show_name, f.air_date),
  }));
}

interface Listable {
  label: string;
  search: string;
  seconds?: number;
  plays?: number;
  favourite?: boolean;
  addedAt?: number | null;
  airDate?: string | null;
}

/**
 * Filter then sort, in that order: the visible count the toolbar reports is the
 * filtered length, and the cap the list applies is a slice of this result.
 */
export function applyList<T extends Listable>(
  rows: T[],
  opts: { query?: string; sort?: Sort; scope?: Scope } = {},
): T[] {
  const query = (opts.query ?? "").trim().toLowerCase();
  const sort = opts.sort ?? "time";
  const scope = opts.scope ?? "all";

  let out = rows;
  if (scope === "fav") out = out.filter((r) => r.favourite === true);
  else if (scope === "listened") out = out.filter((r) => (r.seconds ?? 0) > 0);
  if (query) out = out.filter((r) => r.search.includes(query));

  return [...out].sort((a, b) => {
    switch (sort) {
      case "name":
        return a.label.localeCompare(b.label);
      case "plays":
        return (b.plays ?? 0) - (a.plays ?? 0) || a.label.localeCompare(b.label);
      case "recent": {
        // Air date where there is one (episodes, songs), else when it was starred. Compared
        // as ISO: the raw text is "July 9, 2026", which sorts alphabetically by month name.
        const da = parseAirDate(a.airDate) ?? "";
        const db = parseAirDate(b.airDate) ?? "";
        if (da !== db) return db.localeCompare(da);
        return (b.addedAt ?? 0) - (a.addedAt ?? 0) || a.label.localeCompare(b.label);
      }
      default:
        return (b.seconds ?? 0) - (a.seconds ?? 0) || a.label.localeCompare(b.label);
    }
  });
}

export interface TrackGroup {
  key: string;
  label: string;
  items: TrackRowData[];
}

/**
 * Favourite songs run into the hundreds, where a flat list stops being readable.
 * Grouping collapses them into named blocks with counts.
 */
export function groupTracks(rows: TrackRowData[], by: GroupBy): TrackGroup[] {
  if (by === "none") return [{ key: "", label: "", items: rows }];
  const groups = new Map<string, TrackGroup>();
  for (const row of rows) {
    const label = by === "show" ? row.showName : (row.artist ?? "Unknown artist");
    const key = label.toLowerCase();
    const group = groups.get(key);
    if (group) group.items.push(row);
    else groups.set(key, { key, label, items: [row] });
  }
  // Biggest block first, so what dominates the list is read first.
  return [...groups.values()].sort(
    (a, b) => b.items.length - a.items.length || a.label.localeCompare(b.label),
  );
}

// -- Calendar ---------------------------------------------------------------

export interface DayCell {
  /** YYYY-MM-DD, or "" for the trailing cells of the current week. */
  day: string;
  seconds: number;
  /** 0 (silent) to 4 (busiest quartile). */
  level: number;
}

export interface YearGrid {
  year: number;
  /** Week columns, oldest first; each holds 7 cells, Monday to Sunday. */
  weeks: DayCell[][];
  seconds: number;
  activeDays: number;
  busiest: DayCell | null;
}

/**
 * 54 columns, not the familiar 53. The offset from Monday back to Jan 1 is 0 to 6 days and
 * a year runs 365 or 366 days, so the worst case spans 372 days and needs ceil(372 / 7)
 * columns. At 53 the end of December silently disappears whenever Jan 1 falls on a Sunday
 * in a leap year, as it did in 2012.
 */
export const YEAR_COLUMNS = 54;

const DAY_MS = 86_400_000;

/** Local YYYY-MM-DD, matching SQLite's date(..., 'localtime') bucketing. */
export function dayKey(date: Date): string {
  const y = date.getFullYear();
  const m = String(date.getMonth() + 1).padStart(2, "0");
  const d = String(date.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

function parseDayKey(key: string): number {
  const [y, m, d] = key.split("-").map(Number);
  return new Date(y, (m ?? 1) - 1, d ?? 1).getTime();
}

function startOfDay(date: Date): Date {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

/**
 * Quartile cuts over the *non-zero* days. Scaling off the maximum instead would let
 * one six-hour binge flatten every other day to the lowest level.
 */
function levelThresholds(values: number[]): [number, number, number] {
  const sorted = [...values].sort((a, b) => a - b);
  if (sorted.length === 0) return [0, 0, 0];
  // Nearest rank over (n - 1), not n. Indexing by `n * q` puts the 75th percentile on the
  // last element whenever the sample is small, which makes the top step unreachable: with
  // four active days the busiest one would come out the same shade as the second busiest.
  const at = (q: number) => sorted[Math.floor((sorted.length - 1) * q)];
  return [at(0.25), at(0.5), at(0.75)];
}

/**
 * One grid per calendar year, in the order the years are given. Each row starts on the
 * Monday of the week holding Jan 1; cells outside the year, and cells after today in the
 * current year, are pads with an empty `day`.
 */
export function buildYearGrids(
  days: DayStat[],
  years: number[],
  today = new Date(),
): YearGrid[] {
  const seconds = new Map(days.map((d) => [d.day, d.seconds]));
  // One scale shared by every row. Per-year quartiles would paint a quiet year exactly
  // like a heavy one, and comparing the rows is the whole point of stacking them.
  const [q1, q2, q3] = levelThresholds(days.filter((d) => d.seconds > 0).map((d) => d.seconds));
  const levelOf = (secs: number): number => {
    if (secs <= 0) return 0;
    if (secs <= q1) return 1;
    if (secs <= q2) return 2;
    if (secs <= q3) return 3;
    return 4;
  };
  const end = startOfDay(today);

  return years.map((year) => {
    // Walked with setDate rather than by adding 86.4e6 ms: over seven years the grid
    // crosses a dozen daylight-saving boundaries, and millisecond arithmetic would slide
    // a day onto its neighbour every time the clock shifts.
    const cursor = new Date(year, 0, 1);
    cursor.setDate(cursor.getDate() - ((cursor.getDay() + 6) % 7));

    const weeks: DayCell[][] = [];
    let total = 0;
    let activeDays = 0;
    let busiest: DayCell | null = null;

    for (let w = 0; w < YEAR_COLUMNS; w++) {
      const column: DayCell[] = [];
      for (let d = 0; d < 7; d++) {
        if (cursor.getFullYear() !== year || cursor.getTime() > end.getTime()) {
          column.push({ day: "", seconds: 0, level: 0 });
        } else {
          const secs = seconds.get(dayKey(cursor)) ?? 0;
          const cell: DayCell = { day: dayKey(cursor), seconds: secs, level: levelOf(secs) };
          total += secs;
          if (secs > 0) activeDays++;
          if (secs > (busiest?.seconds ?? 0)) busiest = cell;
          column.push(cell);
        }
        cursor.setDate(cursor.getDate() + 1);
      }
      weeks.push(column);
    }
    return { year, weeks, seconds: total, activeDays, busiest };
  });
}

// -- Archive eras -----------------------------------------------------------

export interface EraCell {
  /** YYYY-MM of the original broadcast. */
  period: string;
  seconds: number;
  plays: number;
  /** 0 (silent) to 4 (heaviest quartile). */
  level: number;
}

export interface EraRow {
  year: number;
  /** Always 12 cells, January to December. */
  months: EraCell[];
  seconds: number;
  plays: number;
}

export interface EraGrid {
  rows: EraRow[];
  /** Busiest year, the denominator for the ranking meters. */
  max: number;
  peak: EraRow | null;
  total: number;
}

/**
 * Listening laid out by the era of the archive it came from, newest year first.
 *
 * Rows span every year between the earliest and the latest with data, so a year you
 * skipped shows up as a real gap in the timeline, but the grid still cannot stretch out to
 * the station's entire history when you have only ever played two decades of it.
 */
export function buildEraGrid(eras: EraStat[]): EraGrid {
  const byYear = new Map<number, EraRow>();
  let lo = Infinity;
  let hi = -Infinity;
  let total = 0;

  const blank = (year: number): EraRow => ({
    year,
    months: Array.from({ length: 12 }, (_, m) => ({
      period: `${year}-${String(m + 1).padStart(2, "0")}`,
      seconds: 0,
      plays: 0,
      level: 0,
    })),
    seconds: 0,
    plays: 0,
  });

  for (const era of eras) {
    const year = Number(era.period.slice(0, 4));
    const month = Number(era.period.slice(5, 7)) - 1;
    if (!Number.isInteger(year) || month < 0 || month > 11) continue;
    let row = byYear.get(year);
    if (!row) {
      row = blank(year);
      byYear.set(year, row);
    }
    row.months[month].seconds += era.seconds;
    row.months[month].plays += era.plays;
    row.seconds += era.seconds;
    row.plays += era.plays;
    total += era.seconds;
    if (year < lo) lo = year;
    if (year > hi) hi = year;
  }

  if (byYear.size === 0) return { rows: [], max: 0, peak: null, total: 0 };

  const rows: EraRow[] = [];
  for (let year = hi; year >= lo; year--) rows.push(byYear.get(year) ?? blank(year));

  const [q1, q2, q3] = levelThresholds(
    rows.flatMap((r) => r.months.map((m) => m.seconds)).filter((s) => s > 0),
  );
  let max = 0;
  let peak: EraRow | null = null;
  for (const row of rows) {
    for (const cell of row.months) {
      if (cell.seconds <= 0) continue;
      cell.level = cell.seconds <= q1 ? 1 : cell.seconds <= q2 ? 2 : cell.seconds <= q3 ? 3 : 4;
    }
    if (row.seconds > max) {
      max = row.seconds;
      peak = row;
    }
  }
  return { rows, max, peak, total };
}

export interface Streaks {
  current: number;
  longest: number;
}

/**
 * Consecutive days with any listening. The current streak counts back from today and
 * tolerates today being silent so far: a run that reached yesterday is still live.
 */
export function streaks(days: DayStat[], today = new Date()): Streaks {
  const active = days
    .filter((d) => d.seconds > 0)
    .map((d) => parseDayKey(d.day))
    .sort((a, b) => a - b);
  if (active.length === 0) return { current: 0, longest: 0 };

  let longest = 1;
  let run = 1;
  for (let i = 1; i < active.length; i++) {
    const gap = Math.round((active[i] - active[i - 1]) / DAY_MS);
    if (gap === 1) run++;
    else if (gap > 1) run = 1;
    if (run > longest) longest = run;
  }

  const sinceLast = Math.round((startOfDay(today).getTime() - active[active.length - 1]) / DAY_MS);
  if (sinceLast > 1) return { current: 0, longest };

  let current = 1;
  for (let i = active.length - 1; i > 0; i--) {
    if (Math.round((active[i] - active[i - 1]) / DAY_MS) !== 1) break;
    current++;
  }
  return { current, longest };
}

/** Seconds listened in the calendar month containing `today`. */
export function monthSeconds(days: DayStat[], today = new Date()): number {
  const prefix = `${today.getFullYear()}-${String(today.getMonth() + 1).padStart(2, "0")}-`;
  let total = 0;
  for (const d of days) if (d.day.startsWith(prefix)) total += d.seconds;
  return total;
}
