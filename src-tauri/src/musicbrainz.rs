//! Recording length lookup on MusicBrainz, so a favourite song with no later timecode in
//! its playlist can still stop near its own end instead of running to the episode's.
//!
//! MusicBrainz asks for a descriptive User-Agent and at most one request per second; the
//! shared `Fetcher` client carries the first, the gate passed in carries the second.

use serde::Deserialize;
use std::time::{Duration, Instant};

const SEARCH_URL: &str = "https://musicbrainz.org/ws/2/recording";
const MIN_REQUEST_GAP: Duration = Duration::from_millis(1100);
/// Search relevance below this is too loose to trust for a cut point.
const MIN_SCORE: i64 = 90;
/// Lengths outside this range are data errors or whole-side medleys, not songs.
const MIN_LENGTH_SEC: i64 = 30;
const MAX_LENGTH_SEC: i64 = 30 * 60;

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    recordings: Vec<Recording>,
}

#[derive(Deserialize)]
struct Recording {
    id: String,
    #[serde(default)]
    score: i64,
    #[serde(default)]
    title: String,
    /// Milliseconds. Absent on many recordings.
    length: Option<i64>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<ArtistCredit>,
}

#[derive(Deserialize)]
struct ArtistCredit {
    #[serde(default)]
    name: String,
    #[serde(default)]
    joinphrase: String,
}

/// A matched recording: its length in whole seconds and its MusicBrainz id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    pub length_sec: i64,
    pub mbid: String,
}

/// Lowercase, drop bracketed qualifiers ("(2011 Remaster)", "[Live]"), drop a trailing
/// "feat. X", fold "&" to "and", keep letters and digits only, collapse spaces.
/// Applied to both sides of a comparison, so WFMU's hand-typed names and MusicBrainz's
/// canonical ones meet in the middle.
pub fn normalize(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut depth = 0usize;
    for c in raw.chars() {
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.saturating_sub(1),
            _ if depth == 0 => text.push(c),
            _ => {}
        }
    }
    let mut text = text.to_lowercase();
    for marker in [" feat. ", " feat ", " ft. ", " ft ", " featuring "] {
        if let Some(at) = text.find(marker) {
            text.truncate(at);
        }
    }
    let text = text.replace('&', " and ");
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.push(c);
        } else if !out.ends_with(' ') && !out.is_empty() {
            out.push(' ');
        }
    }
    let out = out.trim_end().to_string();
    out.strip_prefix("the ").map(str::to_string).unwrap_or(out)
}

fn artist_matches(wanted: &str, credit: &[ArtistCredit]) -> bool {
    let full: String = credit
        .iter()
        .map(|c| format!("{}{}", c.name, c.joinphrase))
        .collect();
    let full = normalize(&full);
    let wanted = normalize(wanted);
    if wanted.is_empty() || full.is_empty() {
        return false;
    }
    // "X & Y" on one side and "X" on the other is still the same record.
    full == wanted
        || full.starts_with(&format!("{wanted} "))
        || wanted.starts_with(&format!("{full} "))
        || credit.iter().any(|c| normalize(&c.name) == wanted)
}

/// Strip the characters that are syntax inside a quoted Lucene phrase.
fn phrase(raw: &str) -> String {
    raw.chars().filter(|c| !matches!(c, '"' | '\\')).collect()
}

pub fn search_url(artist: &str, title: &str, album: Option<&str>) -> String {
    let mut query = format!(
        "recording:\"{}\" AND artist:\"{}\"",
        phrase(title),
        phrase(artist)
    );
    if let Some(album) = album.filter(|a| !a.trim().is_empty()) {
        query.push_str(&format!(" AND release:\"{}\"", phrase(album)));
    }
    let mut url = reqwest::Url::parse(SEARCH_URL).expect("static url");
    url.query_pairs_mut()
        .append_pair("query", &query)
        .append_pair("fmt", "json")
        .append_pair("limit", "10");
    url.into()
}

/// The median length of the confident, name-matching results. Versions of one song
/// (single, album, remaster) differ by a few seconds; the median ignores the odd extended
/// mix. None when nothing qualifies.
pub fn pick(body: &str, artist: &str, title: &str) -> Result<Option<Match>, String> {
    let response: SearchResponse =
        serde_json::from_str(body).map_err(|e| format!("musicbrainz json: {e}"))?;
    let wanted_title = normalize(title);
    let mut hits: Vec<(i64, String)> = response
        .recordings
        .into_iter()
        .filter(|r| r.score >= MIN_SCORE)
        .filter(|r| normalize(&r.title) == wanted_title)
        .filter(|r| artist_matches(artist, &r.artist_credit))
        .filter_map(|r| {
            let sec = (r.length? + 500) / 1000;
            (MIN_LENGTH_SEC..=MAX_LENGTH_SEC)
                .contains(&sec)
                .then_some((sec, r.id))
        })
        .collect();
    if hits.is_empty() {
        return Ok(None);
    }
    hits.sort_by_key(|(sec, _)| *sec);
    let (length_sec, mbid) = hits.swap_remove(hits.len() / 2);
    Ok(Some(Match { length_sec, mbid }))
}

async fn get(
    client: &reqwest::Client,
    gate: &tokio::sync::Mutex<Option<Instant>>,
    url: &str,
) -> Result<String, String> {
    {
        let mut last = gate.lock().await;
        if let Some(t) = *last {
            let elapsed = t.elapsed();
            if elapsed < MIN_REQUEST_GAP {
                tokio::time::sleep(MIN_REQUEST_GAP - elapsed).await;
            }
        }
        *last = Some(Instant::now());
    }
    let resp = client
        .get(url)
        .header("Accept", "application/json")
        .timeout(Duration::from_secs(20))
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

/// Search with the album first, since it pins the right version; a playlist's album
/// field is often a compilation MusicBrainz files differently, so a miss retries without.
pub async fn lookup(
    client: &reqwest::Client,
    gate: &tokio::sync::Mutex<Option<Instant>>,
    artist: &str,
    title: &str,
    album: Option<&str>,
) -> Result<Option<Match>, String> {
    let album = album.filter(|a| !a.trim().is_empty());
    if album.is_some() {
        let body = get(client, gate, &search_url(artist, title, album)).await?;
        if let Some(found) = pick(&body, artist, title)? {
            return Ok(Some(found));
        }
    }
    let body = get(client, gate, &search_url(artist, title, None)).await?;
    pick(&body, artist, title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_qualifiers_and_punctuation() {
        assert_eq!(normalize("Hey Jude (2015 Remaster)"), "hey jude");
        assert_eq!(normalize("The Fall"), "fall");
        assert_eq!(normalize("Simon & Garfunkel"), "simon and garfunkel");
        assert_eq!(normalize("Song [Live] feat. Someone"), "song");
        assert_eq!(normalize("  Don't  Stop! "), "don t stop");
    }

    const SAMPLE: &str = r#"{"recordings":[
        {"id":"a","score":100,"title":"Totally Wired","length":181000,
         "artist-credit":[{"name":"The Fall","joinphrase":""}]},
        {"id":"b","score":98,"title":"Totally Wired (Peel Session)","length":200000,
         "artist-credit":[{"name":"The Fall","joinphrase":""}]},
        {"id":"c","score":95,"title":"Totally Wired","length":183400,
         "artist-credit":[{"name":"The Fall","joinphrase":""}]},
        {"id":"d","score":95,"title":"Totally Wired","length":420000,
         "artist-credit":[{"name":"The Fall","joinphrase":""}]},
        {"id":"e","score":60,"title":"Totally Wired","length":90000,
         "artist-credit":[{"name":"The Fall","joinphrase":""}]},
        {"id":"f","score":100,"title":"Totally Wired","length":150000,
         "artist-credit":[{"name":"Someone Else","joinphrase":""}]},
        {"id":"g","score":100,"title":"Totally Wired",
         "artist-credit":[{"name":"The Fall","joinphrase":""}]}
    ]}"#;

    #[test]
    fn pick_takes_median_of_confident_matches() {
        // Qualifying: a (181), b (200, title normalizes equal), c (183), d (420).
        // Low score, other artist and missing length drop out.
        let found = pick(SAMPLE, "Fall", "Totally Wired").unwrap().unwrap();
        assert_eq!(found.length_sec, 200);
        assert_eq!(found.mbid, "b");
    }

    #[test]
    fn pick_rejects_wrong_title() {
        assert_eq!(pick(SAMPLE, "The Fall", "Hip Priest").unwrap(), None);
    }

    #[test]
    fn pick_accepts_partial_artist_credit() {
        let body = r#"{"recordings":[{"id":"x","score":100,"title":"Song","length":200000,
            "artist-credit":[{"name":"A","joinphrase":" & "},{"name":"B","joinphrase":""}]}]}"#;
        assert_eq!(pick(body, "B", "Song").unwrap().unwrap().length_sec, 200);
        assert_eq!(pick(body, "A & B", "Song").unwrap().unwrap().length_sec, 200);
    }

    #[test]
    fn pick_handles_empty_response() {
        assert_eq!(pick(r#"{"recordings":[]}"#, "A", "B").unwrap(), None);
        assert_eq!(pick("{}", "A", "B").unwrap(), None);
    }

    #[test]
    fn search_url_quotes_and_encodes() {
        let url = search_url("A \"B\"", "C", Some("D"));
        assert!(url.starts_with(SEARCH_URL));
        assert!(url.contains("fmt=json"));
        assert!(!url.contains("%5C"));
        assert!(url.contains("release"));
        assert!(!search_url("A", "C", Some("  ")).contains("release"));
    }
}
