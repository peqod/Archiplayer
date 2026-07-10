use regex::Regex;
use scraper::{Html, Selector};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

pub const BASE: &str = "https://wfmu.org";
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36 ArchiveBunker2/0.1";
const MIN_REQUEST_GAP: Duration = Duration::from_millis(1000);

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

pub struct Fetcher {
    client: reqwest::Client,
    last_request: tokio::sync::Mutex<Option<Instant>>,
}

impl Fetcher {
    pub fn new() -> Self {
        Fetcher {
            client: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .build()
                .expect("http client"),
            last_request: tokio::sync::Mutex::new(None),
        }
    }

    pub fn client(&self) -> &reqwest::Client {
        &self.client
    }

    /// Polite GET: global 1 req/s toward wfmu.org.
    pub async fn get_text(&self, url: &str) -> Result<String, String> {
        {
            let mut last = self.last_request.lock().await;
            if let Some(t) = *last {
                let elapsed = t.elapsed();
                if elapsed < MIN_REQUEST_GAP {
                    tokio::time::sleep(MIN_REQUEST_GAP - elapsed).await;
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
        resp.text().await.map_err(|e| format!("read body failed: {e}"))
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
        "January", "February", "March", "April", "May", "June",
        "July", "August", "September", "October", "November", "December",
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
    let dj_re = DJ
        .get_or_init(|| Regex::new(r#"(?s)</b>\s*(?:with|w/)\s+(.+?)\s*(?:-\s*)?<a href="/playlists/"#).unwrap());

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
        let dj = dj_re.captures(chunk).map(|c| clean_text(&c[1])).and_then(non_empty);
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
        Regex::new(r"flashplayer\.php\?version=\d+&(?:amp;)?show=(\d+)&(?:amp;)?archive=(\d+)").unwrap()
    })
}
fn listen_re() -> &'static Regex {
    LISTEN_RE.get_or_init(|| Regex::new(r"listen\.m3u\?show=(\d+)&(?:amp;)?archive=(\d+)").unwrap())
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
    let date_re =
        DATE.get_or_init(|| Regex::new(r"([A-Z][a-z]+ \d{1,2}, \d{4})\s*:").unwrap());
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
        .filter_map(|c| c[1].parse::<i64>().ok().map(|id| (id, c.get(0).unwrap().end())))
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
    for m in ["</font>", "<br", "On WFMU", "( Visit", "(Visit", "Visit&nbsp;homepage", "</p>", "</div>"] {
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

/// Discover archive-year sub-pages for a show that has an "OLDER PLAYLISTS"
/// dropdown on its page (e.g. /playlists/KG links to /playlists/KG2009, etc.).
/// Returns the URL path portions (e.g. "/playlists/KG2009") for each year.
pub fn parse_show_archive_years(html: &str, show_id: &str) -> Vec<String> {
    // Find the <SELECT NAME="Ubu_Nav_Popup"> block.
    let sel_start = match html.find("Ubu_Nav_Popup") {
        Some(i) => i,
        None => return vec![],
    };
    // Back up to the opening <select / <SELECT tag.
    let block_start = match html[..sel_start]
        .rfind("<select")
        .or_else(|| html[..sel_start].rfind("<SELECT"))
    {
        Some(i) => i,
        None => return vec![],
    };
    let block = &html[block_start..];
    let block_end = match block.find("</select>").or_else(|| block.find("</SELECT>")) {
        Some(i) => i,
        None => return vec![],
    };
    let block = &block[..block_end];

    static OPTION_RE: OnceLock<Regex> = OnceLock::new();
    let re = OPTION_RE.get_or_init(|| {
        Regex::new(r#"<option\s+value="(/playlists/[A-Za-z0-9]+)""#).unwrap()
    });

    // Build the expected prefix: /playlists/{show_id} followed by digits.
    let prefix = format!("/playlists/{show_id}");
    let mut years: Vec<String> = re
        .captures_iter(block)
        .map(|c| c[1].to_string())
        .filter(|path| {
            path.starts_with(&prefix)
                && path[prefix.len()..].chars().all(|c| c.is_ascii_digit())
        })
        .collect();
    years.sort();
    years
}

/// Extract the archive id from a single playlist page (its "Listen to this show" /
/// pop-up player link). Used to discover audio for episodes whose show-index block
/// carried no archive link.
pub fn parse_playlist_archive(html: &str) -> Option<i64> {
    flash_re()
        .captures(html)
        .and_then(|c| c[2].parse::<i64>().ok())
        .or_else(|| listen_re().captures(html).and_then(|c| c[2].parse::<i64>().ok()))
}

/// Extract the direct audio URL from an AccuPlayer page
/// (https://wfmu.org/archiveplayer/?show={ep}&archive={arch}).
/// Works for both storage backends (mp3archives.wfmu.org and s3.amazonaws.com/arch.wfmu.org).
pub fn parse_archiveplayer(html: &str) -> Option<String> {
    static AUDIO: OnceLock<Regex> = OnceLock::new();
    static ANY: OnceLock<Regex> = OnceLock::new();
    let audio = AUDIO.get_or_init(|| Regex::new(r#"<audio[^>]*\bsrc="([^"]+)""#).unwrap());
    let any = ANY.get_or_init(|| {
        Regex::new(r#"src="(https://[^"]+\.(?:mp3|mp4|m4a|aac))""#).unwrap()
    });
    audio
        .captures(html)
        .map(|c| decode_entities(&c[1]))
        .filter(|u| u.starts_with("http"))
        .or_else(|| any.captures(html).map(|c| decode_entities(&c[1])))
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
pub fn parse_playlist(html: &str) -> Vec<ParsedTrack> {
    static TIME: OnceLock<Regex> = OnceLock::new();
    let time_re = TIME.get_or_init(|| Regex::new(r"(\d+:\d{1,2}:\d{2})").unwrap());

    let doc = Html::parse_document(html);
    let row_sel = Selector::parse("tr").unwrap();
    let font_sel = Selector::parse("font").unwrap();
    let cell = |class: &str| Selector::parse(&format!("td.{class}")).unwrap();
    let artist_sel = cell("col_artist");
    let title_sel = cell("col_song_title");
    let album_sel = cell("col_album_title");
    let label_sel = cell("col_record_label");
    let comments_sel = cell("col_comments");
    let time_sel = cell("col_live_timestamps_flag");

    let first_font_text = |row: scraper::ElementRef, sel: &Selector| -> Option<String> {
        let td = row.select(sel).next()?;
        let font = td.select(&font_sel).next()?;
        non_empty(clean_text(&font.text().collect::<String>()))
    };

    let mut tracks = Vec::new();
    for row in doc.select(&row_sel) {
        if row.select(&artist_sel).next().is_none() && row.select(&title_sel).next().is_none() {
            continue;
        }
        let artist = first_font_text(row, &artist_sel);
        let title = first_font_text(row, &title_sel);
        let album = first_font_text(row, &album_sel);
        let label = first_font_text(row, &label_sel);
        let comments = first_font_text(row, &comments_sel);
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
        let ep = eps.iter().find(|e| e.id == 166195).expect("episode present");
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
                "/playlists/KG2008".to_string(),
                "/playlists/KG2009".to_string(),
                "/playlists/KG2010".to_string(),
            ]
        );
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
        assert_eq!(parse_m3u("#EXTM3U\nhttps://a/b.mp3"), Some("https://a/b.mp3".into()));
        assert_eq!(parse_m3u(""), None);
    }
}
