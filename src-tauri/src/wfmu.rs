use regex::Regex;
use scraper::{Html, Selector};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const BASE: &str = "https://wfmu.org";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 ArchiveBunker2/0.1";
const MIN_REQUEST_GAP: Duration = Duration::from_millis(1000);
const MIN_STATUS_REQUEST_GAP: Duration = Duration::from_millis(250);
const MIN_LIVE_PAGE_REQUEST_GAP: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LiveStationConfig {
    pub id: &'static str,
    pub name: &'static str,
    pub rethink_code: &'static str,
    pub info_url: &'static str,
}

pub fn live_station(id: &str) -> Option<LiveStationConfig> {
    let config = match id {
        "freeform" => LiveStationConfig {
            id: "freeform",
            name: "WFMU 91.1",
            rethink_code: "wfmu",
            info_url: "https://wfmu.org/",
        },
        "drummer" => LiveStationConfig {
            id: "drummer",
            name: "Give the Drummer Radio",
            rethink_code: "wfmugtd",
            info_url: "https://wfmu.org/drummer",
        },
        "rocknsoul" => LiveStationConfig {
            id: "rocknsoul",
            name: "Rock'n'Soul Ichiban",
            rethink_code: "wfmurnsi",
            info_url: "https://wfmu.org/rocknsoulradio",
        },
        "sheena" => LiveStationConfig {
            id: "sheena",
            name: "Sheena's Jungle Room",
            rethink_code: "wfmusjr",
            info_url: "https://wfmu.org/sheena",
        },
        _ => return None,
    };
    Some(config)
}

pub fn rethink_playlist_url(code: &str) -> String {
    format!("https://radiorethinkprod-default-rtdb.firebaseio.com/playlistData/{code}.json")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedRecentTrack {
    pub source_id: String,
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub played_at: i64,
    pub air_date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLiveProgram {
    pub show_id: Option<String>,
    pub name: String,
    pub host: Option<String>,
    pub description: Option<String>,
    pub starts_at: Option<String>,
    pub ends_at: Option<String>,
    pub day: Option<String>,
    pub current: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ParsedLivePage {
    pub tracks: Vec<ParsedRecentTrack>,
    pub current_show: Option<ParsedLiveProgram>,
    pub next_show: Option<ParsedLiveProgram>,
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ParsedShow {
    pub id: String,
    pub name: String,
    pub dj: Option<String>,
    pub on_air: bool,
}

#[derive(Debug, Clone)]
pub struct ParsedEpisode {
    pub id: i64,
    pub air_date: Option<String>,
    pub title: Option<String>,
    pub archive_id: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ParsedTrack {
    pub artist: Option<String>,
    pub title: Option<String>,
    pub album: Option<String>,
    pub label: Option<String>,
    pub comments: Option<String>,
    pub start_sec: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct ParsedPlaylistMeta {
    pub show_id: Option<String>,
    pub show_name: Option<String>,
    pub air_date: Option<String>,
    pub title: Option<String>,
}

/// Current state returned by a WFMU channel landing page. During a hosted show
/// `episode_id` points at the normal append-only playlist. Between shows WFMU only
/// publishes artist/title metadata, which the app records in a synthetic live episode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedLiveStatus {
    pub episode_id: Option<i64>,
    pub show_id: Option<String>,
    pub show_name: Option<String>,
    pub artist: Option<String>,
    pub title: Option<String>,
}

/// The small source used to refresh one live station. Channel sources call the
/// same JSON endpoint used by WFMU's own pages; the main FM stream keeps the
/// legacy homepage parser because it has no channel-landing feed.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LiveStatusSource {
    Channel { channel_id: i64 },
    Homepage,
}

impl LiveStatusSource {
    pub fn url(&self) -> String {
        match self {
            Self::Channel { channel_id } => {
                format!("{BASE}/channel_landing.php?cid={channel_id}&hashes%5Bnowplaying%5D=stale")
            }
            Self::Homepage => format!("{BASE}/"),
        }
    }

    pub fn parse(&self, body: &str) -> Option<ParsedLiveStatus> {
        match self {
            Self::Channel { .. } => parse_channel_landing(body),
            Self::Homepage => parse_homepage_live_status(body),
        }
    }
}

#[derive(Deserialize)]
struct ChannelLandingResponse {
    changes: Option<ChannelLandingChanges>,
}

#[derive(Deserialize)]
struct ChannelLandingChanges {
    nowplaying: Option<ChannelLandingSection>,
}

#[derive(Deserialize)]
struct ChannelLandingSection {
    content: String,
}

pub struct Fetcher {
    client: reqwest::Client,
    last_request: tokio::sync::Mutex<Option<Instant>>,
    last_status_request: tokio::sync::Mutex<Option<Instant>>,
    last_live_page_request: tokio::sync::Mutex<Option<Instant>>,
}

impl Fetcher {
    pub fn new() -> Self {
        Fetcher {
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .expect("http client"),
            last_request: tokio::sync::Mutex::new(None),
            last_status_request: tokio::sync::Mutex::new(None),
            last_live_page_request: tokio::sync::Mutex::new(None),
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Polite GET: global 1 req/s toward wfmu.org.
    pub async fn get_text(&self, url: &str) -> Result<String, String> {
        self.get_text_with_bucket(url, &self.last_request, MIN_REQUEST_GAP)
            .await
    }

    /// Live metadata is tiny and refreshed every five seconds. Give it an
    /// independent bucket so it never queues an AccuPlaylist/archive request.
    pub async fn get_status_text(&self, url: &str) -> Result<String, String> {
        self.get_text_with_bucket(url, &self.last_status_request, MIN_STATUS_REQUEST_GAP)
            .await
    }

    /// Recent-song and schedule hydration has its own bucket: a live detail page
    /// must not queue either the five-second player status or an archive scrape.
    pub async fn get_live_page_text(&self, url: &str) -> Result<String, String> {
        self.get_text_with_bucket(url, &self.last_live_page_request, MIN_LIVE_PAGE_REQUEST_GAP)
            .await
    }

    async fn get_text_with_bucket(
        &self,
        url: &str,
        bucket: &tokio::sync::Mutex<Option<Instant>>,
        minimum_gap: Duration,
    ) -> Result<String, String> {
        {
            let mut last = bucket.lock().await;
            if let Some(t) = *last {
                let elapsed = t.elapsed();
                if elapsed < minimum_gap {
                    tokio::time::sleep(minimum_gap - elapsed).await;
                }
            }
            *last = Some(Instant::now());
        }
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| format!("request failed: {e}"))?;
        if !resp.status().is_success() {
            return Err(format!("HTTP {} for {url}", resp.status()));
        }
        resp.text()
            .await
            .map_err(|e| format!("read body failed: {e}"))
    }
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#039;", "'")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&minus;", "-")
}

fn strip_tags(s: &str) -> String {
    static TAG: OnceLock<Regex> = OnceLock::new();
    let re = TAG.get_or_init(|| Regex::new(r"<[^>]*>").unwrap());
    re.replace_all(s, " ").into_owned()
}

fn clean_text(s: &str) -> String {
    // WFMU wraps some titles/fields in nested markup (e.g. <h2><font><b>…</b>); strip any
    // tags so they don't render as literal text, then decode entities and collapse space.
    let collapsed: String = decode_entities(&strip_tags(s))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    collapsed.trim().to_string()
}

fn non_empty(s: String) -> Option<String> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// Convert a WFMU short date ("MM.DD.YY", as it appears in the playlist-index link
/// text on deep-archive shows like Kenny G's — /playlists/KG) into the app's canonical
/// "Month D, YYYY" form so those episodes don't render as "unknown date".
fn mdy_from_dotted(mm: &str, dd: &str, yy: &str) -> Option<String> {
    const MONTHS: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let m: usize = mm.parse().ok()?;
    let d: u32 = dd.parse().ok()?;
    let y: i32 = yy.parse().ok()?;
    let name = MONTHS.get(m.checked_sub(1)?)?;
    // 2-digit years: pivot at 70 (WFMU archives run 1990s→present).
    let year = if y < 70 { 2000 + y } else { 1900 + y };
    Some(format!("{name} {d}, {year}"))
}

/// Parse the full catalog from https://wfmu.org/playlists/.
/// Shows appear multiple times (weekday schedule + alphabetical list);
/// entries seen more than once are treated as currently on air.
pub fn parse_catalog(html: &str) -> Vec<ParsedShow> {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    static NAME: OnceLock<Regex> = OnceLock::new();
    static DJ: OnceLock<Regex> = OnceLock::new();
    let split = SPLIT.get_or_init(|| Regex::new(r#"id="KDBprogram-([A-Za-z0-9]+)""#).unwrap());
    let name_re = NAME.get_or_init(|| Regex::new(r"(?s)<b>(.+?)</b>").unwrap());
    let dj_re = DJ.get_or_init(|| {
        Regex::new(r#"(?s)</b>\s*(?:with|w/)\s+(.+?)\s*(?:-\s*)?<a href="/playlists/"#).unwrap()
    });

    let mut found: std::collections::HashMap<String, (ParsedShow, u32)> =
        std::collections::HashMap::new();
    let mut order: Vec<String> = Vec::new();

    let matches: Vec<(String, usize)> = split
        .captures_iter(html)
        .map(|c| (c[1].to_string(), c.get(0).unwrap().end()))
        .collect();
    for (i, (id, start)) in matches.iter().enumerate() {
        let end = matches
            .get(i + 1)
            .map(|(_, s)| *s)
            .unwrap_or(html.len().min(start + 4000));
        let chunk = &html[*start..end.min(html.len())];
        let name = name_re
            .captures(chunk)
            .map(|c| clean_text(&c[1]))
            .filter(|n| !n.is_empty());
        let Some(name) = name else { continue };
        // Require a playlists link for this ID inside the chunk to reject stray spans.
        if !chunk.contains(&format!("/playlists/{id}")) {
            continue;
        }
        let dj = dj_re
            .captures(chunk)
            .map(|c| clean_text(&c[1]))
            .and_then(non_empty);
        match found.get_mut(id.as_str()) {
            Some((show, count)) => {
                *count += 1;
                if show.dj.is_none() {
                    show.dj = dj;
                }
            }
            None => {
                order.push(id.clone());
                found.insert(
                    id.clone(),
                    (
                        ParsedShow {
                            id: id.clone(),
                            name,
                            dj,
                            on_air: false,
                        },
                        1,
                    ),
                );
            }
        }
    }
    order
        .into_iter()
        .filter_map(|id| {
            found.remove(&id).map(|(mut show, count)| {
                show.on_air = count > 1;
                show
            })
        })
        .collect()
}

static FLASH_RE: OnceLock<Regex> = OnceLock::new();
static LISTEN_RE: OnceLock<Regex> = OnceLock::new();

fn flash_re() -> &'static Regex {
    FLASH_RE.get_or_init(|| {
        Regex::new(r"flashplayer\.php\?version=\d+&(?:amp;)?show=(\d+)&(?:amp;)?archive=(\d+)")
            .unwrap()
    })
}
fn listen_re() -> &'static Regex {
    LISTEN_RE.get_or_init(|| Regex::new(r"listen\.m3u\?show=(\d+)&(?:amp;)?archive=(\d+)").unwrap())
}

fn query_rows(value: &Value) -> Vec<std::collections::HashMap<String, Value>> {
    let columns = value
        .get("COLUMNS")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    value
        .get("DATA")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_array)
        .map(|values| {
            columns
                .iter()
                .filter_map(Value::as_str)
                .zip(values.iter().cloned())
                .map(|(key, value)| (key.to_ascii_lowercase(), value))
                .collect()
        })
        .collect()
}

fn json_string(value: Option<&Value>) -> Option<String> {
    value
        .and_then(|value| match value {
            Value::String(value) => Some(value.trim().to_string()),
            Value::Number(value) => Some(value.to_string()),
            _ => None,
        })
        .filter(|value| !value.is_empty())
}

fn parse_rethink_query(value: Option<&Value>) -> Option<Value> {
    match value? {
        Value::String(encoded) => serde_json::from_str(encoded).ok(),
        object @ Value::Object(_) => Some(object.clone()),
        _ => None,
    }
}

fn show_id_from_url(url: Option<String>) -> Option<String> {
    static SHOW_ID: OnceLock<Regex> = OnceLock::new();
    let regex =
        SHOW_ID.get_or_init(|| Regex::new(r"(?i)/playlists/([a-z0-9]+)(?:[/?#]|$)").unwrap());
    url.and_then(|url| regex.captures(&url).map(|capture| capture[1].to_string()))
}

fn parse_provider_program(value: Option<&Value>, current: bool) -> Option<ParsedLiveProgram> {
    let query = parse_rethink_query(value)?;
    let row = query_rows(&query).into_iter().next()?;
    let name = json_string(row.get("showname"))?;
    Some(ParsedLiveProgram {
        show_id: show_id_from_url(json_string(row.get("showurl"))),
        name,
        host: json_string(row.get("showhost")),
        description: json_string(row.get("showdescription")),
        starts_at: json_string(row.get("showstarttime")).map(normalize_provider_time),
        ends_at: json_string(row.get("showendtime")).map(normalize_provider_time),
        day: json_string(row.get("day")),
        current,
    })
}

fn normalize_provider_time(value: String) -> String {
    chrono::DateTime::parse_from_str(&value, "%m-%d-%Y %H:%M:%S %z")
        .map(|date| date.to_rfc3339())
        .unwrap_or(value)
}

/// Parse Radio Rethink's public Firebase station snapshot. The ColdFusion query
/// values are JSON strings inside the outer JSON document, so both layers are
/// deliberately decoded here rather than relying on the tuner's rendered HTML.
pub fn parse_rethink_live_page(body: &str) -> Result<ParsedLivePage, String> {
    let root: Value =
        serde_json::from_str(body).map_err(|error| format!("invalid live history: {error}"))?;
    let playlist = parse_rethink_query(root.get("playListDataJson"));
    let mut tracks = Vec::new();
    if let Some(playlist) = playlist {
        for row in query_rows(&playlist) {
            let timestamp = json_string(row.get("trackplayedatstationtimezone"))
                .or_else(|| json_string(row.get("tracktimestamp")));
            let Some(timestamp) = timestamp else { continue };
            let Ok(played) = chrono::DateTime::parse_from_str(&timestamp, "%m-%d-%Y %H:%M:%S %z")
            else {
                continue;
            };
            let source_id = json_string(row.get("id"))
                .unwrap_or_else(|| format!("{}:{}", played.timestamp(), tracks.len()));
            let mut artist = json_string(row.get("artist"));
            let mut title = json_string(row.get("track"));
            if artist.is_none() {
                if let Some(combined) = title.clone() {
                    if let Some((left, right)) = combined.split_once(" - ") {
                        artist = Some(left.trim().to_string()).filter(|value| !value.is_empty());
                        title = Some(right.trim().to_string()).filter(|value| !value.is_empty());
                    }
                }
            }
            tracks.push(ParsedRecentTrack {
                source_id,
                artist,
                title,
                album: json_string(row.get("album")),
                played_at: played.timestamp(),
                air_date: played.format("%Y-%m-%d").to_string(),
            });
        }
    }
    tracks.sort_by_key(|track| track.played_at);
    if tracks.len() > 20 {
        tracks.drain(..tracks.len() - 20);
    }
    let updated_at = tracks.last().map(|track| track.played_at);
    Ok(ParsedLivePage {
        tracks,
        current_show: parse_provider_program(root.get("onNowJSON"), true),
        next_show: parse_provider_program(root.get("onNextJSON"), false),
        updated_at,
    })
}

/// Parse the compact weekly schedule embedded on WFMU's channel pages. It is
/// structured as day-heading rows followed by show rows; callers filter the
/// result to the current Eastern day and merge it with Radio Rethink's exact
/// current/next timestamps.
pub fn parse_live_schedule(html: &str) -> Vec<ParsedLiveProgram> {
    let document = Html::parse_document(html);
    let row_selector = Selector::parse("#section_upcoming tr").unwrap();
    let day_selector = Selector::parse(".upcoming_dow").unwrap();
    let cell_selector = Selector::parse("td").unwrap();
    let link_selector = Selector::parse("a[href*='/playlists/']").unwrap();
    let description_selector = Selector::parse("span[id^='expander_target_'] i").unwrap();
    let mut day: Option<String> = None;
    let mut programs = Vec::new();
    for row in document.select(&row_selector) {
        if let Some(heading) = row.select(&day_selector).next() {
            day = Some(clean_text(&heading.inner_html()));
            continue;
        }
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() < 2 {
            continue;
        }
        let time_label = clean_text(&cells[0].inner_html());
        let link = cells[1].select(&link_selector).find(|link| {
            link.value()
                .attr("href")
                .map(|href| !href.contains("/playlists/shows/"))
                .unwrap_or(false)
        });
        let Some(link) = link else { continue };
        let href = link.value().attr("href").map(str::to_string);
        let name = clean_text(&link.inner_html());
        if name.is_empty() {
            continue;
        }
        let description = cells[1]
            .select(&description_selector)
            .next()
            .map(|node| clean_text(&node.inner_html()))
            .filter(|value| !value.is_empty());
        let class = row.value().attr("class").unwrap_or_default();
        programs.push(ParsedLiveProgram {
            show_id: show_id_from_url(href),
            name,
            host: None,
            description,
            starts_at: Some(time_label).filter(|value| !value.is_empty()),
            ends_at: None,
            day: day.clone(),
            current: class
                .split_whitespace()
                .any(|value| value == "upcoming_current_slot"),
        });
    }
    if !programs.is_empty() {
        return programs;
    }

    // The main FM homepage predates the channel template but exposes the same
    // information in #playingtoday. Its highlighted gold row is the current show.
    let row_selector = Selector::parse("#playingtoday tr").unwrap();
    let bold_selector = Selector::parse("b").unwrap();
    let mut day: Option<String> = None;
    for row in document.select(&row_selector) {
        let cells = row.select(&cell_selector).collect::<Vec<_>>();
        if cells.len() == 1 && cells[0].value().attr("colspan").is_some() {
            day = cells[0]
                .select(&bold_selector)
                .next()
                .map(|node| clean_text(&node.inner_html()));
            continue;
        }
        if cells.len() < 2 {
            continue;
        }
        let link = cells[1].select(&link_selector).find(|link| {
            link.value()
                .attr("href")
                .map(|href| !href.contains("/playlists/shows/"))
                .unwrap_or(false)
        });
        let Some(link) = link else { continue };
        let name = clean_text(&link.inner_html());
        let style = row
            .value()
            .attr("style")
            .unwrap_or_default()
            .to_ascii_lowercase();
        programs.push(ParsedLiveProgram {
            show_id: show_id_from_url(link.value().attr("href").map(str::to_string)),
            name,
            host: None,
            description: None,
            starts_at: Some(clean_text(&cells[0].inner_html())),
            ends_at: None,
            day: day.clone(),
            current: style.contains("#dfbe3f") || style.contains("rgb(223, 190, 63)"),
        });
    }
    programs
}

/// The archive id that the /archiveplayer/ resolver expects is the one from the
/// flashplayer ("Pop-up player") link, which is present for every playable episode
/// on both current and deep-archive shows. The listen.m3u link only appears on some
/// recent episodes and uses a *different* archive id, so it is only a fallback.
fn archive_for(chunk: &str, episode_id: i64) -> Option<i64> {
    let flash = flash_re()
        .captures_iter(chunk)
        .find(|c| c[1].parse::<i64>().ok() == Some(episode_id))
        .and_then(|c| c[2].parse::<i64>().ok());
    flash.or_else(|| {
        listen_re()
            .captures_iter(chunk)
            .find(|c| c[1].parse::<i64>().ok() == Some(episode_id))
            .and_then(|c| c[2].parse::<i64>().ok())
    })
}

/// Parse a show page (https://wfmu.org/playlists/{ID}) into its episode list.
pub fn parse_show_page(html: &str) -> Vec<ParsedEpisode> {
    static SPLIT: OnceLock<Regex> = OnceLock::new();
    static DATE: OnceLock<Regex> = OnceLock::new();
    static ALT_DATE: OnceLock<Regex> = OnceLock::new();
    static TITLE: OnceLock<Regex> = OnceLock::new();
    let split = SPLIT.get_or_init(|| Regex::new(r#"id="KDBepisode-(\d+)""#).unwrap());
    let date_re = DATE.get_or_init(|| Regex::new(r"([A-Z][a-z]+ \d{1,2}, \d{4})\s*:").unwrap());
    // Deep-archive templates (e.g. /playlists/KG and its legacy per-year pages): the date
    // is rendered as "MM.DD.YY" rather than the "Month DD, YYYY:" heading — either as a
    // "MM.DD.YY playlist" link, or (for episodes with no archived playlist) as bare text
    // followed by "| Listen". Anchor on a trailing "playlist" or "|" so we skip the date
    // baked into legacy hrefs (…/MM.DD.YY.html) and only read the displayed date. The month
    // is validated in mdy_from_dotted, so stray dotted numbers are rejected.
    let alt_date_re = ALT_DATE
        .get_or_init(|| Regex::new(r"(\d{1,2})\.(\d{1,2})\.(\d{2})\s*(?:playlist|\|)").unwrap());
    let title_re = TITLE.get_or_init(|| Regex::new(r"(?s)<b>(.*?)</b>").unwrap());

    let matches: Vec<(i64, usize)> = split
        .captures_iter(html)
        .filter_map(|c| {
            c[1].parse::<i64>()
                .ok()
                .map(|id| (id, c.get(0).unwrap().end()))
        })
        .collect();
    let mut episodes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for (i, (id, start)) in matches.iter().enumerate() {
        if !seen.insert(*id) {
            continue;
        }
        let end = matches
            .get(i + 1)
            .map(|(_, s)| *s)
            .unwrap_or(html.len().min(start + 3000));
        let chunk = &html[*start..end.min(html.len())];
        let air_date = date_re
            .captures(chunk)
            .map(|c| clean_text(&c[1]))
            .or_else(|| {
                alt_date_re
                    .captures(chunk)
                    .and_then(|c| mdy_from_dotted(&c[1], &c[2], &c[3]))
            });
        let title = title_re
            .captures(chunk)
            .map(|c| clean_text(&c[1]))
            .and_then(non_empty);
        let archive_id = archive_for(chunk, *id);
        episodes.push(ParsedEpisode {
            id: *id,
            air_date,
            title,
            archive_id,
        });
    }
    episodes
}

/// Find the playlist belonging to the currently scheduled programme on a station
/// page. The page also contains older/recent playlist links, so start after the
/// "Playing Today" heading when it is present.
pub fn parse_current_playlist(html: &str) -> Option<i64> {
    static MARKER: OnceLock<Regex> = OnceLock::new();
    static LINK: OnceLock<Regex> = OnceLock::new();
    let marker = MARKER.get_or_init(|| Regex::new(r"(?is)playing\s+today").unwrap());
    let link = LINK.get_or_init(|| Regex::new(r#"/playlists/shows/(\d+)"#).unwrap());
    let start = marker.find(html).map(|m| m.end()).unwrap_or(0);
    link.captures(&html[start..])
        .and_then(|c| c[1].parse::<i64>().ok())
        .or_else(|| link.captures(html).and_then(|c| c[1].parse::<i64>().ok()))
}

/// Extract WFMU's channel id from the landing-page polling configuration.
pub fn parse_channel_id(html: &str) -> Option<i64> {
    static CID: OnceLock<Regex> = OnceLock::new();
    let cid = CID.get_or_init(|| Regex::new(r#"[\"']cid[\"']\s*:\s*(\d+)"#).unwrap());
    cid.captures(html).and_then(|c| c[1].parse().ok())
}

/// Build the small, now-playing-only request used by WFMU's own channel page.
/// A deliberately stale hash forces the endpoint to include the current section.
pub fn channel_nowplaying_url(page_url: &str, channel_id: i64) -> Option<String> {
    let mut parts = page_url.split('/');
    // `split('/')` yields the scheme with its trailing colon ("https:"), so drop it.
    let scheme = parts.next()?.strip_suffix(':')?;
    let empty = parts.next()?;
    let host = parts.next()?;
    if !empty.is_empty() || !matches!(scheme, "http" | "https") || host.is_empty() {
        return None;
    }
    Some(format!(
        "{scheme}://{host}/channel_landing.php?cid={channel_id}&hashes%5Bnowplaying%5D=stale"
    ))
}

/// Parse the JSON response from `channel_landing.php`.
pub fn parse_channel_landing(body: &str) -> Option<ParsedLiveStatus> {
    let response: ChannelLandingResponse = serde_json::from_str(body).ok()?;
    let html = response.changes?.nowplaying?.content;
    parse_live_nowplaying(&html)
}

/// Parse the now-playing fragment embedded in a channel page or polling response.
pub fn parse_live_nowplaying(html: &str) -> Option<ParsedLiveStatus> {
    static EPISODE: OnceLock<Regex> = OnceLock::new();
    static SHOW: OnceLock<Regex> = OnceLock::new();
    let episode = EPISODE.get_or_init(|| Regex::new(r#"/playlists/shows/(\d+)"#).unwrap());
    let show = SHOW.get_or_init(|| Regex::new(r#"/playlists/([A-Za-z0-9_-]+)"#).unwrap());

    let doc = Html::parse_fragment(html);
    let title_sel = Selector::parse(".nowplaying_program_title").unwrap();
    let live_sel = Selector::parse(".nowplaying_live_song").unwrap();
    let link_sel = Selector::parse("a").unwrap();

    let episode_id = episode
        .captures(html)
        .and_then(|c| c[1].parse::<i64>().ok());
    let live = doc.select(&live_sel).next();
    let show_id = live
        .and_then(|el| el.select(&link_sel).find_map(|a| a.value().attr("href")))
        .and_then(|href| show.captures(href))
        .map(|c| c[1].to_string());
    let show_name = live
        .map(|el| clean_text(&el.text().collect::<String>()))
        .and_then(|s| non_empty(s.trim_start_matches("on ").trim().to_string()));
    let raw_track = doc
        .select(&title_sel)
        .next()
        .map(|el| clean_text(&el.text().collect::<String>()))
        .and_then(non_empty);
    let (artist, title) = raw_track
        .as_deref()
        .map(parse_nowplaying_track)
        .unwrap_or((None, None));

    if episode_id.is_none() && artist.is_none() && title.is_none() {
        return None;
    }
    Some(ParsedLiveStatus {
        episode_id,
        show_id,
        show_name,
        artist,
        title,
    })
}

/// Parse the highlighted "Playing Today" row on the WFMU homepage. Unlike the
/// web-only channels, the main station exposes show/episode context but no
/// separate lightweight current-song JSON feed.
pub fn parse_homepage_live_status(html: &str) -> Option<ParsedLiveStatus> {
    static EPISODE: OnceLock<Regex> = OnceLock::new();
    let episode = EPISODE.get_or_init(|| Regex::new(r#"/playlists/shows/(\d+)"#).unwrap());
    let document = Html::parse_document(html);
    let playing_rows = Selector::parse("#playingtoday tr").unwrap();
    let all_rows = Selector::parse("tr").unwrap();
    let links = Selector::parse("a").unwrap();
    let mut rows = document.select(&playing_rows).collect::<Vec<_>>();
    if rows.is_empty() {
        rows = document.select(&all_rows).collect();
    }
    let row = rows.into_iter().find(|row| episode.is_match(&row.html()))?;
    let row_html = row.html();
    let episode_id = episode.captures(&row_html)?.get(1)?.as_str().parse().ok()?;
    let show_link = row.select(&links).find(|link| {
        link.value()
            .attr("href")
            .map(|href| href.starts_with("/playlists/") && !href.starts_with("/playlists/shows/"))
            .unwrap_or(false)
    });
    Some(ParsedLiveStatus {
        episode_id: Some(episode_id),
        show_id: show_link
            .as_ref()
            .and_then(|link| link.value().attr("href"))
            .and_then(|href| href.strip_prefix("/playlists/"))
            .map(str::to_string),
        show_name: show_link
            .map(|link| clean_text(&link.text().collect::<String>()))
            .and_then(non_empty),
        artist: None,
        title: None,
    })
}

fn parse_nowplaying_track(raw: &str) -> (Option<String>, Option<String>) {
    let value = raw.trim();
    // Hosted shows use: `"Track" by Artist`.
    if let Some(quoted) = value.strip_prefix('"') {
        if let Some(end) = quoted.find('"') {
            let title = quoted[..end].trim();
            let rest = quoted[end + 1..].trim();
            if let Some(artist) = rest.strip_prefix("by ") {
                return (
                    non_empty(artist.trim().to_string()),
                    non_empty(title.to_string()),
                );
            }
            // Unattended streams use: `"Artist - Track"`.
            if let Some((artist, title)) = title.split_once(" - ") {
                return (
                    non_empty(artist.trim().to_string()),
                    non_empty(title.trim().to_string()),
                );
            }
            return (None, non_empty(title.to_string()));
        }
    }
    if let Some((artist, title)) = value.split_once(" - ") {
        return (
            non_empty(artist.trim().to_string()),
            non_empty(title.trim().to_string()),
        );
    }
    (None, non_empty(value.trim_matches('"').to_string()))
}

/// Extract the normal show/episode context from a playlist page. This lets live
/// playlists use the same database rows and favourite actions as archive shows.
pub fn parse_playlist_meta(html: &str) -> ParsedPlaylistMeta {
    static FEED: OnceLock<Regex> = OnceLock::new();
    static OG_TITLE: OnceLock<Regex> = OnceLock::new();
    static OG_DESC: OnceLock<Regex> = OnceLock::new();
    let feed = FEED.get_or_init(|| Regex::new(r#"playlistfeed/([A-Za-z0-9_-]+)\.xml"#).unwrap());
    let og_title = OG_TITLE
        .get_or_init(|| Regex::new(r#"<meta\s+property="og:title"\s+content="([^"]+)""#).unwrap());
    let og_desc = OG_DESC.get_or_init(|| {
        Regex::new(r#"<meta\s+property="og:description"\s+content="([^"]+)""#).unwrap()
    });

    let show_id = feed.captures(html).map(|c| c[1].to_string());
    let raw_title = og_title.captures(html).map(|c| clean_text(&c[1]));
    let show_name = raw_title
        .as_deref()
        .and_then(|title| title.rsplit_once(": ").map(|(_, show)| show.to_string()));
    let title = raw_title.as_deref().and_then(|full| {
        show_name
            .as_deref()
            .and_then(|show| full.strip_suffix(&format!(": {show}")))
            .map(str::to_string)
            .or_else(|| non_empty(full.to_string()))
    });
    let air_date = og_desc.captures(html).and_then(|c| {
        Regex::new(r"(?i)(?:from|on)\s+([A-Z][a-z]+\s+\d{1,2},\s+\d{4})")
            .ok()
            .and_then(|date| date.captures(&c[1]).map(|m| m[1].to_string()))
    });
    ParsedPlaylistMeta {
        show_id,
        show_name,
        air_date,
        title,
    }
}

/// Extract the show's blurb from its show page. It lives in
/// `<div class="everything">…` and is followed by boilerplate ("On WFMU | …FM…",
/// "(Visit homepage.)") and newsletter links — trim those off.
pub fn parse_show_description(html: &str) -> Option<String> {
    const ANCHOR: &str = r#"<div class="everything">"#;
    let div_start = html.find(ANCHOR)? + ANCHOR.len();
    let region = &html[div_start..];
    // The blurb text is wrapped in the template's <font size="-1"> element. Anchoring on
    // it skips any leading centered image paragraph (e.g. show AU) that comes first.
    let text_start = region
        .find("<font")
        .and_then(|i| region[i..].find('>').map(|j| i + j + 1))
        .unwrap_or(0);
    let rest = &region[text_start..];
    // All cut markers are ASCII, so the byte index is a valid slice boundary.
    let mut cut = rest.len();
    for m in [
        "</font>",
        "<br",
        "On WFMU",
        "( Visit",
        "(Visit",
        "Visit&nbsp;homepage",
        "</p>",
        "</div>",
    ] {
        if let Some(i) = rest.find(m) {
            cut = cut.min(i);
        }
    }
    let text = clean_text(&rest[..cut]);
    // Drop trailing separators left where the boilerplate was cut (e.g. "… &", "… |").
    let text = text
        .trim_end_matches(['|', '(', '&', '-', ' '])
        .trim()
        .to_string();
    non_empty(text)
}

/// Slice out the <SELECT NAME="Ubu_Nav_Popup"> ("OLDER PLAYLISTS") block, if present.
fn nav_popup_block(html: &str) -> Option<&str> {
    let sel_start = html.find("Ubu_Nav_Popup")?;
    // Back up to the opening <select / <SELECT tag.
    let block_start = html[..sel_start]
        .rfind("<select")
        .or_else(|| html[..sel_start].rfind("<SELECT"))?;
    let block = &html[block_start..];
    let block_end = block.find("</select>").or_else(|| block.find("</SELECT>"))?;
    Some(&block[..block_end])
}

/// Discover archive-year sub-pages for a show. Two markup shapes exist in the wild:
/// an "OLDER PLAYLISTS" dropdown (/playlists/KG links to /playlists/KG2009 that way)
/// and a plain run of links (/playlists/BK lists "2001 playlists" … as anchors).
/// Both are scanned, so a page carrying both is still covered.
/// Returns the URL path portions (e.g. "/playlists/KG2009"), newest year first.
pub fn parse_show_archive_years(html: &str, show_id: &str) -> Vec<String> {
    // Both patterns tolerate any attribute order, single or double quotes, an
    // optional absolute origin, and any casing (WFMU emits <OPTION VALUE=…> and
    // /Playlists/ on some pages).
    static OPTION_RE: OnceLock<Regex> = OnceLock::new();
    let option_re = OPTION_RE.get_or_init(|| {
        Regex::new(
            r#"(?i)<option[^>]*\svalue=["'](?:https?://(?:www\.)?wfmu\.org)?(/playlists/[a-z0-9_-]+)["']"#,
        )
        .unwrap()
    });
    static ANCHOR_RE: OnceLock<Regex> = OnceLock::new();
    let anchor_re = ANCHOR_RE.get_or_init(|| {
        Regex::new(
            r#"(?i)<a[^>]*\shref=["'](?:https?://(?:www\.)?wfmu\.org)?(/playlists/[a-z0-9_-]+)["']"#,
        )
        .unwrap()
    });

    // Expected shape: /playlists/{show_id} followed by at least one digit. The
    // digits-only rule keeps legacy paths like /~kennyg/playlists/00.html out, and
    // requiring a non-empty suffix keeps the show's own page out (year pages link
    // back to it).
    let prefix = format!("/playlists/{show_id}");
    let lower_prefix = prefix.to_ascii_lowercase();
    let keep = |path: &str| -> Option<String> {
        let suffix = path.to_ascii_lowercase();
        let suffix = suffix.strip_prefix(&lower_prefix)?.to_string();
        if suffix.is_empty() || !suffix.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        // Re-emit with the caller's show-id casing so paths dedupe cleanly.
        Some(format!("{prefix}{suffix}"))
    };

    let from_dropdown = nav_popup_block(html)
        .into_iter()
        .flat_map(|block| option_re.captures_iter(block).map(|c| c[1].to_string()))
        .collect::<Vec<_>>();
    let from_anchors = anchor_re.captures_iter(html).map(|c| c[1].to_string());

    let mut years: Vec<String> = from_dropdown
        .into_iter()
        .chain(from_anchors)
        .filter_map(|path| keep(&path))
        .collect();
    years.sort();
    years.dedup();
    // Newest first: episodes are stored in `seq` order and the UI reads that as
    // newest → oldest.
    years.reverse();
    years
}

/// Extract the archive id from a single playlist page (its "Listen to this show" /
/// pop-up player link). Used to discover audio for episodes whose show-index block
/// carried no archive link.
pub fn parse_playlist_archive(html: &str) -> Option<i64> {
    flash_re()
        .captures(html)
        .and_then(|c| c[2].parse::<i64>().ok())
        .or_else(|| {
            listen_re()
                .captures(html)
                .and_then(|c| c[2].parse::<i64>().ok())
        })
}

/// Extract the direct audio URL from an AccuPlayer page
/// (https://wfmu.org/archiveplayer/?show={ep}&archive={arch}).
/// Works for both storage backends (mp3archives.wfmu.org and s3.amazonaws.com/arch.wfmu.org).
pub fn parse_archiveplayer(html: &str) -> Option<String> {
    static AUDIO: OnceLock<Regex> = OnceLock::new();
    static ANY: OnceLock<Regex> = OnceLock::new();
    let audio = AUDIO.get_or_init(|| Regex::new(r#"<audio[^>]*\bsrc="([^"]+)""#).unwrap());
    let any =
        ANY.get_or_init(|| Regex::new(r#"src="(https://[^"]+\.(?:mp3|mp4|m4a|aac))""#).unwrap());
    audio
        .captures(html)
        .map(|c| decode_entities(&c[1]))
        .filter(|u| u.starts_with("http"))
        .or_else(|| any.captures(html).map(|c| decode_entities(&c[1])))
}

/// Extract the archive pre-roll offset (seconds) from an AccuPlayer page's
/// `<body data-offset="…">`. This is the lead-in (prior-show tail + station IDs +
/// audition jingle) sitting before the show's playlist timeline; WFMU maps a
/// playlist timestamp to the audio file as `audio_time = start_sec + offset`.
pub fn parse_archiveplayer_offset(html: &str) -> Option<i64> {
    static OFFSET: OnceLock<Regex> = OnceLock::new();
    let re = OFFSET.get_or_init(|| Regex::new(r#"data-offset="(\d+)""#).unwrap());
    re.captures(html).and_then(|c| c[1].parse().ok())
}

/// "0:05:56" or "1:02:03" -> seconds. Also accepts "5:56".
pub fn parse_hms(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.trim().split(':').collect();
    let nums: Option<Vec<i64>> = parts.iter().map(|p| p.trim().parse::<i64>().ok()).collect();
    let nums = nums?;
    match nums.as_slice() {
        [h, m, s] => Some(h * 3600 + m * 60 + s),
        [m, s] => Some(m * 60 + s),
        _ => None,
    }
}

/// Parse a playlist page (https://wfmu.org/playlists/shows/{epId}) into tracks.
/// Collect a table cell's text while skipping WFMU's `KDBFavIcon` helper spans.
/// Those spans hold the song-star, jump/comment buttons and a hidden
/// `..._summary_html` block ("Title" by "Artist"); their text would otherwise
/// leak into the song-title cell.
fn cell_text_without_helpers(td: scraper::ElementRef) -> String {
    let mut out = String::new();
    for node in td.descendants() {
        let scraper::Node::Text(text) = node.value() else {
            continue;
        };
        let in_helper = node.ancestors().any(|a| {
            a.value()
                .as_element()
                .is_some_and(|el| el.classes().any(|c| c == "KDBFavIcon"))
        });
        if in_helper {
            continue;
        }
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(text);
    }
    out
}

pub fn parse_playlist(html: &str) -> Vec<ParsedTrack> {
    static TIME: OnceLock<Regex> = OnceLock::new();
    let time_re = TIME.get_or_init(|| Regex::new(r"(\d+:\d{1,2}:\d{2})").unwrap());

    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tr").unwrap();
    let cell = |class: &str| Selector::parse(&format!("td.{class}")).unwrap();
    let artist_sel = cell("col_artist");
    let title_sel = cell("col_song_title");
    let album_sel = cell("col_album_title");
    let label_sel = cell("col_record_label");
    let comments_sel = cell("col_comments");
    let time_sel = cell("col_live_timestamps_flag");

    let cell_text = |row: scraper::ElementRef, sel: &Selector| -> Option<String> {
        let td = row.select(sel).next()?;
        non_empty(clean_text(&cell_text_without_helpers(td)))
    };

    let mut tracks = Vec::new();
    for row in doc.select(&row_sel) {
        if row.select(&artist_sel).next().is_none() && row.select(&title_sel).next().is_none() {
            continue;
        }
        let artist = cell_text(row, &artist_sel);
        let title = cell_text(row, &title_sel);
        let album = cell_text(row, &album_sel);
        let label = cell_text(row, &label_sel);
        let comments = cell_text(row, &comments_sel);
        let start_sec = row
            .select(&time_sel)
            .next()
            .map(|td| td.text().collect::<String>())
            .and_then(|t| time_re.captures(&t).map(|c| c[1].to_string()))
            .and_then(|hms| parse_hms(&hms));
        if artist.is_none() && title.is_none() {
            continue;
        }
        tracks.push(ParsedTrack {
            artist,
            title,
            album,
            label,
            comments,
            start_sec,
        });
    }
    // Deep-archive DJs (e.g. Kenny G's Hour of Pain, /playlists/KG) use a free-form
    // "comment" template with no col_* table, so the tabular pass finds nothing.
    if tracks.is_empty() {
        return parse_playlist_freeform(html);
    }
    tracks
}

/// Fallback parser for the free-form "comment" playlist template. Each track is a red
/// ">" marker followed by "Artist | Title" text, a KDBsong star span, and an optional
/// "| H:MM:SS" approximate start time (also carried in the pop-up link's `starttime`).
/// Tracks are separated by `<br><br>`.
fn parse_playlist_freeform(html: &str) -> Vec<ParsedTrack> {
    static SONG: OnceLock<Regex> = OnceLock::new();
    static START: OnceLock<Regex> = OnceLock::new();
    let song_span =
        SONG.get_or_init(|| Regex::new(r#"<span[^>]*class="KDBFavIcon KDBsong""#).unwrap());
    let start_re = START.get_or_init(|| Regex::new(r"starttime=(\d+:\d{1,2}:\d{2})").unwrap());

    let mut tracks = Vec::new();
    for segment in html.split("<br><br>") {
        // Only segments carrying a song star are real tracks.
        let Some(m) = song_span.find(segment) else {
            continue;
        };
        // Text before the star holds ">Artist | Title".
        let head = clean_text(&segment[..m.start()]);
        let head = head.trim_start_matches('>').trim();
        if head.is_empty() {
            continue;
        }
        let (artist, title) = match head.split_once('|') {
            Some((a, t)) => (
                non_empty(a.trim().to_string()),
                non_empty(t.trim().to_string()),
            ),
            None => (non_empty(head.to_string()), None),
        };
        if artist.is_none() && title.is_none() {
            continue;
        }
        let start_sec = start_re.captures(segment).and_then(|c| parse_hms(&c[1]));
        tracks.push(ParsedTrack {
            artist,
            title,
            album: None,
            label: None,
            comments: None,
            start_sec,
        });
    }
    tracks
}

/// Resolve the direct MP3 URL from the listen.m3u endpoint.
pub fn parse_m3u(body: &str) -> Option<String> {
    body.lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_string)
}

pub fn catalog_url() -> String {
    format!("{BASE}/playlists/")
}

pub fn show_url(show_id: &str) -> String {
    format!("{BASE}/playlists/{show_id}")
}

pub fn playlist_url(episode_id: i64) -> String {
    format!("{BASE}/playlists/shows/{episode_id}")
}

pub fn m3u_url(episode_id: i64, archive_id: i64) -> String {
    format!("{BASE}/listen.m3u?show={episode_id}&archive={archive_id}")
}

pub fn archiveplayer_url(episode_id: i64, archive_id: i64) -> String {
    format!("{BASE}/archiveplayer/?show={episode_id}&archive={archive_id}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(path).expect("fixture readable")
    }

    #[test]
    fn rethink_snapshot_parses_tracks_and_show_context() {
        let playlist = serde_json::json!({
            "COLUMNS": ["id", "artist", "track", "album", "trackPlayedAtStationTimezone"],
            "DATA": [
                ["new", "", "The New Band - Last Song", "Newest", "07-14-2026 12:34:51 -0400"],
                ["old", "An Artist", "First Song", "Older", "07-14-2026 12:30:00 -0400"]
            ]
        });
        let now = serde_json::json!({
            "COLUMNS": ["showName", "showHost", "showDescription", "showURL", "day", "showStartTime", "showEndTime"],
            "DATA": [["Tony Coulter", "Tony", "A freeform show", "http://www.wfmu.org/playlists/TC", "Tuesday", "07-14-2026 12:00:00 -0400", "07-14-2026 15:00:00 -0400"]]
        });
        let next = serde_json::json!({
            "COLUMNS": ["showName", "showURL", "day", "showStartTime", "showEndTime"],
            "DATA": [["Dark Night", "https://wfmu.org/playlists/JQ", "Tuesday", "07-14-2026 15:00:00 -0400", "07-14-2026 19:00:00 -0400"]]
        });
        let body = serde_json::json!({
            "playListDataJson": playlist.to_string(),
            "onNowJSON": now.to_string(),
            "onNextJSON": next.to_string()
        })
        .to_string();
        let parsed = parse_rethink_live_page(&body).unwrap();
        assert_eq!(parsed.tracks.len(), 2);
        assert_eq!(parsed.tracks[0].source_id, "old");
        assert_eq!(parsed.tracks[1].artist.as_deref(), Some("The New Band"));
        assert_eq!(parsed.tracks[1].title.as_deref(), Some("Last Song"));
        assert_eq!(
            parsed.current_show.as_ref().unwrap().show_id.as_deref(),
            Some("TC")
        );
        assert_eq!(
            parsed.next_show.as_ref().unwrap().show_id.as_deref(),
            Some("JQ")
        );
        assert!(parsed
            .current_show
            .unwrap()
            .starts_at
            .unwrap()
            .contains('T'));
    }

    #[test]
    fn channel_schedule_preserves_day_links_and_descriptions() {
        let html = r#"<div id="section_upcoming"><table>
          <tr><td><div class="upcoming_dow">Tuesday</div></td></tr>
          <tr class="upcoming_current_slot"><td>12-3pm</td><td>
            <a href="/playlists/TC">Tony Coulter</a>
            <span id="expander_target_1"><i>Rhino and psychedelic fork.</i></span>
            <a href="/playlists/shows/166313">Live playlist</a></td></tr>
          <tr class="upcoming_even_slot"><td>3-7pm</td><td>
            <a href="/playlists/JQ">Dark Night of the Soul</a></td></tr>
          <tr><td><div class="upcoming_dow">Wednesday</div></td></tr>
        </table></div>"#;
        let programs = parse_live_schedule(html);
        assert_eq!(programs.len(), 2);
        assert!(programs[0].current);
        assert_eq!(programs[0].show_id.as_deref(), Some("TC"));
        assert_eq!(
            programs[0].description.as_deref(),
            Some("Rhino and psychedelic fork.")
        );
        assert_eq!(programs[1].day.as_deref(), Some("Tuesday"));
    }

    #[test]
    fn homepage_schedule_uses_the_highlighted_current_row() {
        let html = r##"<div id="playingtoday"><table>
          <tr bgcolor="#000000"><td colspan="2"><b>Tuesday</b></td></tr>
          <tr style="background-color: #DFBE3F;"><td>Noon&nbsp;to&nbsp;3pm</td>
            <td><a href="/playlists/CM">Feelings with Michele</a>
            <a href="/playlists/shows/166351">See playlist</a></td></tr>
          <tr style="background-color: #FFFFFF;"><td>3pm&nbsp;to&nbsp;6pm</td>
            <td><a href="/playlists/ED">The Evan Davies Show</a></td></tr>
        </table></div>"##;
        let programs = parse_live_schedule(html);
        assert_eq!(programs.len(), 2);
        assert!(programs[0].current);
        assert_eq!(programs[0].show_id.as_deref(), Some("CM"));
        assert_eq!(programs[1].starts_at.as_deref(), Some("3pm to 6pm"));
    }

    #[test]
    fn every_live_station_has_a_canonical_provider() {
        for id in ["freeform", "drummer", "rocknsoul", "sheena"] {
            let station = live_station(id).unwrap();
            assert!(rethink_playlist_url(station.rethink_code).ends_with(".json"));
            assert!(station.info_url.starts_with("https://wfmu.org/"));
        }
        assert!(live_station("unknown").is_none());
    }

    #[test]
    fn catalog_parses_hundreds_of_shows() {
        let shows = parse_catalog(&fixture("playlists_index.html"));
        assert!(shows.len() > 400, "got {}", shows.len());
        let wake = shows.iter().find(|s| s.id == "WA").expect("WA present");
        assert_eq!(wake.name, "Wake");
        assert_eq!(wake.dj.as_deref(), Some("Clay Pigeon"));
        assert!(wake.on_air);
        // Past show from alphabetical list only.
        let cod = shows.iter().find(|s| s.id == "CT").expect("CT present");
        assert_eq!(cod.name, "Codpaste");
    }

    #[test]
    fn show_page_parses_episodes() {
        let eps = parse_show_page(&fixture("show_WA.html"));
        assert!(eps.len() > 50, "got {}", eps.len());
        let ep = eps
            .iter()
            .find(|e| e.id == 166195)
            .expect("episode present");
        assert_eq!(ep.air_date.as_deref(), Some("July 9, 2026"));
        // archive id comes from the flashplayer link (what /archiveplayer/ needs), not listen.m3u
        assert_eq!(ep.archive_id, Some(291227));
        assert!(ep.title.as_deref().unwrap_or("").contains("Wake 'N Bake"));
    }

    #[test]
    fn deep_archive_short_dates_are_parsed() {
        // Kenny G-style playlist-index template: date is in the link text ("MM.DD.YY
        // playlist"), not a "Month DD, YYYY:" heading. It must still yield an air_date.
        let html = r#"
<span class="KDBFavIcon KDBepisode" id="KDBepisode-37520"></span>
<a href="/playlists/shows/37520">09.30.10 playlist</a>
| Listen: <a href="/flashplayer.php?version=3&amp;show=37520&amp;archive=99">Pop-up</a>
<span class="KDBFavIcon KDBepisode" id="KDBepisode-58274"></span>
<a href="/playlists/shows/58274">11.26.14 playlist</a>
<span class="KDBFavIcon KDBepisode" id="KDBepisode-136"></span>
<a href="https://www.wfmu.org/Playlists/KG/playlists/01/02.15.01.html">02.15.01 playlist</a>
<span class="KDBFavIcon KDBepisode" id="KDBepisode-888"></span>
&gt;
10.11.01
| Listen: <img src="/flashplayer/playbuttont.gif">
"#;
        let eps = parse_show_page(html);
        assert_eq!(eps.len(), 4);
        assert_eq!(eps[0].air_date.as_deref(), Some("September 30, 2010"));
        assert_eq!(eps[1].air_date.as_deref(), Some("November 26, 2014"));
        // Legacy per-year page link (different href, same "MM.DD.YY playlist" text).
        assert_eq!(eps[2].air_date.as_deref(), Some("February 15, 2001"));
        // Episode with no archived playlist: bare "MM.DD.YY" before "| Listen".
        assert_eq!(eps[3].air_date.as_deref(), Some("October 11, 2001"));
    }

    #[test]
    fn archiveplayer_extracts_audio_url() {
        let html = r#"<audio autoplay preload="metadata" src="https://s3.amazonaws.com/arch.wfmu.org/BT/bt010116r.mp4"></audio>"#;
        assert_eq!(
            parse_archiveplayer(html).as_deref(),
            Some("https://s3.amazonaws.com/arch.wfmu.org/BT/bt010116r.mp4")
        );
        let html2 = r#"<audio src="https://mp3archives.wfmu.org/archive/WA/wa260709.mp3" controls></audio>"#;
        assert_eq!(
            parse_archiveplayer(html2).as_deref(),
            Some("https://mp3archives.wfmu.org/archive/WA/wa260709.mp3")
        );
        assert_eq!(parse_archiveplayer("<p>no audio</p>"), None);
    }

    #[test]
    fn archiveplayer_extracts_offset() {
        let html = r#"<body
    data-offset="370"

    data-archiveid="275956"
    data-showid="155909"
>"#;
        assert_eq!(parse_archiveplayer_offset(html), Some(370));
        // No offset attribute -> None (caller defaults to 0).
        assert_eq!(parse_archiveplayer_offset("<body data-showid=\"1\">"), None);
    }

    #[test]
    fn show_description_is_extracted_and_trimmed() {
        let desc = parse_show_description(&fixture("show_WA.html")).expect("WA has a blurb");
        assert_eq!(
            desc,
            "WFMU's morning show, featuring new technology that will sonically force caffeine directly into your bloodstream. Hosted by Clay Pigeon."
        );
        assert!(!desc.contains("On WFMU"));
        assert!(!desc.contains("Newsletter"));
    }

    #[test]
    fn show_description_skips_leading_image() {
        // Real shape of show AU: a centered image paragraph precedes the blurb.
        let html = r##"<div class="everything">
        <p align="center">
        <img src="https://wfmu.org/Gfx/playlist_images/AU/x.jpg" alt="" >
        </p>
        <p><font size="-1">
        <b>The world is bound with secret knots. Everyday magic; magic every day.
        </b>
        </font></p>
        <div><b>Sunday Midnight - 3am | On <a href="https://wfmu.org/">WFMU</a></b></div>"##;
        assert_eq!(
            parse_show_description(html).as_deref(),
            Some("The world is bound with secret knots. Everyday magic; magic every day.")
        );
    }

    #[test]
    fn playlist_parses_tracks_with_timestamps() {
        let tracks = parse_playlist(&fixture("playlist_166195.html"));
        assert!(tracks.len() > 10, "got {}", tracks.len());
        let first = &tracks[0];
        assert_eq!(first.artist.as_deref(), Some("Dilemastronauta"));
        assert_eq!(first.title.as_deref(), Some("Donde Canta la Paloma"));
        assert_eq!(first.album.as_deref(), Some("SLEEPWALK - EP"));
        assert_eq!(first.start_sec, Some(0));
        assert!(tracks.iter().any(|t| t.start_sec.unwrap_or(0) > 60));
    }

    #[test]
    fn modern_playlist_cells_parse_without_nested_fonts() {
        let html = r#"
            <table><tbody>
              <tr>
                <td class="col_artist"><a>Artist One</a></td>
                <td class="col_song_title"><strong>Song One</strong></td>
                <td class="col_album_title">Album One</td>
                <td class="col_record_label">Label One</td>
                <td class="col_comments"><span>DJ note</span></td>
                <td class="col_live_timestamps_flag">0:00:05</td>
              </tr>
              <tr>
                <td class="col_artist">Artist Two</td>
                <td class="col_song_title">Song Two</td>
                <td class="col_album_title"></td>
                <td class="col_record_label">Label Two</td>
                <td class="col_comments"></td>
                <td class="col_live_timestamps_flag">0:03:10</td>
              </tr>
            </tbody></table>
        "#;
        let tracks = parse_playlist(html);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].artist.as_deref(), Some("Artist One"));
        assert_eq!(tracks[0].title.as_deref(), Some("Song One"));
        assert_eq!(tracks[0].album.as_deref(), Some("Album One"));
        assert_eq!(tracks[0].comments.as_deref(), Some("DJ note"));
        assert_eq!(tracks[0].start_sec, Some(5));
        assert_eq!(tracks[1].start_sec, Some(190));
    }

    #[test]
    fn current_station_playlist_skips_recent_archive_links() {
        let html = r#"
            <a href="/playlists/shows/111">recent archive</a>
            <h2>Playing Today</h2>
            <a href="/playlists/WA">Wake</a>
            <a href="/playlists/shows/222">See the playlist</a>
        "#;
        assert_eq!(parse_current_playlist(html), Some(222));
    }

    #[test]
    fn channel_landing_parses_hosted_live_playlist() {
        let status = parse_channel_landing(&fixture("channel_landing_drummer_live.json"))
            .expect("live status");
        assert_eq!(status.episode_id, Some(166500));
        assert_eq!(status.show_id.as_deref(), Some("WL"));
        assert_eq!(status.show_name.as_deref(), Some("Wound Liquor with Olleh"));
        assert_eq!(status.artist.as_deref(), Some("Heart Station"));
        assert_eq!(status.title.as_deref(), Some("Contagious Orgasm"));
    }

    #[test]
    fn channel_landing_parses_unattended_stream_metadata() {
        let status = parse_channel_landing(&fixture("channel_landing_drummer_auto.json"))
            .expect("automatic stream status");
        assert_eq!(status.episode_id, None);
        assert_eq!(status.artist.as_deref(), Some("Illaiyaraaja"));
        assert_eq!(
            status.title.as_deref(),
            Some("Yennadi Meenakshi feat. S.P. Balasubrahmanyam")
        );
        assert_eq!(status.show_name.as_deref(), Some("GTD Radio Stream"));
    }

    #[test]
    fn channel_page_config_builds_minimal_poll_url() {
        let html = r#"KDBchlnd.initialize({"poll_url":"channel_landing.php","cid":4});"#;
        assert_eq!(parse_channel_id(html), Some(4));
        assert_eq!(
            channel_nowplaying_url("https://wfmu.org/drummer", 4).as_deref(),
            Some("https://wfmu.org/channel_landing.php?cid=4&hashes%5Bnowplaying%5D=stale")
        );
    }

    #[test]
    fn configured_live_sources_use_direct_status_requests() {
        for (channel_id, expected) in [(4, "cid=4"), (6, "cid=6"), (8, "cid=8")] {
            let source = LiveStatusSource::Channel { channel_id };
            let url = source.url();
            assert!(url.contains("channel_landing.php"));
            assert!(url.contains(expected));
            assert!(source
                .parse(&fixture("channel_landing_drummer_auto.json"))
                .is_some());
        }
        assert_eq!(LiveStatusSource::Homepage.url(), "https://wfmu.org/");
    }

    #[test]
    fn homepage_status_uses_only_the_current_playlist_row() {
        let html = r#"
          <a href="/playlists/shows/111">Recent archive</a>
          <div id="playingtoday"><table>
            <tr><td><a href="/playlists/WA">Wake</a></td></tr>
            <tr style="background-color: #DFBE3F"><td>
              <a href="/playlists/GT">Garbage Time with Matt Warwick</a>
              <a href="/playlists/shows/166344">See the playlist</a>
            </td></tr>
          </table></div>
        "#;
        let status = LiveStatusSource::Homepage.parse(html).unwrap();
        assert_eq!(status.episode_id, Some(166344));
        assert_eq!(status.show_id.as_deref(), Some("GT"));
        assert_eq!(
            status.show_name.as_deref(),
            Some("Garbage Time with Matt Warwick")
        );
        assert_eq!(status.artist, None);
    }

    #[test]
    fn playlist_meta_reuses_show_episode_context() {
        let meta = parse_playlist_meta(&fixture("playlist_166195.html"));
        assert_eq!(meta.show_id.as_deref(), Some("WA"));
        assert_eq!(meta.show_name.as_deref(), Some("Wake with Clay Pigeon"));
        assert_eq!(meta.air_date.as_deref(), Some("July 9, 2026"));
    }

    #[test]
    fn freeform_comment_playlist_parses_tracks() {
        // Kenny G-style free-form template: no col_* table, tracks are ">Artist | Title"
        // with an optional "| H:MM:SS" start time in the pop-up link.
        let html = r#"
| <font size="-1">Approx. start time</font>
<br><br><br>
<span style="color: red">&gt;</span>
Nachum Segal
| Kenny G is Next
<span class="KDBFavIcon KDBsong" id="KDBsong-1456397"><a href="x">star</a></span>
<br><br>
<span style="color: red">&gt;</span>
Peter Sellers
| Singing in the rain
<span class="KDBFavIcon KDBsong" id="KDBsong-1456406"><a href="x">star</a></span>
|
0:32:14 <a href="/flashplayer.php?version=3&amp;show=58274&amp;archive=118775&amp;starttime=0:32:14">Pop-up</a>)
<br><br>
"#;
        // The tabular parser finds nothing, so parse_playlist falls back to free-form.
        let tracks = parse_playlist(html);
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].artist.as_deref(), Some("Nachum Segal"));
        assert_eq!(tracks[0].title.as_deref(), Some("Kenny G is Next"));
        assert_eq!(tracks[0].start_sec, None);
        assert_eq!(tracks[1].artist.as_deref(), Some("Peter Sellers"));
        assert_eq!(tracks[1].title.as_deref(), Some("Singing in the rain"));
        assert_eq!(tracks[1].start_sec, Some(32 * 60 + 14));
    }

    #[test]
    fn title_with_nested_markup_is_stripped() {
        // Real case: Advanced D&D (SU), May 19 2005 — title wrapped in <h2><font><b>…
        let html = r##"<span class="KDBFavIcon KDBepisode" id="KDBepisode-15110"></span>
        May 19, 2005:
        <b><h2><font face="Verdana, Arial, Helvetica, sans-serif"><b><font color="#EE0022">Death by vinyl!!!!</font></b></font></h2></b>
        | <a href="/playlists/shows/15110">See the playlist</a>
        | Listen: <a href="/flashplayer.php?version=3&amp;show=15110&amp;archive=122625">Pop-up</a>"##;
        let eps = parse_show_page(html);
        let ep = eps.iter().find(|e| e.id == 15110).expect("episode parsed");
        assert_eq!(ep.title.as_deref(), Some("Death by vinyl!!!!"));
        assert_eq!(ep.archive_id, Some(122625));
    }

    #[test]
    fn archive_years_extracted_from_dropdown() {
        let html = r##"<SELECT NAME="Ubu_Nav_Popup" ONCHANGE="goto_URL(this.form.Ubu_Nav_Popup)" SIZE="-3">
<OPTION VALUE="">OLDER PLAYLISTS</option>
<OPTION VALUE="">------</option>
<option value="/playlists/KG2010">2010</option>
<option value="/playlists/KG2009">2009</option>
<option value="/playlists/KG2008">2008</option>
</SELECT>"##;
        let years = parse_show_archive_years(html, "KG");
        assert_eq!(
            years,
            vec![
                "/playlists/KG2010".to_string(),
                "/playlists/KG2009".to_string(),
                "/playlists/KG2008".to_string(),
            ]
        );
    }

    #[test]
    fn archive_years_extracted_from_anchor_links() {
        // /playlists/BK has no dropdown at all; the years are a run of plain links.
        let html = fixture("show_BK.html");
        let years = parse_show_archive_years(&html, "BK");
        let expected: Vec<String> = (2001..=2023)
            .rev()
            .map(|y| format!("/playlists/BK{y}"))
            .collect();
        assert_eq!(years, expected);
    }

    #[test]
    fn archive_years_are_newest_first() {
        let html = r##"<a href="/playlists/BK2003">2003 playlists</a> |
<a href="/playlists/BK2001">2001 playlists</a> |
<a href="/playlists/BK2002">2002 playlists</a> |"##;
        let years = parse_show_archive_years(html, "BK");
        assert_eq!(
            years,
            vec![
                "/playlists/BK2003".to_string(),
                "/playlists/BK2002".to_string(),
                "/playlists/BK2001".to_string(),
            ]
        );
    }

    #[test]
    fn archive_years_deduped_across_dropdown_and_anchors() {
        let html = r##"<SELECT NAME="Ubu_Nav_Popup">
<option value="/playlists/KG2009">2009</option>
<option value="/playlists/KG2008">2008</option>
</SELECT>
<a href="/playlists/KG2009">2009 playlists</a> |
<a href="https://wfmu.org/playlists/KG2007">2007 playlists</a>"##;
        let years = parse_show_archive_years(html, "KG");
        assert_eq!(
            years,
            vec![
                "/playlists/KG2009".to_string(),
                "/playlists/KG2008".to_string(),
                "/playlists/KG2007".to_string(),
            ]
        );
    }

    #[test]
    fn archive_years_tolerate_uppercase_option_markup() {
        let html = r##"<SELECT NAME="Ubu_Nav_Popup">
<OPTION VALUE="">OLDER PLAYLISTS</OPTION>
<OPTION SELECTED VALUE="/Playlists/KG2009">2009</OPTION>
<OPTION VALUE='/playlists/KG2008'>2008</OPTION>
</SELECT>"##;
        let years = parse_show_archive_years(html, "KG");
        assert_eq!(
            years,
            vec![
                "/playlists/KG2009".to_string(),
                "/playlists/KG2008".to_string(),
            ]
        );
    }

    #[test]
    fn archive_years_exclude_the_show_page_itself() {
        // Year pages link back to /playlists/BK; chasing that would re-scrape the show.
        let html = fixture("show_BK2001.html");
        let years = parse_show_archive_years(&html, "BK");
        assert!(!years.contains(&"/playlists/BK".to_string()));
        assert!(years.contains(&"/playlists/BK2023".to_string()));
    }

    #[test]
    fn year_page_parses_episodes() {
        let html = fixture("show_BK2001.html");
        let eps = parse_show_page(&html);
        assert_eq!(eps.len(), 22);
        assert!(eps.iter().all(|e| e.air_date.is_some()));
        assert!(eps.iter().any(|e| e.archive_id.is_some()));
    }

    #[test]
    fn archive_years_ignores_non_matching_urls() {
        let html = r##"<SELECT NAME="Ubu_Nav_Popup">
<option value="/playlists/KG2009">2009</option>
<option value="/~kennyg/playlists/00.html">2000</option>
<option value="/playlists/WA2009">WA 2009 (wrong show)</option>
</SELECT>"##;
        let years = parse_show_archive_years(html, "KG");
        assert_eq!(years, vec!["/playlists/KG2009".to_string()]);
    }

    #[test]
    fn archive_years_empty_when_no_dropdown() {
        let html = "<html><body>No dropdown here</body></html>";
        let years = parse_show_archive_years(html, "KG");
        assert!(years.is_empty());
    }

    #[test]
    fn hms_parsing() {
        assert_eq!(parse_hms("0:05:56"), Some(356));
        assert_eq!(parse_hms("1:02:03"), Some(3723));
        assert_eq!(parse_hms("5:56"), Some(356));
        assert_eq!(parse_hms("junk"), None);
    }

    #[test]
    fn m3u_body_resolves() {
        assert_eq!(
            parse_m3u("https://mp3archives.wfmu.org/x/y.mp3\n"),
            Some("https://mp3archives.wfmu.org/x/y.mp3".to_string())
        );
        assert_eq!(
            parse_m3u("#EXTM3U\nhttps://a/b.mp3"),
            Some("https://a/b.mp3".into())
        );
        assert_eq!(parse_m3u(""), None);
    }
}
