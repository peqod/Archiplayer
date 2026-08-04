import assert from "node:assert/strict";
import {
  applyList,
  buildEraGrid,
  buildYearGrids,
  dayKey,
  groupTracks,
  mergeEpisodes,
  mergeShows,
  monthSeconds,
  parseAirDate,
  streaks,
  toTrackRows,
} from "../src/lib/profile-lists.ts";

function test(name, body) {
  body();
  process.stdout.write(`ok - ${name}\n`);
}

const show = (id, name, dj = null) => ({
  id,
  name,
  dj,
  description: null,
  on_air: false,
  episode_count: 0,
  favourite: true,
  last_scraped: null,
});

const episode = (id, showId, airDate, title = null) => ({
  id,
  show_id: showId,
  air_date: airDate,
  title,
  archive_id: null,
  audio_url: null,
  has_audio: true,
  favourite: true,
  downloaded: false,
  download_path: null,
  track_count: 0,
  resume_sec: null,
  duration_sec: null,
  completed: false,
  offset_sec: null,
  broadcast_duration_sec: null,
});

test("a show that is both listened to and favourited merges into one row", () => {
  const rows = mergeShows(
    [{ show_id: "WB", show_name: "Wake N Bake", seconds: 7200, plays: 4 }],
    [{ show: show("WB", "Wake N Bake", "Clay Pigeon"), added_at: 99 }],
  );
  assert.equal(rows.length, 1);
  assert.deepEqual(
    { seconds: rows[0].seconds, plays: rows[0].plays, dj: rows[0].dj, fav: rows[0].favourite },
    { seconds: 7200, plays: 4, dj: "Clay Pigeon", fav: true },
  );
});

test("listened-only and favourite-only shows both survive the merge", () => {
  const rows = mergeShows(
    [{ show_id: "A", show_name: "Heard", seconds: 60, plays: 1 }],
    [{ show: show("B", "Starred"), added_at: 1 }],
  );
  assert.deepEqual(
    rows.map((r) => [r.id, r.seconds, r.favourite]).sort(),
    [
      ["A", 60, false],
      ["B", 0, true],
    ],
  );
});

test("episode merge keeps the show id and the playable episode object", () => {
  const rows = mergeEpisodes(
    [
      {
        episode_id: 1,
        show_id: "WB",
        show_name: "Wake N Bake",
        air_date: "2026-01-02",
        title: null,
        seconds: 600,
        plays: 2,
      },
      {
        episode_id: 2,
        show_id: "MP",
        show_name: "Mac's Place",
        air_date: "2026-02-02",
        title: null,
        seconds: 300,
        plays: 1,
      },
    ],
    [{ episode: episode(1, "WB", "2026-01-02"), show_name: "Wake N Bake", added_at: 5 }],
  );
  const merged = rows.find((r) => r.id === 1);
  const statOnly = rows.find((r) => r.id === 2);
  assert.equal(rows.length, 2);
  assert.equal(merged.episode?.id, 1);
  assert.equal(merged.favourite, true);
  assert.equal(statOnly.episode, null);
  assert.equal(statOnly.showId, "MP"); // links to the show page without a favourite record
});

test("scope filters split favourites from listening history", () => {
  const rows = mergeShows(
    [{ show_id: "A", show_name: "Heard", seconds: 60, plays: 1 }],
    [{ show: show("B", "Starred"), added_at: 1 }],
  );
  assert.deepEqual(applyList(rows, { scope: "fav" }).map((r) => r.id), ["B"]);
  assert.deepEqual(applyList(rows, { scope: "listened" }).map((r) => r.id), ["A"]);
  assert.equal(applyList(rows, { scope: "all" }).length, 2);
});

test("search matches the DJ and the title, not only the name", () => {
  const rows = mergeShows(
    [],
    [
      { show: show("A", "Wake N Bake", "Clay Pigeon"), added_at: 1 },
      { show: show("B", "Mac's Place", "Mac"), added_at: 2 },
    ],
  );
  assert.deepEqual(applyList(rows, { query: "clay" }).map((r) => r.id), ["A"]);
  assert.deepEqual(applyList(rows, { query: "PLACE" }).map((r) => r.id), ["B"]);
  assert.equal(applyList(rows, { query: "   " }).length, 2); // blank query is no filter
});

test("sorts order by time, plays and name", () => {
  const rows = mergeShows(
    [
      { show_id: "A", show_name: "Zulu", seconds: 100, plays: 9 },
      { show_id: "B", show_name: "Alpha", seconds: 900, plays: 1 },
    ],
    [],
  );
  assert.deepEqual(applyList(rows, { sort: "time" }).map((r) => r.id), ["B", "A"]);
  assert.deepEqual(applyList(rows, { sort: "plays" }).map((r) => r.id), ["A", "B"]);
  assert.deepEqual(applyList(rows, { sort: "name" }).map((r) => r.id), ["B", "A"]);
});

test("recent sort puts the newest air date first", () => {
  const rows = mergeEpisodes(
    [
      { episode_id: 1, show_id: "S", show_name: "S", air_date: "2025-01-01", title: null, seconds: 900, plays: 1 },
      { episode_id: 2, show_id: "S", show_name: "S", air_date: "2026-05-05", title: null, seconds: 10, plays: 1 },
    ],
    [],
  );
  assert.deepEqual(applyList(rows, { sort: "recent" }).map((r) => r.id), [2, 1]);
});

test("recent sort reads real air dates, not month names alphabetically", () => {
  const stat = (id, airDate) => ({
    episode_id: id,
    show_id: "S",
    show_name: "S",
    air_date: airDate,
    title: null,
    seconds: 10,
    plays: 1,
  });
  const rows = mergeEpisodes(
    [stat(1, "April 2, 2020"), stat(2, "January 5, 2021"), stat(3, "December 9, 2019")],
    [],
  );
  assert.deepEqual(applyList(rows, { sort: "recent" }).map((r) => r.id), [2, 1, 3]);
});

test("applyList does not mutate the source order", () => {
  const rows = mergeShows(
    [
      { show_id: "A", show_name: "Zulu", seconds: 100, plays: 1 },
      { show_id: "B", show_name: "Alpha", seconds: 900, plays: 1 },
    ],
    [],
  );
  applyList(rows, { sort: "name" });
  assert.deepEqual(rows.map((r) => r.id), ["A", "B"]);
});

test("songs group by show or artist, biggest block first", () => {
  const track = (id, artist, showName) => ({
    track: {
      id,
      episode_id: id,
      seq: 0,
      artist,
      title: `t${id}`,
      album: null,
      label: null,
      comments: null,
      start_sec: 0,
      source_id: null,
      played_at: null,
      favourite: true,
    },
    show_id: showName,
    show_name: showName,
    air_date: "2026-01-01",
    added_at: id,
  });
  const rows = toTrackRows([
    track(1, "Sun Ra", "Wake N Bake"),
    track(2, "Sun Ra", "Mac's Place"),
    track(3, "Can", "Wake N Bake"),
  ]);
  assert.deepEqual(
    groupTracks(rows, "artist").map((g) => [g.label, g.items.length]),
    [
      ["Sun Ra", 2],
      ["Can", 1],
    ],
  );
  assert.deepEqual(
    groupTracks(rows, "show").map((g) => [g.label, g.items.length]),
    [
      ["Wake N Bake", 2],
      ["Mac's Place", 1],
    ],
  );
  assert.equal(groupTracks(rows, "none").length, 1);
  assert.equal(groupTracks(rows, "none")[0].items.length, 3);
});

test("a song with no artist still groups, under a named bucket", () => {
  const rows = toTrackRows([
    {
      track: {
        id: 1,
        episode_id: 1,
        seq: 0,
        artist: null,
        title: null,
        album: null,
        label: null,
        comments: null,
        start_sec: null,
        source_id: null,
        played_at: null,
        favourite: true,
      },
      show_id: "S",
      show_name: "Show",
      air_date: null,
      added_at: 1,
    },
  ]);
  assert.deepEqual(groupTracks(rows, "artist").map((g) => g.label), ["Unknown artist"]);
});

// -- Calendar ---------------------------------------------------------------

const AT = new Date(2026, 6, 15); // Wed 15 July 2026, local

test("a year row is 54 week columns of 7 cells, starting on a Monday", () => {
  const [g] = buildYearGrids([], [2026], AT);
  assert.equal(g.weeks.length, 54);
  assert.ok(g.weeks.every((w) => w.length === 7));
  // 1 Jan 2026 is a Thursday, so the row opens on Monday 29 December with three pads.
  assert.deepEqual(
    g.weeks[0].map((c) => c.day),
    ["", "", "", "2026-01-01", "2026-01-02", "2026-01-03", "2026-01-04"],
  );
});

test("the current year stops at today; a past year runs to 31 December", () => {
  const dated = (year) => buildYearGrids([], [year], AT)[0].weeks.flat().filter((c) => c.day);
  const thisYear = dated(2026);
  assert.equal(thisYear[thisYear.length - 1].day, "2026-07-15");
  const lastYear = dated(2025);
  assert.equal(lastYear[0].day, "2025-01-01");
  assert.equal(lastYear[lastYear.length - 1].day, "2025-12-31");
  assert.equal(lastYear.length, 365);
});

test("54 columns because 53 clips a leap year that opens on a Sunday", () => {
  // 1 Jan 2012 was a Sunday and 2012 was a leap year: six leading pads plus 366 days is
  // 372 cells, one more than 53 columns can hold.
  const dated = buildYearGrids([], [2012], AT)[0].weeks.flat().filter((c) => c.day);
  assert.equal(dated[0].day, "2012-01-01");
  assert.equal(dated[dated.length - 1].day, "2012-12-31");
  assert.equal(dated.length, 366);
});

test("silent days back-fill as zero and the busiest day is reported", () => {
  const [g] = buildYearGrids(
    [
      { day: "2026-07-14", seconds: 3600 },
      { day: "2026-07-15", seconds: 60 },
    ],
    [2026],
    AT,
  );
  const cells = g.weeks.flat();
  assert.equal(cells.find((c) => c.day === "2026-07-13").seconds, 0);
  assert.equal(g.activeDays, 2);
  assert.equal(g.seconds, 3660);
  assert.equal(g.busiest.day, "2026-07-14");
});

test("levels come from quartiles of active days, so one binge cannot flatten the rest", () => {
  const days = [
    { day: "2026-07-09", seconds: 60 },
    { day: "2026-07-10", seconds: 120 },
    { day: "2026-07-11", seconds: 240 },
    { day: "2026-07-12", seconds: 36000 },
  ];
  const cells = buildYearGrids(days, [2026], AT)[0].weeks.flat();
  const level = (d) => cells.find((c) => c.day === d).level;
  assert.equal(level("2026-07-13"), 0); // silent
  assert.ok(level("2026-07-09") < level("2026-07-12"));
  assert.equal(level("2026-07-12"), 4);
  assert.ok(level("2026-07-11") >= 2); // not squashed to the bottom by the binge
});

test("one colour scale is shared by every row, so the years stay comparable", () => {
  const days = [
    { day: "2025-03-01", seconds: 36000 },
    { day: "2025-03-02", seconds: 30000 },
    { day: "2025-03-03", seconds: 24000 },
    { day: "2026-01-05", seconds: 60 },
  ];
  const [thisYear, lastYear] = buildYearGrids(days, [2026, 2025], AT);
  const top = (g) => Math.max(...g.weeks.flat().map((c) => c.level));
  assert.equal(top(thisYear), 1); // a quiet year cannot reach the top step
  assert.ok(top(lastYear) >= 3);
});

test("days outside the requested years are simply absent from the grid", () => {
  const [g] = buildYearGrids([{ day: "2020-01-01", seconds: 999 }], [2026], AT);
  assert.equal(g.activeDays, 0);
  assert.equal(g.seconds, 0);
  assert.equal(g.busiest, null);
});

test("dayKey formats local dates with zero padding", () => {
  assert.equal(dayKey(new Date(2026, 0, 5)), "2026-01-05");
  assert.equal(dayKey(new Date(2026, 11, 31)), "2026-12-31");
});

// -- Streaks and month totals -----------------------------------------------

test("current streak counts back from today", () => {
  const days = [
    { day: "2026-07-13", seconds: 10 },
    { day: "2026-07-14", seconds: 10 },
    { day: "2026-07-15", seconds: 10 },
  ];
  assert.deepEqual(streaks(days, AT), { current: 3, longest: 3 });
});

test("a run reaching yesterday is still live; a two-day gap ends it", () => {
  const live = [
    { day: "2026-07-13", seconds: 10 },
    { day: "2026-07-14", seconds: 10 },
  ];
  assert.equal(streaks(live, AT).current, 2);
  const stale = [
    { day: "2026-07-01", seconds: 10 },
    { day: "2026-07-02", seconds: 10 },
  ];
  assert.deepEqual(streaks(stale, AT), { current: 0, longest: 2 });
});

test("longest streak survives gaps and zero-second days", () => {
  const days = [
    { day: "2026-06-01", seconds: 10 },
    { day: "2026-06-02", seconds: 10 },
    { day: "2026-06-03", seconds: 10 },
    { day: "2026-06-04", seconds: 0 }, // recorded but silent: breaks the run
    { day: "2026-06-05", seconds: 10 },
  ];
  assert.deepEqual(streaks(days, AT), { current: 0, longest: 3 });
});

test("no listening means no streak", () => {
  assert.deepEqual(streaks([], AT), { current: 0, longest: 0 });
  assert.deepEqual(streaks([{ day: "2026-07-15", seconds: 0 }], AT), { current: 0, longest: 0 });
});

test("month total covers only the calendar month holding today", () => {
  const days = [
    { day: "2026-06-30", seconds: 100 },
    { day: "2026-07-01", seconds: 200 },
    { day: "2026-07-15", seconds: 300 },
    { day: "2026-08-01", seconds: 400 },
  ];
  assert.equal(monthSeconds(days, AT), 500);
  assert.equal(monthSeconds([], AT), 0);
});

// -- Air dates and archive eras ---------------------------------------------

test("air dates parse from the scraped text form, and ISO passes through", () => {
  assert.equal(parseAirDate("July 9, 2026"), "2026-07-09");
  assert.equal(parseAirDate("September 30, 2010"), "2010-09-30");
  assert.equal(parseAirDate("  March 1, 1999  "), "1999-03-01");
  assert.equal(parseAirDate("2026-07-14"), "2026-07-14"); // live episodes already ISO
});

test("an unreadable air date is null, never a guess", () => {
  assert.equal(parseAirDate(null), null);
  assert.equal(parseAirDate(""), null);
  assert.equal(parseAirDate("sometime last spring"), null);
  assert.equal(parseAirDate("Smarch 4, 2011"), null); // not a month
  assert.equal(parseAirDate("July 44, 2026"), null); // not a day
  assert.equal(parseAirDate("July 9, 1626"), null); // before the station existed
});

test("era rows span every year between the first and the last, newest first", () => {
  const grid = buildEraGrid([
    { period: "2002-03", seconds: 600, plays: 2 },
    { period: "2004-03", seconds: 1800, plays: 3 },
    { period: "2004-11", seconds: 3600, plays: 5 },
  ]);
  assert.deepEqual(grid.rows.map((r) => r.year), [2004, 2003, 2002]);
  assert.ok(grid.rows.every((r) => r.months.length === 12));
  assert.equal(grid.rows[1].seconds, 0); // the skipped year stays as a visible gap
  assert.equal(grid.rows[0].months[10].seconds, 3600); // November
  assert.equal(grid.rows[0].plays, 8);
  assert.equal(grid.peak.year, 2004);
  assert.equal(grid.max, 5400);
  assert.equal(grid.total, 6000);
});

test("era months carry levels only where something was played", () => {
  const grid = buildEraGrid([
    { period: "2004-01", seconds: 60, plays: 1 },
    { period: "2004-06", seconds: 36000, plays: 9 },
  ]);
  const [january, february, june] = [0, 1, 5].map((m) => grid.rows[0].months[m]);
  assert.equal(february.level, 0);
  assert.ok(january.level > 0);
  assert.ok(june.level > january.level);
});

test("no archive dates means no rows and no peak", () => {
  assert.deepEqual(buildEraGrid([]), { rows: [], max: 0, peak: null, total: 0 });
  assert.equal(buildEraGrid([{ period: "junk", seconds: 10, plays: 1 }]).rows.length, 0);
});
