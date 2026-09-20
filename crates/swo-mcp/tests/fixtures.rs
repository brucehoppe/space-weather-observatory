//! Fixture-only tests: a cache built from `fixtures/captured/`, a fixed clock,
//! no network. Mirrors the snapshot table in `src-tauri/src/store.rs`.

use chrono::{DateTime, Utc};
use rmcp::handler::server::wrapper::Parameters;
use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};
use swo_mcp::detail::{FlaresRequest, KpRequest};
use swo_mcp::{
    glossary, ollama, reports, ExplainRequest, SaveReportRequest, SolarWindRequest, SwoServer,
};

const CAPTURED_AT: &str = "2026-09-06T18:04:00Z";

fn t(s: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
}

fn fixture(name: &str) -> String {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/captured/");
    std::fs::read_to_string(format!("{dir}{name}")).unwrap()
}

fn cache() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
         CREATE TABLE snapshots (
             id INTEGER PRIMARY KEY AUTOINCREMENT,
             product TEXT NOT NULL, source_url TEXT NOT NULL,
             retrieved_at TEXT NOT NULL, sha256 TEXT NOT NULL, payload TEXT NOT NULL,
             UNIQUE (product, sha256));
         INSERT INTO meta VALUES ('schema_version', '1');",
    )
    .unwrap();
    insert(
        &conn,
        &[
            (
                "rtsw_wind_1m",
                "rtsw_wind_1m.json",
                swo_core::parse::rtsw::WIND_URL,
            ),
            (
                "rtsw_mag_1m",
                "rtsw_mag_1m.json",
                swo_core::parse::rtsw::MAG_URL,
            ),
        ],
    );
    conn
}

fn insert(conn: &Connection, products: &[(&str, &str, &str)]) {
    for &(product, file, url) in products {
        let payload = fixture(file);
        let sha = format!("{:x}", Sha256::digest(payload.as_bytes()));
        conn.execute(
            "INSERT INTO snapshots (product, source_url, retrieved_at, sha256, payload)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![product, url, CAPTURED_AT, sha, payload],
        )
        .unwrap();
    }
}

/// Every product the app caches, as captured on 2026-09-06.
fn full_cache() -> Connection {
    use swo_core::parse::*;
    let conn = cache();
    insert(
        &conn,
        &[
            (
                "goes_xrays_1day",
                "goes_primary_xrays_1day.json",
                goes::PRIMARY_XRAYS_1DAY_URL,
            ),
            (
                "goes_instrument_sources",
                "goes_instrument_sources.json",
                goes::INSTRUMENT_SOURCES_URL,
            ),
            ("planetary_k_index", "planetary_k_index.json", kp::KP_URL),
            (
                "planetary_k_index_forecast",
                "planetary_k_index_forecast.json",
                kp::KP_FORECAST_URL,
            ),
            ("noaa_scales", "noaa_scales.json", scales::URL),
            ("alerts", "alerts.json", bulletins::URL),
            (
                "ovation_aurora_latest",
                "ovation_aurora_latest.json",
                ovation::URL,
            ),
            (
                "three_day_forecast",
                "3-day-forecast.txt",
                forecast_text::THREE_DAY_URL,
            ),
            (
                "three_day_geomag_forecast",
                "3-day-geomag-forecast.txt",
                forecast_text::GEOMAG_URL,
            ),
            (
                "solar_cycle_observed",
                "observed_solar_cycle_indices.json",
                solar_cycle::OBSERVED_URL,
            ),
            (
                "solar_cycle_predicted",
                "predicted_solar_cycle.json",
                solar_cycle::PREDICTED_URL,
            ),
        ],
    );
    conn
}

fn full_server() -> SwoServer {
    SwoServer::from_connection(full_cache())
        .unwrap()
        .with_fixed_now(t("2026-09-06T18:30:00Z"))
}

/// A fresh, empty directory under Cargo's per-test temp dir.
fn reports_dir(name: &str) -> std::path::PathBuf {
    let dir = std::path::Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const REPORT_BODY: &str =
    "**Calm.** The solar wind is ordinary and no storms are in progress or forecast by NOAA.";

fn server() -> SwoServer {
    SwoServer::from_connection(cache())
        .unwrap()
        .with_fixed_now(t("2026-09-06T18:30:00Z"))
}

#[tokio::test]
async fn list_products_reports_availability_and_age() {
    let list = server().list_products().await.unwrap().0;
    let wind = list
        .products
        .iter()
        .find(|p| p.product == "rtsw_wind_1m")
        .unwrap();
    assert!(wind.available);
    assert_eq!(wind.age_minutes, Some(26));
    assert_eq!(wind.snapshot_count, 1);
    let kp = list
        .products
        .iter()
        .find(|p| p.product == "planetary_k_index")
        .unwrap();
    assert!(!kp.available, "absent products are reported, not invented");
}

#[tokio::test]
async fn solar_wind_carries_provenance_and_consistent_summary() {
    let req = SolarWindRequest {
        window_minutes: Some(60),
        at: None,
    };
    let r = server().get_solar_wind(Parameters(req)).await.unwrap().0;

    assert_eq!(r.sources.len(), 2);
    assert!(r
        .sources
        .iter()
        .all(|s| s.retrieved_at == "2026-09-06T18:04:00Z"));
    assert!(r.sources.iter().all(|s| s.payload_sha256.len() == 64));

    // Samples are oldest-first and inside the window.
    let times: Vec<&str> = r.speed_km_s.iter().map(|s| s.time.as_str()).collect();
    assert!(times.windows(2).all(|w| w[0] <= w[1]));
    assert!(times
        .iter()
        .all(|&x| x >= r.window_start.as_str() && x <= r.window_end.as_str()));

    // Summary agrees with the samples it summarises.
    let s = &r.summary.speed_km_s;
    let vals: Vec<f64> = r.speed_km_s.iter().filter_map(|x| x.value).collect();
    assert_eq!(s.max, vals.iter().copied().reduce(f64::max));
    assert_eq!(s.missing, r.speed_km_s.len() - vals.len());
    assert!(r.summary.bz_southward_minutes <= r.bz_gsm_nt.len());
}

#[tokio::test]
async fn replay_before_first_snapshot_is_an_error_not_a_guess() {
    let req = SolarWindRequest {
        window_minutes: None,
        at: Some("2026-09-01T00:00:00Z".into()),
    };
    assert!(server().get_solar_wind(Parameters(req)).await.is_err());
}

#[tokio::test]
async fn rejects_an_unknown_schema() {
    let conn = cache();
    conn.execute(
        "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
        [],
    )
    .unwrap();
    assert!(SwoServer::from_connection(conn).is_err());
}

// ---------------------------------------------------------------- dashboard

#[tokio::test]
async fn dashboard_covers_every_panel_from_the_captured_products() {
    let d = full_server().get_dashboard().await.unwrap().0;
    assert!(d.unavailable.is_empty(), "{:?}", d.unavailable);
    assert!(d.solar_wind.is_some() && d.xray_flux.is_some() && d.kp_index.is_some());
    assert!(d.noaa_scales.is_some() && d.bulletins.is_some() && d.three_day_forecast.is_some());
    assert!(d.aurora.is_some() && d.solar_cycle.is_some());
    assert!(!d.statements.is_empty());
    assert_eq!(d.oldest_retrieval_age_minutes, Some(26));
    assert!(
        d.stale_data_warning.is_none(),
        "26-minute-old data is not stale"
    );

    // The latest X-ray class is what the app's classifier says about the latest flux.
    let x = d.xray_flux.unwrap();
    let class =
        swo_core::flare::classify(x.latest_flux_w_m2.unwrap(), swo_core::flare::XrayBand::Long)
            .unwrap();
    assert_eq!(x.latest_class, Some(class.format()));
}

#[tokio::test]
async fn missing_products_are_reported_missing_never_quiet() {
    let d = server().get_dashboard().await.unwrap().0; // wind + mag only
    assert!(d.solar_wind.is_some());
    assert!(d.kp_index.is_none() && d.noaa_scales.is_none());
    assert!(d.unavailable.iter().any(|u| u.starts_with("noaa_scales")));
    assert!(d
        .statements
        .iter()
        .any(|s| s.headline == "NOAA scale status unavailable"));
}

#[tokio::test]
async fn old_data_carries_a_warning() {
    let server = SwoServer::from_connection(full_cache())
        .unwrap()
        .with_fixed_now(t("2026-09-08T18:04:00Z"));
    let d = server.get_dashboard().await.unwrap().0;
    assert!(d.stale_data_warning.unwrap().contains("2 days"));
    // An interval only counts as "latest" if it had begun when the data was retrieved.
    let latest = d.kp_index.unwrap().latest.unwrap();
    assert!(latest.interval_start.as_str() <= CAPTURED_AT);
}

#[tokio::test]
async fn fingerprint_changes_only_when_the_data_changes() {
    let a = full_server()
        .get_dashboard()
        .await
        .unwrap()
        .0
        .data_fingerprint;
    let later = SwoServer::from_connection(full_cache())
        .unwrap()
        .with_fixed_now(t("2026-09-06T19:30:00Z"));
    assert_eq!(
        a,
        later.get_dashboard().await.unwrap().0.data_fingerprint,
        "time alone does not change it"
    );
    assert_ne!(
        a,
        server().get_dashboard().await.unwrap().0.data_fingerprint
    );
}

// ---------------------------------------------------------------- detail tools

#[tokio::test]
async fn kp_kinds_are_never_merged() {
    let r = full_server()
        .get_kp(Parameters(KpRequest::default()))
        .await
        .unwrap()
        .0;
    assert!(r.observed.iter().all(|p| p.kind == "observed"));
    assert!(r.estimated.iter().all(|p| p.kind == "NOAA estimate"));
    assert!(r.forecast.iter().all(|p| p.kind == "forecast"));
    assert!(!r.forecast.is_empty() && !(r.observed.is_empty() && r.estimated.is_empty()));
    // Nothing labelled as measured begins after the retrieval.
    assert!(r
        .observed
        .iter()
        .chain(&r.estimated)
        .all(|p| p.interval_start.as_str() <= CAPTURED_AT));
    assert!(r
        .estimated_not_yet_begun
        .iter()
        .all(|p| p.interval_start.as_str() > CAPTURED_AT));
}

#[tokio::test]
async fn flare_events_agree_with_the_classifier_and_threshold() {
    let req = FlaresRequest {
        min_class: Some("b".into()),
        at: None,
    };
    let r = full_server()
        .get_xray_flares(Parameters(req))
        .await
        .unwrap()
        .0;
    assert_eq!(r.min_class, "B");
    assert!(r.peak_class.is_some() && r.background_class.is_some());
    for e in &r.events {
        assert!(e.peak_flux_w_m2 >= 1e-7, "event below the B floor");
        assert!(e.start <= e.peak_time);
        let class =
            swo_core::flare::classify(e.peak_flux_w_m2, swo_core::flare::XrayBand::Long).unwrap();
        assert_eq!(e.peak_class, class.format());
    }
    // An X threshold can only ever report X-class peaks.
    let x = FlaresRequest {
        min_class: Some("X".into()),
        at: None,
    };
    let r = full_server()
        .get_xray_flares(Parameters(x))
        .await
        .unwrap()
        .0;
    assert!(r.events.iter().all(|e| e.peak_class.starts_with('X')));

    let bad = FlaresRequest {
        min_class: Some("Z".into()),
        at: None,
    };
    assert!(full_server()
        .get_xray_flares(Parameters(bad))
        .await
        .is_err());
}

#[tokio::test]
async fn interpretation_comes_from_the_rule_layer() {
    let r = full_server().get_interpretation().await.unwrap().0;
    assert_eq!(r.rule_version, swo_core::interpret::RULE_VERSION);
    assert!(!r.statements.is_empty());
    assert!(r
        .statements
        .iter()
        .all(|s| s.rule_version == r.rule_version && !s.source_ref.is_empty()));
    assert!(
        r.statements.iter().any(|s| s.basis == "Interpretation"),
        "the solar-wind note is labelled as interpretation"
    );
}

// ---------------------------------------------------------------- explanations

#[tokio::test]
async fn every_reading_has_a_complete_explanation() {
    let all = full_server()
        .explain_reading(Parameters(ExplainRequest::default()))
        .await
        .unwrap()
        .0;
    assert_eq!(all.entries.len(), glossary::ids().len());
    for e in &all.entries {
        for text in [
            &e.what_it_is,
            &e.how_to_read_it,
            &e.why_it_matters,
            &e.caveats,
        ] {
            assert!(text.len() > 40, "{} has a thin explanation", e.id);
        }
    }
}

#[tokio::test]
async fn explain_reading_matches_loosely_and_attaches_the_current_value() {
    let server = full_server();
    for (query, id) in [
        ("bz", "bz"),
        ("Bz (GSM)", "bz"),
        ("solar wind density", "solar_wind_density"),
        ("the Kp number", "kp_index"),
        ("flare class", "xray_flux"),
        ("G scale", "noaa_scales"),
        ("northern lights", "aurora"),
    ] {
        let req = ExplainRequest {
            reading: Some(query.into()),
        };
        let r = server.explain_reading(Parameters(req)).await.unwrap().0;
        assert_eq!(r.entries[0].id, id, "query {query:?}");
        assert!(
            r.current.is_some(),
            "{id} has a current value in the full cache"
        );
    }
    let req = ExplainRequest {
        reading: Some("nonsense".into()),
    };
    let err = server.explain_reading(Parameters(req)).await.err().unwrap();
    assert!(
        err.contains("kp_index"),
        "the error lists valid ids so a model can correct itself"
    );
}

#[test]
fn every_glossary_field_exists_on_the_dashboard() {
    let conn = full_cache();
    let board =
        serde_json::to_value(swo_mcp::dashboard::build(&conn, t("2026-09-06T18:30:00Z"))).unwrap();
    for e in glossary::all()
        .iter()
        .filter(|e| !e.dashboard_field.starts_with(['(', '*']))
    {
        let found = e
            .dashboard_field
            .split('.')
            .try_fold(&board, |v, k| v.get(k));
        assert!(
            found.is_some_and(|v| !v.is_null()),
            "{} points at missing field {}",
            e.id,
            e.dashboard_field
        );
    }
}

// ---------------------------------------------------------------- reports

#[tokio::test]
async fn a_saved_report_is_framed_with_computed_numbers_and_stays_current() {
    let server = full_server().with_reports_dir(reports_dir("saved"));
    let before = server.get_report().await.unwrap().0;
    assert!(before.report.is_none() && !before.freshness.is_current);

    let req = SaveReportRequest {
        summary_markdown: REPORT_BODY.into(),
        model: "test-model".into(),
    };
    let saved = server.save_report(Parameters(req)).await.unwrap().0;
    assert!(
        saved.headline.starts_with("Calm. The solar wind"),
        "markdown markers are stripped: {}",
        saved.headline
    );
    let on_disk = std::fs::read_to_string(&saved.path).unwrap();

    let status = server.get_report().await.unwrap().0;
    assert!(
        status.freshness.is_current,
        "{:?}",
        status.freshness.reasons
    );
    let report = status.report.unwrap();
    assert_eq!(report.markdown, on_disk);
    assert!(report.markdown.contains(REPORT_BODY));
    assert!(report.markdown.contains("## Readings at a glance"));
    assert!(
        report.markdown.contains("test-model")
            && report.markdown.contains("not an official forecast")
    );
    // The speed in the table is the dashboard's, to the rounding a person would use.
    let speed = server
        .get_dashboard()
        .await
        .unwrap()
        .0
        .solar_wind
        .unwrap()
        .speed_km_s
        .latest
        .unwrap();
    assert!(report
        .markdown
        .contains(&format!("| Solar-wind speed | {speed:.0} km/s |")));
}

#[tokio::test]
async fn a_report_goes_out_of_date_when_data_changes_or_time_passes() {
    let dir = reports_dir("stale");
    let req = || {
        Parameters(SaveReportRequest {
            summary_markdown: REPORT_BODY.into(),
            model: "m".into(),
        })
    };
    full_server()
        .with_reports_dir(&dir)
        .save_report(req())
        .await
        .unwrap();

    // Same data, seven hours later.
    let later = SwoServer::from_connection(full_cache())
        .unwrap()
        .with_fixed_now(t("2026-09-07T01:31:00Z"))
        .with_reports_dir(&dir);
    let f = later.get_report().await.unwrap().0.freshness;
    assert!(
        !f.is_current && f.reasons.iter().any(|r| r.contains("hours old")),
        "{:?}",
        f.reasons
    );

    // Same time, different data.
    let other = server().with_reports_dir(&dir);
    let f = other.get_report().await.unwrap().0.freshness;
    assert!(
        !f.is_current && f.reasons.iter().any(|r| r.contains("data has changed")),
        "{:?}",
        f.reasons
    );
}

#[tokio::test]
async fn an_empty_report_is_rejected_and_old_data_is_flagged_in_the_frame() {
    let server = SwoServer::from_connection(full_cache())
        .unwrap()
        .with_fixed_now(t("2026-09-08T18:04:00Z"))
        .with_reports_dir(reports_dir("frame"));
    let empty = SaveReportRequest {
        summary_markdown: "  ".into(),
        model: "m".into(),
    };
    assert!(server.save_report(Parameters(empty)).await.is_err());

    let (report, _) = server.store_report(REPORT_BODY, "m").unwrap();
    let warning = report.markdown.find("**Old data.**").expect("stale frame");
    assert!(
        warning < report.markdown.find(REPORT_BODY).unwrap(),
        "the warning precedes the prose"
    );
}

#[tokio::test]
async fn report_tools_need_a_reports_directory() {
    assert!(full_server().get_report().await.is_err());
}

#[test]
fn freshness_without_a_report_asks_for_one() {
    let conn = full_cache();
    let now = t("2026-09-06T18:30:00Z");
    let f = reports::freshness(None, &swo_mcp::dashboard::build(&conn, now), now);
    assert!(!f.is_current && f.report_age_minutes.is_none());
}

// ---------------------------------------------------------------- local-model driver

#[test]
fn thinking_blocks_are_removed_from_model_output() {
    assert_eq!(
        ollama::strip_thinking("<think>hmm\nok</think>\n\n**Calm.**"),
        "**Calm.**"
    );
    assert_eq!(ollama::strip_thinking("no tags"), "no tags");
    assert_eq!(
        ollama::strip_thinking("</think> stray <think>"),
        "</think> stray <think>"
    );
}

#[tokio::test]
async fn chat_tools_are_read_only_and_all_dispatchable() {
    let server = full_server().with_reports_dir(reports_dir("chat"));
    let tools = server.chat_tools();
    assert!(tools
        .iter()
        .all(|t| t.name != "save_report" && t.name != "get_solar_wind"));
    for tool in &tools {
        let result = server
            .call_tool_json(&tool.name, serde_json::Value::Null)
            .await;
        assert!(
            result.is_ok(),
            "{} failed with no arguments: {result:?}",
            tool.name
        );
    }
    // Arguments sent as a JSON string, as some small models do.
    let v = server
        .call_tool_json("explain_reading", serde_json::json!("{\"reading\":\"bz\"}"))
        .await
        .unwrap();
    assert_eq!(v["entries"][0]["id"], "bz");
    assert!(server
        .call_tool_json("save_report", serde_json::json!({}))
        .await
        .is_err());
}

#[test]
fn the_footer_reports_the_age_of_real_time_data_not_of_monthly_products() {
    let now = t("2026-09-06T18:30:00Z");
    let mut d = swo_mcp::dashboard::build(&full_cache(), now);
    assert_eq!(d.realtime_retrieval_age_minutes, Some(26));
    // As in a live cache: solar wind just refreshed, the monthly solar cycle two days ago.
    d.realtime_retrieval_age_minutes = Some(2);
    d.oldest_retrieval_age_minutes = Some(2988);
    let md = reports::compose(&d, REPORT_BODY, "m", now).markdown;
    assert!(
        md.contains("real-time readings retrieved 2 minutes before this report"),
        "{md}"
    );
    assert!(md.contains("(slower-changing products up to 2 days before)"));
    assert!(!md.contains("the oldest was retrieved"));
}

// ---------------------------------------------------------------- client registration

#[test]
fn registering_merges_into_an_existing_config_and_is_idempotent() {
    use swo_mcp::register::{remove, upsert, Change, SERVER_NAME};
    let existing = r#"{ "mcpServers": { "other": { "command": "/bin/other", "args": ["x"] } }, "preferences": { "a": 1 } }"#;

    let (text, change) = upsert(Some(existing), "/usr/local/bin/swo-mcp").unwrap();
    assert_eq!(change, Change::Added);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        v["mcpServers"][SERVER_NAME]["command"],
        "/usr/local/bin/swo-mcp"
    );
    assert_eq!(
        v["mcpServers"]["other"]["args"][0], "x",
        "other servers are kept"
    );
    assert_eq!(v["preferences"]["a"], 1, "other settings are kept");

    assert_eq!(
        upsert(Some(&text), "/usr/local/bin/swo-mcp").unwrap().1,
        Change::AlreadyCurrent
    );
    assert_eq!(
        upsert(Some(&text), "/elsewhere/swo-mcp").unwrap().1,
        Change::Updated
    );

    let (text, change) = remove(Some(&text)).unwrap();
    assert_eq!(change, Change::Removed);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert!(v["mcpServers"].get(SERVER_NAME).is_none() && v["mcpServers"].get("other").is_some());
    assert_eq!(remove(Some(&text)).unwrap().1, Change::NotPresent);

    // No file yet.
    let (text, change) = upsert(None, "/x").unwrap();
    assert_eq!(change, Change::Added);
    assert!(text.contains(SERVER_NAME));
}

#[test]
fn a_config_that_is_not_valid_json_is_refused_not_repaired() {
    use swo_mcp::register::upsert;
    assert!(upsert(Some("{ not json"), "/x").is_err());
    assert!(upsert(Some("[1, 2]"), "/x").is_err());
    assert!(upsert(Some(r#"{ "mcpServers": [] }"#), "/x").is_err());
}

#[test]
fn editing_a_config_file_backs_it_up_first_and_skips_no_op_writes() {
    use swo_mcp::register::{edit_file, upsert, Change};
    let path = reports_dir("register").join("claude_desktop_config.json");
    let original = r#"{"mcpServers":{"other":{"command":"/bin/other"}}}"#;
    std::fs::write(&path, original).unwrap();

    let (change, backup) = edit_file(&path, "STAMP", |c| upsert(c, "/x/swo-mcp")).unwrap();
    assert_eq!(change, Change::Added);
    assert_eq!(
        std::fs::read_to_string(backup.unwrap()).unwrap(),
        original,
        "the backup is the untouched original"
    );

    let (change, backup) = edit_file(&path, "STAMP2", |c| upsert(c, "/x/swo-mcp")).unwrap();
    assert_eq!(change, Change::AlreadyCurrent);
    assert!(
        backup.is_none(),
        "nothing changed, so nothing was written or backed up"
    );

    std::fs::write(&path, "{ broken").unwrap();
    assert!(edit_file(&path, "STAMP3", |c| upsert(c, "/x/swo-mcp")).is_err());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "{ broken",
        "a broken file is left exactly as it was"
    );
}

#[tokio::test]
async fn a_server_started_before_the_cache_exists_explains_itself_then_recovers() {
    let db = reports_dir("lazy").join("observatory.sqlite3");
    let server = SwoServer::lazy(&db);
    let err = server.get_dashboard().await.err().unwrap();
    assert!(err.contains("Open the desktop app"), "{err}");
    // What a reading *is* needs no data; only the current value is missing.
    let req = ExplainRequest {
        reading: Some("kp".into()),
    };
    let explained = server.explain_reading(Parameters(req)).await.unwrap().0;
    assert!(explained.entries[0].id == "kp_index" && explained.current.is_none());

    // The desktop app runs for the first time; the same server picks the cache up.
    full_cache()
        .execute("VACUUM INTO ?1", [db.to_str().unwrap()])
        .unwrap();
    assert!(server.get_dashboard().await.unwrap().0.solar_wind.is_some());
}
