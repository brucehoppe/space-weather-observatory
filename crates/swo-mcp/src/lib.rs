//! Read-only MCP server over the Space Weather Observatory cache.
//!
//! Design rules, matching the desktop app:
//! - Separate process. The app itself still never loads a language model.
//! - Read-only. The SQLite cache is opened with `SQLITE_OPEN_READ_ONLY`; the
//!   app remains the only writer (the cache is already in WAL mode, so reads
//!   never block its refreshes).
//! - No network. Every answer comes from snapshots the app already retrieved.
//! - All parsing goes through `swo-core`, so the model sees the same numbers,
//!   quality flags and provenance the app shows.
//! - The only thing this crate writes is plain-language reports, in a
//!   `reports/` directory beside the cache (see `reports.rs`).

pub mod dashboard;
pub mod detail;
pub mod glossary;
pub mod ollama;
pub mod prompts;
pub mod register;
pub mod reports;

use chrono::{DateTime, Duration, SecondsFormat, Utc};
use rmcp::{
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{Implementation, PromptMessage, Role, ServerCapabilities, ServerConfig},
    prompt, prompt_handler, prompt_router, tool, tool_handler, tool_router, Json, ServerHandler,
};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use swo_core::model::{Observation, Quality};
use swo_core::parse::rtsw;

/// Cache schema this server understands (see `src-tauri/src/store.rs`).
pub const SUPPORTED_SCHEMA: u32 = 1;

/// Product keys, identical to `Product::key()` in `src-tauri/src/providers.rs`.
pub const PRODUCTS: &[&str] = &[
    "rtsw_wind_1m",
    "rtsw_mag_1m",
    "goes_xrays_1day",
    "goes_instrument_sources",
    "planetary_k_index",
    "planetary_k_index_forecast",
    "noaa_scales",
    "alerts",
    "ovation_aurora_latest",
    "three_day_forecast",
    "three_day_geomag_forecast",
    "solar_cycle_observed",
    "solar_cycle_predicted",
];

const INSTRUCTIONS: &str = "\
Read-only access to the Space Weather Observatory's local cache of NOAA SWPC products. \
Every value carries provider provenance (source_url, retrieved_at, payload_sha256) and a \
quality state. When you state a number, cite its product and retrieved_at. Treat samples \
with quality 'missing' as absent, never as zero. Say when data is stale (see age_minutes). \
Do not present observations as forecasts or invent values the tools did not return. \
Start with get_dashboard for the current readings. Use explain_reading before explaining what \
a reading means, and reword its text rather than explaining from memory. To keep the saved \
plain-language report up to date, call get_report; when it is not current, write a new one \
from get_dashboard and store it with save_report.";

// ---------------------------------------------------------------- store access

#[derive(Debug, Clone)]
pub(crate) struct Snapshot {
    pub(crate) source_url: String,
    pub(crate) retrieved_at: DateTime<Utc>,
    pub(crate) sha256: String,
    pub(crate) payload: String,
}

fn parse_time(raw: &str) -> Result<DateTime<Utc>, String> {
    DateTime::parse_from_rfc3339(raw)
        .map(|t| t.with_timezone(&Utc))
        .map_err(|e| format!("invalid RFC 3339 time {raw:?}: {e}"))
}

pub(crate) fn rfc3339(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Newest snapshot of `product` retrieved at or before `at` (or newest overall).
pub(crate) fn snapshot_at(
    conn: &Connection,
    product: &str,
    at: Option<DateTime<Utc>>,
) -> Result<Option<Snapshot>, String> {
    let row = |r: &rusqlite::Row| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    };
    let found = match at {
        None => conn
            .query_row(
                "SELECT source_url, retrieved_at, sha256, payload FROM snapshots
                 WHERE product = ?1
                 ORDER BY datetime(retrieved_at) DESC, id DESC LIMIT 1",
                params![product],
                row,
            )
            .optional(),
        Some(t) => conn
            .query_row(
                "SELECT source_url, retrieved_at, sha256, payload FROM snapshots
                 WHERE product = ?1 AND datetime(retrieved_at) <= datetime(?2)
                 ORDER BY datetime(retrieved_at) DESC, id DESC LIMIT 1",
                params![product, t.to_rfc3339()],
                row,
            )
            .optional(),
    }
    .map_err(|e| format!("cache query failed: {e}"))?;

    found
        .map(|(source_url, retrieved, sha256, payload)| {
            Ok(Snapshot {
                source_url,
                retrieved_at: parse_time(&retrieved)?,
                sha256,
                payload,
            })
        })
        .transpose()
}

// ---------------------------------------------------------------- tool I/O types

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProductStatus {
    /// Product key, e.g. `rtsw_wind_1m`.
    pub product: String,
    /// False when the cache holds no snapshot of this product yet.
    pub available: bool,
    pub source_url: Option<String>,
    /// When the app retrieved the newest snapshot (not a provider timestamp).
    pub retrieved_at: Option<String>,
    /// Minutes between `retrieved_at` and now.
    pub age_minutes: Option<i64>,
    /// Snapshots retained for replay.
    pub snapshot_count: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ProductList {
    pub now: String,
    pub products: Vec<ProductStatus>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct SolarWindRequest {
    /// Minutes of data to return, ending at the newest sample. Default 120, max 1440.
    pub window_minutes: Option<u32>,
    /// Replay: use the snapshots retrieved at or before this RFC 3339 time.
    /// Omit for the latest data.
    pub at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Sample {
    /// Provider timestamp (UTC).
    pub time: String,
    /// Absent when the provider supplied no usable value.
    pub value: Option<f64>,
    /// `good`, `suspect` or `missing`.
    pub quality: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SourceRef {
    pub product: String,
    pub source_url: String,
    pub retrieved_at: String,
    pub payload_sha256: String,
    /// Spacecraft the provider marks as the active real-time stream.
    pub spacecraft: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Stat {
    pub latest: Option<f64>,
    pub latest_time: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    /// Samples in the window with no usable value.
    pub missing: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SolarWindSummary {
    pub speed_km_s: Stat,
    pub density_per_cm3: Stat,
    pub bt_nt: Stat,
    pub bz_gsm_nt: Stat,
    /// Minutes in the window with Bz (GSM) below zero (southward).
    pub bz_southward_minutes: usize,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SolarWindReport {
    pub window_start: String,
    pub window_end: String,
    /// Minutes between the newest sample and now; large values mean stale data.
    pub age_minutes: i64,
    pub sources: Vec<SourceRef>,
    pub summary: SolarWindSummary,
    pub speed_km_s: Vec<Sample>,
    pub density_per_cm3: Vec<Sample>,
    pub bt_nt: Vec<Sample>,
    /// Bz in GSM, the frame relevant to coupling with Earth's field.
    pub bz_gsm_nt: Vec<Sample>,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExplainRequest {
    /// Reading to explain: an id such as `bz`, `kp_index`, `xray_flux`, `aurora`,
    /// or the name shown on the dashboard. Omit to get every explanation.
    pub reading: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Explanation {
    pub glossary_version: String,
    /// Reviewed explanations. Reword these for the reader; do not contradict them.
    pub entries: Vec<glossary::Entry>,
    /// The reading's current values from the dashboard, when one reading was asked for.
    pub current: Option<serde_json::Value>,
    /// Every id `explain_reading` accepts.
    pub available_ids: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct ReportStatus {
    /// The latest saved report, if any.
    pub report: Option<reports::StoredReport>,
    pub freshness: reports::Freshness,
    /// What to do next.
    pub next_step: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SaveReportRequest {
    /// The plain-language report body in markdown, without a title. The server
    /// adds the title, the readings table and the provenance footer itself.
    pub summary_markdown: String,
    /// Name of the model that wrote it, e.g. `qwen3:8b`.
    pub model: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct SavedReport {
    pub path: String,
    pub headline: String,
    pub generated_at: String,
    pub data_fingerprint: String,
}

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct ExplainPromptArgs {
    /// Reading to explain (e.g. `bz`). Omit to walk through the whole dashboard.
    pub reading: Option<String>,
}

// ---------------------------------------------------------------- helpers

fn quality_label(q: Quality) -> &'static str {
    match q {
        Quality::Good => "good",
        Quality::Suspect => "suspect",
        Quality::Missing => "missing",
    }
}

/// Observations within `[start, end]`, oldest first.
fn window(obs: &[Observation], start: DateTime<Utc>, end: DateTime<Utc>) -> Vec<Observation> {
    let mut v: Vec<Observation> = obs
        .iter()
        .filter(|o| o.time >= start && o.time <= end)
        .cloned()
        .collect();
    v.sort_by_key(|o| o.time);
    v
}

fn samples(obs: &[Observation]) -> Vec<Sample> {
    obs.iter()
        .map(|o| Sample {
            time: rfc3339(o.time),
            value: o.accepted(),
            quality: quality_label(o.quality).to_string(),
        })
        .collect()
}

fn stat(obs: &[Observation]) -> Stat {
    let vals: Vec<(DateTime<Utc>, f64)> = obs
        .iter()
        .filter_map(|o| o.accepted().map(|v| (o.time, v)))
        .collect();
    let round = |x: f64| (x * 100.0).round() / 100.0;
    Stat {
        latest: vals.last().map(|v| v.1),
        latest_time: vals.last().map(|v| rfc3339(v.0)),
        min: vals.iter().map(|v| v.1).reduce(f64::min),
        max: vals.iter().map(|v| v.1).reduce(f64::max),
        mean: (!vals.is_empty())
            .then(|| round(vals.iter().map(|v| v.1).sum::<f64>() / vals.len() as f64)),
        missing: obs.len() - vals.len(),
    }
}

fn newest_time(obs: &[Observation]) -> Option<DateTime<Utc>> {
    obs.iter()
        .filter(|o| o.accepted().is_some())
        .map(|o| o.time)
        .max()
}

// ---------------------------------------------------------------- server

/// A locked, open cache connection.
struct CacheGuard<'a>(std::sync::MutexGuard<'a, Option<Connection>>);

impl std::ops::Deref for CacheGuard<'_> {
    type Target = Connection;
    fn deref(&self) -> &Connection {
        self.0
            .as_ref()
            .expect("CacheGuard is only built around an open connection")
    }
}

#[derive(Clone)]
pub struct SwoServer {
    /// `None` until the cache has been opened (see [`SwoServer::lazy`]).
    conn: Arc<Mutex<Option<Connection>>>,
    /// Cache to open on first use, when serving before the cache exists.
    db: Option<PathBuf>,
    /// Fixed clock for deterministic tests; `None` reads the system clock.
    fixed_now: Option<DateTime<Utc>>,
    /// Where reports are saved; `None` disables the report tools.
    reports: Option<reports::ReportStore>,
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
}

impl SwoServer {
    fn connect(db: &Path) -> Result<Connection, String> {
        if !db.exists() {
            return Err(format!(
                "The Space Weather Observatory cache was not found at {}. Open the desktop app once \
                 so it can download data, then try again.",
                db.display()
            ));
        }
        Connection::open_with_flags(
            db,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("cannot open cache {}: {e}", db.display()))
    }

    /// Open the app's cache read-only and check its schema version.
    pub fn open(db: &Path) -> Result<Self, String> {
        Self::from_connection(Self::connect(db)?)
    }

    /// A server that opens the cache on first use. An MCP client starts the
    /// server at launch, possibly before the desktop app has ever run; failing
    /// then would show the user a bare "connection closed". This way the server
    /// starts, and each tool explains what is missing until the cache appears.
    pub fn lazy(db: &Path) -> Self {
        match Self::open(db) {
            Ok(server) => server,
            Err(_) => Self {
                conn: Arc::new(Mutex::new(None)),
                db: Some(db.to_path_buf()),
                fixed_now: None,
                reports: None,
                tool_router: Self::tool_router() + Self::report_tool_router(),
                prompt_router: Self::prompt_router(),
            },
        }
    }

    /// The open cache, opening it now if it was not available at startup.
    fn cache(&self) -> Result<CacheGuard<'_>, String> {
        let mut guard = self.conn.lock().map_err(|_| "cache lock poisoned")?;
        if guard.is_none() {
            let db = self.db.as_deref().ok_or("no cache configured")?;
            let conn = Self::connect(db)?;
            Self::check_schema(&conn)?;
            *guard = Some(conn);
        }
        Ok(CacheGuard(guard))
    }

    pub fn from_connection(conn: Connection) -> Result<Self, String> {
        Self::check_schema(&conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(Some(conn))),
            db: None,
            fixed_now: None,
            reports: None,
            tool_router: Self::tool_router() + Self::report_tool_router(),
            prompt_router: Self::prompt_router(),
        })
    }

    fn check_schema(conn: &Connection) -> Result<(), String> {
        let version: Option<String> = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| format!("not an Observatory cache: {e}"))?;
        match version.as_deref().and_then(|v| v.parse::<u32>().ok()) {
            Some(SUPPORTED_SCHEMA) => {}
            other => {
                return Err(format!(
                    "cache schema {other:?} not supported (expected {SUPPORTED_SCHEMA})"
                ))
            }
        }
        Ok(())
    }

    pub fn with_fixed_now(mut self, now: DateTime<Utc>) -> Self {
        self.fixed_now = Some(now);
        self
    }

    pub fn with_reports_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.reports = Some(reports::ReportStore::new(dir));
        self
    }

    pub fn now(&self) -> DateTime<Utc> {
        self.fixed_now.unwrap_or_else(Utc::now)
    }

    /// Every dashboard reading from the newest cached snapshots.
    pub fn dashboard(&self) -> Result<dashboard::Dashboard, String> {
        let conn = self.cache()?;
        Ok(dashboard::build(&conn, self.now()))
    }

    fn stale_addendum(&self) -> String {
        let warning = self.dashboard().ok().and_then(|d| d.stale_data_warning);
        prompts::stale_addendum(warning.as_deref())
    }

    pub fn report_store(&self) -> Result<&reports::ReportStore, String> {
        self.reports.as_ref().ok_or_else(|| {
            "no reports directory configured (start the server with --reports DIR)".into()
        })
    }

    /// Frame a model's prose with the computed readings table and save it.
    pub fn store_report(
        &self,
        body: &str,
        model: &str,
    ) -> Result<(reports::StoredReport, PathBuf), String> {
        if body.trim().len() < 40 {
            return Err("the report body is empty or too short to be a report".into());
        }
        let report = reports::compose(&self.dashboard()?, body, model, self.now());
        let path = self.report_store()?.save(&report)?;
        Ok((report, path))
    }
}

#[tool_router(router = tool_router)]
impl SwoServer {
    /// Which products are cached, how fresh each is, and how many replay snapshots exist.
    #[tool(
        name = "list_products",
        description = "List the NOAA SWPC products in the local cache with their source URL, \
                       retrieval time, age in minutes and number of replay snapshots. \
                       Call this first to check what data is available and whether it is stale."
    )]
    pub async fn list_products(&self) -> Result<Json<ProductList>, String> {
        let now = self.now();
        let conn = self.cache()?;
        let mut products = Vec::with_capacity(PRODUCTS.len());
        for &product in PRODUCTS {
            let latest = snapshot_at(&conn, product, None)?;
            let snapshot_count: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM snapshots WHERE product = ?1",
                    params![product],
                    |r| r.get(0),
                )
                .map_err(|e| format!("cache query failed: {e}"))?;
            products.push(ProductStatus {
                product: product.to_string(),
                available: latest.is_some(),
                source_url: latest.as_ref().map(|s| s.source_url.clone()),
                retrieved_at: latest.as_ref().map(|s| rfc3339(s.retrieved_at)),
                age_minutes: latest
                    .as_ref()
                    .map(|s| (now - s.retrieved_at).num_minutes()),
                snapshot_count,
            });
        }
        Ok(Json(ProductList {
            now: rfc3339(now),
            products,
        }))
    }

    /// Solar-wind plasma and magnetic field from the active real-time spacecraft.
    #[tool(
        name = "get_solar_wind",
        description = "Solar-wind speed, density, total field Bt and Bz (GSM) at 1-minute \
                       cadence from the active real-time spacecraft, with a summary \
                       (latest/min/max/mean, minutes of southward Bz) and full provenance. \
                       Optional `at` replays the snapshots retrieved at or before that time."
    )]
    pub async fn get_solar_wind(
        &self,
        Parameters(req): Parameters<SolarWindRequest>,
    ) -> Result<Json<SolarWindReport>, String> {
        let minutes = i64::from(req.window_minutes.unwrap_or(120).clamp(1, 1440));
        let at = req.at.as_deref().map(parse_time).transpose()?;

        let (wind_snap, mag_snap) = {
            let conn = self.cache()?;
            (
                snapshot_at(&conn, "rtsw_wind_1m", at)?,
                snapshot_at(&conn, "rtsw_mag_1m", at)?,
            )
        };
        let wind_snap = wind_snap.ok_or("no rtsw_wind_1m snapshot in the cache for that time")?;
        let mag_snap = mag_snap.ok_or("no rtsw_mag_1m snapshot in the cache for that time")?;

        let winds = rtsw::parse_wind(&wind_snap.payload).map_err(|e| e.to_string())?;
        let mags = rtsw::parse_mag(&mag_snap.payload).map_err(|e| e.to_string())?;
        let wind = rtsw::active_wind(&winds).ok_or("provider marks no plasma stream active")?;
        let mag = rtsw::active_mag(&mags).ok_or("provider marks no magnetometer stream active")?;

        let end = [newest_time(&wind.speed_km_s), newest_time(&mag.bz_gsm_nt)]
            .into_iter()
            .flatten()
            .max()
            .ok_or("active streams contain no usable samples")?;
        let start = end - Duration::minutes(minutes);

        let speed = window(&wind.speed_km_s, start, end);
        let density = window(&wind.density_per_cm3, start, end);
        let bt = window(&mag.bt_nt, start, end);
        let bz = window(&mag.bz_gsm_nt, start, end);

        let now = at.unwrap_or_else(|| self.now());
        let source = |product: &str, s: &Snapshot, craft: &str| SourceRef {
            product: product.to_string(),
            source_url: s.source_url.clone(),
            retrieved_at: rfc3339(s.retrieved_at),
            payload_sha256: s.sha256.clone(),
            spacecraft: craft.to_string(),
        };

        Ok(Json(SolarWindReport {
            window_start: rfc3339(start),
            window_end: rfc3339(end),
            age_minutes: (now - end).num_minutes(),
            sources: vec![
                source("rtsw_wind_1m", &wind_snap, &wind.spacecraft),
                source("rtsw_mag_1m", &mag_snap, &mag.spacecraft),
            ],
            summary: SolarWindSummary {
                speed_km_s: stat(&speed),
                density_per_cm3: stat(&density),
                bt_nt: stat(&bt),
                bz_gsm_nt: stat(&bz),
                bz_southward_minutes: bz
                    .iter()
                    .filter(|o| o.accepted().is_some_and(|v| v < 0.0))
                    .count(),
            },
            speed_km_s: samples(&speed),
            density_per_cm3: samples(&density),
            bt_nt: samples(&bt),
            bz_gsm_nt: samples(&bz),
        }))
    }
}

#[tool_router(router = report_tool_router)]
impl SwoServer {
    #[tool(
        name = "get_dashboard",
        description = "Every reading on the Space Weather Observatory dashboard in one call: solar wind \
                       (speed, density, Bt, Bz), X-ray flux and flare class, Kp index, NOAA G/R/S scales, \
                       active NOAA bulletins, the 3-day forecast, the aurora model and solar-cycle \
                       progress, plus the app's own plain-language statements, data age and a \
                       data_fingerprint. Start here to analyse conditions or write a report."
    )]
    pub async fn get_dashboard(&self) -> Result<Json<dashboard::Dashboard>, String> {
        self.dashboard().map(Json)
    }

    #[tool(
        name = "get_kp",
        description = "Planetary Kp index per 3-hour interval, with observed values, NOAA estimates and \
                       NOAA forecasts kept in three separate lists (never merged). `hours_back` \
                       (default 24) and `hours_ahead` (default 72) set the span; `at` replays an \
                       earlier snapshot."
    )]
    pub async fn get_kp(
        &self,
        Parameters(req): Parameters<detail::KpRequest>,
    ) -> Result<Json<detail::KpReport>, String> {
        let at = req.at.as_deref().map(parse_time).transpose()?;
        let conn = self.cache()?;
        detail::kp_report(&conn, &req, at.unwrap_or_else(|| self.now()), at).map(Json)
    }

    #[tool(
        name = "get_xray_flares",
        description = "Solar X-ray flare activity from the GOES long band over the product's 24 hours: \
                       latest class, background level, peak, and each period the flux stayed at or \
                       above `min_class` (A, B, C, M or X; default C), classified with the app's flux \
                       classifier. `at` replays an earlier snapshot."
    )]
    pub async fn get_xray_flares(
        &self,
        Parameters(req): Parameters<detail::FlaresRequest>,
    ) -> Result<Json<detail::FlaresReport>, String> {
        let at = req.at.as_deref().map(parse_time).transpose()?;
        let conn = self.cache()?;
        detail::flares_report(&conn, &req, at.unwrap_or_else(|| self.now()), at).map(Json)
    }

    #[tool(
        name = "get_interpretation",
        description = "The app's reviewed plain-language statements for current conditions and NOAA's \
                       forecast days: what each published G/R/S level means for aurora viewing, HF \
                       radio, GNSS, satellites and power, with NOAA's geographic qualifiers, plus \
                       the solar-wind note. Each carries its basis, rule version and source ref. \
                       Use these as the 'what it means' of a report."
    )]
    pub async fn get_interpretation(&self) -> Result<Json<detail::Interpretation>, String> {
        let now = self.now();
        let conn = self.cache()?;
        let (_, statements) = dashboard::build_with_statements(&conn, now);
        Ok(Json(detail::interpretation(statements, now)))
    }

    #[tool(
        name = "explain_reading",
        description = "Reviewed plain-language explanation of a dashboard reading: what it is, how to \
                       read the number, why it matters and its limits, with the current value. \
                       Pass `reading` (e.g. bz, kp_index, xray_flux, noaa_scales, aurora) or omit it \
                       for all readings. Use this instead of explaining from memory."
    )]
    pub async fn explain_reading(
        &self,
        Parameters(req): Parameters<ExplainRequest>,
    ) -> Result<Json<Explanation>, String> {
        let available_ids = glossary::ids().into_iter().map(String::from).collect();
        let query = req
            .reading
            .as_deref()
            .map(str::trim)
            .filter(|q| !q.is_empty() && *q != "all");
        let Some(query) = query else {
            return Ok(Json(Explanation {
                glossary_version: glossary::GLOSSARY_VERSION.into(),
                entries: glossary::all(),
                current: None,
                available_ids,
            }));
        };
        let entry = glossary::find(query).ok_or_else(|| {
            format!(
                "no reading matches {query:?}; valid ids: {}",
                glossary::ids().join(", ")
            )
        })?;
        // `dashboard_field` is `panel` or `panel.measure`; walk it through the JSON.
        // The explanation itself needs no data, so a missing cache only costs the current value.
        let current = self
            .dashboard()
            .ok()
            .and_then(|d| serde_json::to_value(d).ok())
            .and_then(|board| {
                entry
                    .dashboard_field
                    .split('.')
                    .try_fold(&board, |v, key| v.get(key))
                    .filter(|v| !v.is_null())
                    .cloned()
            });
        Ok(Json(Explanation {
            glossary_version: glossary::GLOSSARY_VERSION.into(),
            entries: vec![entry],
            current,
            available_ids,
        }))
    }

    #[tool(
        name = "get_report",
        description = "The latest saved plain-language space weather report, and whether it is still \
                       current. It is out of date when the cached data changed after it was written or \
                       it is more than 6 hours old. When not current, write a new one with save_report."
    )]
    pub async fn get_report(&self) -> Result<Json<ReportStatus>, String> {
        let report = self.report_store()?.latest()?;
        let freshness = reports::freshness(report.as_ref(), &self.dashboard()?, self.now());
        let next_step = if freshness.is_current {
            "The report is current. Show it to the user; no update is needed."
        } else {
            "The report is out of date. Call get_dashboard, write a new plain-language report from it, \
             and store it with save_report."
        };
        Ok(Json(ReportStatus {
            report,
            freshness,
            next_step: next_step.into(),
        }))
    }

    #[tool(
        name = "save_report",
        description = "Save a new plain-language report written from get_dashboard. Pass only the prose \
                       (markdown, no title): the server adds the title, a readings table computed \
                       from the data, and the provenance footer, then records the data_fingerprint so \
                       get_report can tell when it goes out of date."
    )]
    pub async fn save_report(
        &self,
        Parameters(req): Parameters<SaveReportRequest>,
    ) -> Result<Json<SavedReport>, String> {
        let (report, path) = self.store_report(&req.summary_markdown, &req.model)?;
        Ok(Json(SavedReport {
            path: path.display().to_string(),
            headline: report.headline,
            generated_at: report.generated_at,
            data_fingerprint: report.data_fingerprint,
        }))
    }
}

impl SwoServer {
    /// Tools offered to the built-in Ollama chat: everything read-only and small
    /// enough for a local model's context (so not the raw 1-minute samples).
    pub fn chat_tools(&self) -> Vec<rmcp::model::Tool> {
        const HIDDEN: &[&str] = &["get_solar_wind", "save_report"];
        self.tool_router
            .list_all()
            .into_iter()
            .filter(|t| !HIDDEN.contains(&t.name.as_ref()))
            .collect()
    }

    /// Call a tool by name with JSON arguments, as the MCP transport would.
    pub async fn call_tool_json(
        &self,
        name: &str,
        args: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        // Small models send `null` or a JSON string where an object is expected.
        let args = match args {
            serde_json::Value::String(s) => {
                serde_json::from_str(&s).unwrap_or(serde_json::json!({}))
            }
            serde_json::Value::Null => serde_json::json!({}),
            other => other,
        };
        fn params<T: serde::de::DeserializeOwned>(
            args: serde_json::Value,
        ) -> Result<Parameters<T>, String> {
            serde_json::from_value(args)
                .map(Parameters)
                .map_err(|e| format!("invalid arguments: {e}"))
        }
        fn out<T: Serialize>(r: Json<T>) -> Result<serde_json::Value, String> {
            serde_json::to_value(r.0).map_err(|e| e.to_string())
        }
        match name {
            "list_products" => out(self.list_products().await?),
            "get_dashboard" => out(self.get_dashboard().await?),
            "get_kp" => out(self.get_kp(params(args)?).await?),
            "get_xray_flares" => out(self.get_xray_flares(params(args)?).await?),
            "get_interpretation" => out(self.get_interpretation().await?),
            "explain_reading" => out(self.explain_reading(params(args)?).await?),
            "get_report" => out(self.get_report().await?),
            other => Err(format!("unknown tool {other:?}")),
        }
    }
}

#[prompt_router]
impl SwoServer {
    /// Write a plain-language space weather report for a general reader and save it.
    #[prompt(name = "layman_report")]
    pub async fn layman_report(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "{}{}\n\nCall get_dashboard, write the report from what it returns, then store it with \
                 save_report (pass only the prose; the server adds the title and readings table).",
                prompts::REPORT_RULES,
                self.stale_addendum()
            ),
        )]
    }

    /// Check whether the saved report is still current and rewrite it only if it is not.
    #[prompt(name = "update_report")]
    pub async fn update_report(&self) -> Vec<PromptMessage> {
        vec![PromptMessage::new_text(
            Role::User,
            format!(
                "Call get_report. If freshness.is_current is true, show the saved report and stop. \
                 Otherwise call get_dashboard, write a new report following the rules below, store it \
                 with save_report, and say briefly what changed since the previous report.\n\n{}{}",
                prompts::REPORT_RULES,
                self.stale_addendum()
            ),
        )]
    }

    /// Explain one dashboard reading, or walk through all of them, in everyday language.
    #[prompt(name = "explain_dashboard")]
    pub async fn explain_dashboard(
        &self,
        Parameters(args): Parameters<ExplainPromptArgs>,
    ) -> Vec<PromptMessage> {
        let target = match args.reading.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
            Some(r) => format!("Explain the dashboard reading {r:?}. Call explain_reading with that reading."),
            None => "Explain every reading on the dashboard, one short paragraph each. Call explain_reading \
                     with no arguments for the explanations and get_dashboard for the current values."
                .to_string(),
        };
        vec![PromptMessage::new_text(
            Role::User,
            format!("{target}\n\n{}", prompts::EXPLAIN_RULES),
        )]
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for SwoServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .build(),
        )
        .with_server_info(
            Implementation::new(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
                .with_title("Space Weather Observatory (read-only cache)"),
        )
        .with_instructions(INSTRUCTIONS)
    }
}
