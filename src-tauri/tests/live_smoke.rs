//! Live end-to-end smoke test against wfmu.org. Ignored by default (network + politeness).
//! Run explicitly:  cargo test --test live_smoke -- --ignored --nocapture
use archiplayer_lib::wfmu::{self, Fetcher};

#[tokio::test]
#[ignore]
async fn live_pipeline_catalog_show_playlist_audio() {
    let f = Fetcher::new();

    // 1. Catalog (403-guarded; proves browser UA works)
    let html = f.get_text(&wfmu::catalog_url()).await.expect("catalog fetch");
    let shows = wfmu::parse_catalog(&html);
    assert!(shows.len() > 400, "catalog shows: {}", shows.len());
    let wake = shows.iter().find(|s| s.id == "WA").expect("WA in catalog");
    eprintln!("catalog: {} shows; sample: {} / {:?}", shows.len(), wake.name, wake.dj);

    // 2. Show page → episodes + blurb
    let show_html = f.get_text(&wfmu::show_url("WA")).await.expect("show fetch");
    let eps = wfmu::parse_show_page(&show_html);
    assert!(eps.len() > 5, "episodes: {}", eps.len());
    let desc = wfmu::parse_show_description(&show_html).expect("WA blurb");
    eprintln!("WA blurb: {desc}");
    assert!(desc.to_lowercase().contains("caffeine") && !desc.contains("On WFMU"));
    let ep = eps.iter().find(|e| e.archive_id.is_some()).expect("an ep with audio");
    eprintln!("show WA: {} episodes; first playable ep {} arch {:?}", eps.len(), ep.id, ep.archive_id);

    // 3. Playlist → tracks
    let pl_html = f.get_text(&wfmu::playlist_url(ep.id)).await.expect("playlist fetch");
    let tracks = wfmu::parse_playlist(&pl_html);
    eprintln!("playlist ep {}: {} tracks", ep.id, tracks.len());
    assert!(!tracks.is_empty(), "expected tracks for ep {}", ep.id);

    // 4. Resolve audio via AccuPlayer → direct audio URL
    let player = f
        .get_text(&wfmu::archiveplayer_url(ep.id, ep.archive_id.unwrap()))
        .await
        .expect("archiveplayer fetch");
    let audio = wfmu::parse_archiveplayer(&player).expect("audio url");
    eprintln!("audio url: {audio}");
    assert!(audio.starts_with("http") && audio.contains("wfmu.org"));

    // 5. Confirm the audio host honours range requests (needed for seek/scrub)
    let head = f
        .client()
        .get(&audio)
        .header("Range", "bytes=0-1023")
        .send()
        .await
        .expect("range request");
    assert_eq!(head.status().as_u16(), 206, "expected 206 partial content");
    eprintln!("range OK: {} / content-range {:?}", head.status(), head.headers().get("content-range"));

    // 6. Deep-archive show (Brian Turner): no listen.m3u links on the index at all,
    //    audio lives on S3. Proves the old-episode fix end to end.
    let bt_html = f.get_text(&wfmu::show_url("BT")).await.expect("BT show fetch");
    let bt_eps = wfmu::parse_show_page(&bt_html);
    let bt_playable = bt_eps.iter().filter(|e| e.archive_id.is_some()).count();
    eprintln!("Brian Turner: {} episodes, {} playable", bt_eps.len(), bt_playable);
    assert!(bt_playable > 500, "expected most BT episodes playable, got {bt_playable}");
    let old = bt_eps.last().expect("an old BT episode");
    let bt_player = f
        .get_text(&wfmu::archiveplayer_url(old.id, old.archive_id.expect("old BT archive")))
        .await
        .expect("BT archiveplayer fetch");
    let bt_audio = wfmu::parse_archiveplayer(&bt_player).expect("BT audio url");
    eprintln!("old BT ep {} → {bt_audio}", old.id);
    assert!(bt_audio.starts_with("http"));
}
