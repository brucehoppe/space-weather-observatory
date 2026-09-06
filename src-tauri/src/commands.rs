//! The command surface exposed to the webview.
//!
//! Every command is narrowly typed. Nothing here accepts a URL, a shell string
//! or an arbitrary path from the UI: acquisition targets come from the
//! `Product` enum, and export paths come from a native save dialog.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use swo_core::alert::{self, AlertMemory, AlertSettings, Evaluation, SettingsError};
use swo_core::export::{self, ExportMetadata, SeriesMetadata};
use swo_core::model::Series;
use swo_core::parse::ovation::AuroraGrid;
use tauri::{AppHandle, Manager, State};

use crate::demo::{self, AlertScenario};
use crate::imagery::{self, Passband, SunImage};
use crate::providers::Product;
use crate::settings::Settings;
use crate::snapshot::{Dashboard, Mode};
use crate::AppState;

type CmdResult<T> = Result<T, String>;

// --- Dashboard ---------------------------------------------------------------

#[tauri::command]
pub async fn get_dashboard(state: State<'_, AppState>) -> CmdResult<Dashboard> {
    Ok(state.dashboard().await)
}

/// Refresh one product, or all of them when `product` is `None`.
#[tauri::command]
pub async fn refresh(state: State<'_, AppState>, product: Option<String>) -> CmdResult<Dashboard> {
    match product {
        Some(key) => {
            let p = Product::ALL
                .into_iter()
                .find(|p| p.key() == key)
                .ok_or_else(|| format!("unknown product {key}"))?;
            refresh_product(&state, p).await;
        }
        None => refresh_all(&state).await,
    }
    Ok(state.dashboard().await)
}

/// Fetch one product and store it. Failures are recorded, never fatal: the
/// previous snapshot stays in place as last-known-good.
pub async fn refresh_product(state: &AppState, product: Product) {
    let now = Utc::now();
    if !state.fetcher.may_attempt(product, now).await {
        return; // still backing off
    }
    let Ok(outcome) = state.fetcher.fetch(product, now).await else {
        return;
    };
    let stored = {
        let store = state.store.lock().await;
        store.put_snapshot(
            product.key(),
            product.url(),
            outcome.retrieved_at,
            &outcome.body,
        )
    };
    if let Ok(snapshot) = stored {
        state
            .live
            .write()
            .await
            .insert(product.key().to_string(), snapshot);
    }
}

pub async fn refresh_all(state: &AppState) {
    for product in Product::ALL {
        refresh_product(state, product).await;
    }
    let settings = state.settings.read().await;
    let store = state.store.lock().await;
    let _ = store.enforce_limits(
        settings.snapshot_retention,
        settings.cache_limit_mb * 1024 * 1024,
    );
}

/// Background polling at each product's own documented cadence.
pub fn start_polling(handle: AppHandle) {
    for product in Product::ALL {
        let handle = handle.clone();
        tauri::async_runtime::spawn(async move {
            let period = std::time::Duration::from_secs(product.poll_seconds() as u64);
            loop {
                tokio::time::sleep(period).await;
                let state = handle.state::<AppState>();
                // Replay must not drive live acquisition, but live acquisition
                // continues so returning to now is immediate.
                refresh_product(&state, product).await;
            }
        });
    }
}

// --- Settings ----------------------------------------------------------------

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    Ok(state.settings.read().await.clone())
}

fn describe(e: SettingsError) -> String {
    match e {
        SettingsError::ThresholdOutOfRange => "Threshold must be between 200 and 3000 km/s.".into(),
        SettingsError::PersistenceOutOfRange => {
            "Persistence must be between 1 and 720 minutes.".into()
        }
        SettingsError::HysteresisOutOfRange => {
            "Hysteresis must be between 0 and 200 km/s and below the threshold.".into()
        }
        SettingsError::ClearanceOutOfRange => "Clearance must be between 1 and 720 minutes.".into(),
        SettingsError::CadenceOutOfRange => "Cadence must be between 1 and 3600 seconds.".into(),
        SettingsError::StaleWindowOutOfRange => {
            "Stale window must be between 1 and 1440 minutes.".into()
        }
        SettingsError::NonFinite => "Values must be real numbers.".into(),
    }
}

/// Save settings. Invalid values are rejected with an explanation rather than
/// silently coerced (spec §13B).
#[tauri::command]
pub async fn save_settings(
    state: State<'_, AppState>,
    mut settings: Settings,
) -> CmdResult<Settings> {
    settings.alert.validate().map_err(describe)?;
    let previous = state.settings.read().await.clone();
    // A changed alert configuration bumps the version, which resets the pending
    // evaluation window and stamps future episodes with the new configuration.
    if previous.alert != settings.alert {
        settings.alert.settings_version = previous.alert.settings_version.saturating_add(1);
    }
    settings
        .save(&state.config_dir)
        .map_err(|e| e.to_string())?;
    *state.settings.write().await = settings.clone();
    Ok(settings)
}

#[tauri::command]
pub async fn reset_alert_settings(state: State<'_, AppState>) -> CmdResult<Settings> {
    let mut settings = state.settings.read().await.clone();
    let version = settings.alert.settings_version.saturating_add(1);
    settings.alert = AlertSettings {
        settings_version: version,
        ..AlertSettings::default()
    };
    settings
        .save(&state.config_dir)
        .map_err(|e| e.to_string())?;
    *state.settings.write().await = settings.clone();
    Ok(settings)
}

// --- Solar-wind alert --------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertView {
    pub evaluation: Evaluation,
    pub settings: AlertSettings,
    /// `live`, `replay` or `demo`. Replay and demo banners are labelled and
    /// never touch live history.
    pub context: Mode,
    /// Product and spacecraft the evaluated samples came from.
    pub source_product: String,
    pub source_spacecraft: Option<String>,
    /// Latest Bz with its own observation time, or `None` when unavailable.
    pub bz_gsm_nt: Option<f64>,
    pub bz_time: Option<DateTime<Utc>>,
    pub bz_stale: bool,
}

/// Evaluate the detector against the current dataset.
///
/// The chart crosshair never calls this: inspection time and live monitoring
/// time are separate, so inspecting an old reading cannot retarget the detector.
#[tauri::command]
pub async fn evaluate_alert(state: State<'_, AppState>) -> CmdResult<AlertView> {
    let settings = state.settings.read().await.alert.clone();
    let dashboard = state.dashboard().await;
    let source = state.wind_source().await;
    let now = Utc::now();

    let speed_key = "noaa-swpc:rtsw_wind_1m:proton_speed";
    let samples = dashboard
        .series
        .get(speed_key)
        .map(|s| s.samples.clone())
        .unwrap_or_default();

    let bz_series = dashboard.series.get("noaa-swpc:rtsw_mag_1m:bz_gsm");
    let bz_obs = bz_series.and_then(|s| s.last_accepted());
    let bz_stale = bz_obs
        .map(|o| (now - o.time) > Duration::minutes(settings.stale_after_minutes))
        .unwrap_or(true);

    let is_replay = state.replay.read().await.is_some();
    let evaluation = if is_replay {
        // Replay evaluates in its own state and is discarded afterwards.
        let mut replay = state.replay.write().await;
        let session = replay.as_mut().expect("checked above");
        let at = samples.last().map(|o| o.time).unwrap_or(now);
        let e = alert::evaluate(&samples, &settings, &session.memory, &source, at);
        session.memory = e.memory.clone();
        e
    } else {
        let prior = state.alert_memory.read().await.clone();
        let e = alert::evaluate(&samples, &settings, &prior, &source, now);
        *state.alert_memory.write().await = e.memory.clone();
        let store = state.store.lock().await;
        let _ = store.save_alert_memory(&e.memory);
        e
    };

    Ok(AlertView {
        evaluation,
        settings,
        context: dashboard.mode,
        source_product: source.product,
        source_spacecraft: source.spacecraft,
        bz_gsm_nt: if bz_stale {
            None
        } else {
            bz_obs.and_then(|o| o.value)
        },
        bz_time: bz_obs.map(|o| o.time),
        bz_stale,
    })
}

/// Dismiss an episode. Presentation only: it cannot clear a condition or
/// disable the detector.
#[tauri::command]
pub async fn acknowledge_episode(state: State<'_, AppState>, episode_id: String) -> CmdResult<()> {
    if let Some(session) = state.replay.write().await.as_mut() {
        alert::acknowledge(&mut session.memory, &episode_id);
        return Ok(());
    }
    let mut memory = state.alert_memory.write().await;
    alert::acknowledge(&mut memory, &episode_id);
    let store = state.store.lock().await;
    store.save_alert_memory(&memory).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn get_alert_scenarios() -> Vec<AlertScenario> {
    demo::alert_scenarios()
}

/// Run one labelled synthetic scenario through the real evaluator, stepping
/// through it the way the running application would. Used by the demonstration
/// view and by the release evidence.
#[tauri::command]
pub async fn evaluate_scenario(
    state: State<'_, AppState>,
    scenario_id: String,
) -> CmdResult<Vec<Evaluation>> {
    let settings = state.settings.read().await.alert.clone();
    let scenario = demo::alert_scenarios()
        .into_iter()
        .find(|s| s.id == scenario_id)
        .ok_or_else(|| format!("unknown scenario {scenario_id}"))?;
    let source = alert::SourceIdentity {
        product: "demo-scenario".into(),
        spacecraft: Some("SYNTHETIC".into()),
    };
    let mut memory = AlertMemory {
        settings_version: settings.settings_version,
        ..Default::default()
    };
    let mut steps = Vec::new();
    for i in (5..=scenario.samples.len()).step_by(5) {
        let window = &scenario.samples[..i];
        let at = window.last().expect("non-empty window").time;
        let e = alert::evaluate(window, &settings, &memory, &source, at);
        memory = e.memory.clone();
        steps.push(e);
    }
    Ok(steps)
}

// --- Aurora and imagery ------------------------------------------------------

#[tauri::command]
pub async fn get_aurora_grid(state: State<'_, AppState>) -> CmdResult<AuroraGrid> {
    let payloads = match state.replay.read().await.as_ref() {
        Some(session) => session.payloads.clone(),
        None => state.live.read().await.clone(),
    };
    crate::snapshot::aurora_grid(&payloads).ok_or_else(|| "aurora product unavailable".to_string())
}

/// Fetch current solar imagery. `at` selects the requested instant explicitly,
/// so replay imagery is never confused with live imagery.
#[tauri::command]
pub async fn get_sun_images(
    state: State<'_, AppState>,
    at: Option<DateTime<Utc>>,
) -> CmdResult<Vec<SunImage>> {
    let now = Utc::now();
    let requested = at.unwrap_or(now);
    let mut out = Vec::new();
    let mut errors = Vec::new();
    for passband in Passband::ALL {
        match imagery::fetch_image(&state.fetcher, passband, requested, now).await {
            Ok(img) => out.push(img),
            Err(e) => errors.push(format!("{}: {e}", passband.label())),
        }
    }
    if out.is_empty() {
        return Err(format!("solar imagery unavailable ({})", errors.join("; ")));
    }
    Ok(out)
}

// --- Replay ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotRef {
    pub id: i64,
    pub product: String,
    pub retrieved_at: DateTime<Utc>,
}

#[tauri::command]
pub async fn list_snapshots(
    state: State<'_, AppState>,
    product: String,
    limit: Option<u32>,
) -> CmdResult<Vec<SnapshotRef>> {
    let store = state.store.lock().await;
    let rows = store
        .snapshot_index(&product, limit.unwrap_or(48))
        .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, retrieved_at)| SnapshotRef {
            id,
            product: product.clone(),
            retrieved_at,
        })
        .collect())
}

/// Enter replay for a stored snapshot set.
///
/// Replay reconstructs **what was published at that moment**, because that is
/// what the stored payloads contain. It does not reconstruct revised physical
/// event times, which would require provider version history.
#[tauri::command]
pub async fn enter_replay(
    state: State<'_, AppState>,
    snapshot_ids: Vec<i64>,
) -> CmdResult<Dashboard> {
    let mut payloads = crate::snapshot::Payloads::new();
    {
        let store = state.store.lock().await;
        for id in snapshot_ids {
            if let Ok(Some(s)) = store.snapshot_by_id(id) {
                payloads.insert(s.product.clone(), s);
            }
        }
    }
    if payloads.is_empty() {
        return Err("no stored snapshots matched the requested replay".into());
    }
    let label = payloads
        .values()
        .map(|s| s.retrieved_at)
        .max()
        .map(|t| format!("replay:{}", t.to_rfc3339()))
        .unwrap_or_else(|| "replay".into());
    *state.replay.write().await = Some(crate::ReplaySession {
        mode: Mode::Replay,
        payloads,
        memory: AlertMemory::default(),
        label,
    });
    Ok(state.dashboard().await)
}

#[tauri::command]
pub async fn enter_demo(state: State<'_, AppState>) -> CmdResult<Dashboard> {
    *state.replay.write().await = Some(crate::ReplaySession {
        mode: Mode::Demo,
        payloads: demo::payloads(),
        memory: AlertMemory::default(),
        label: format!("demo:{}", demo::captured_at().to_rfc3339()),
    });
    Ok(state.dashboard().await)
}

#[tauri::command]
pub async fn exit_replay(state: State<'_, AppState>) -> CmdResult<Dashboard> {
    // Live episode history was never touched, so returning to now needs no
    // reconciliation beyond dropping the replay session.
    *state.replay.write().await = None;
    Ok(state.dashboard().await)
}

// --- Export ------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportPayload {
    pub csv: String,
    pub json: String,
    pub suggested_basename: String,
    pub metadata: ExportMetadata,
}

/// Build an export from the **current snapshot**, not from a live view that
/// might refresh mid-export.
#[tauri::command]
pub async fn export_series(
    state: State<'_, AppState>,
    series_keys: Vec<String>,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> CmdResult<ExportPayload> {
    let dashboard = state.dashboard().await;
    let display_time_zone = state.settings.read().await.display_time_zone.clone();

    let mut selected: Vec<Series> = Vec::new();
    for key in &series_keys {
        let Some(series) = dashboard.series.get(key) else {
            return Err(format!("unknown series {key}"));
        };
        let mut s = series.clone();
        if let (Some(from), Some(to)) = (start, end) {
            s.samples.retain(|o| o.time >= from && o.time <= to);
        }
        selected.push(s);
    }
    if selected.is_empty() {
        return Err("select at least one series to export".into());
    }

    let selection_start = start
        .or_else(|| {
            selected
                .iter()
                .filter_map(|s| s.samples.first().map(|o| o.time))
                .min()
        })
        .unwrap_or(dashboard.assembled_at);
    let selection_end = end
        .or_else(|| {
            selected
                .iter()
                .filter_map(|s| s.samples.last().map(|o| o.time))
                .max()
        })
        .unwrap_or(dashboard.assembled_at);

    let metadata = ExportMetadata {
        schema_version: export::EXPORT_SCHEMA_VERSION.into(),
        app_version: dashboard.app_version.clone(),
        series: selected.iter().map(SeriesMetadata::of).collect(),
        selection_start,
        selection_end,
        display_time_zone,
        snapshot_id: dashboard.snapshot_id.clone(),
        exported_at: Utc::now(),
    };

    Ok(ExportPayload {
        csv: export::to_csv(&selected),
        json: export::to_json(selected, metadata.clone()).map_err(|e| e.to_string())?,
        suggested_basename: format!("space-weather-{}", selection_end.format("%Y%m%dT%H%M%SZ")),
        metadata,
    })
}

/// Write export contents to a path the user chose in a native save dialog.
///
/// The path is validated before use: it must be absolute, must not contain a
/// parent traversal, and must carry an expected extension.
#[tauri::command]
pub async fn write_export(path: String, contents: String) -> CmdResult<String> {
    let path = validated_export_path(&path, &["csv", "json", "svg"])?;
    std::fs::write(&path, contents).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

/// Write a binary export (PNG) supplied as base64, to a dialog-chosen path.
/// Same path validation as `write_export`.
#[tauri::command]
pub async fn write_export_binary(path: String, base64_contents: String) -> CmdResult<String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(base64_contents.as_bytes())
        .map_err(|e| format!("export payload was not valid base64: {e}"))?;
    let path = validated_export_path(&path, &["png", "svg"])?;
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(path.display().to_string())
}

fn validated_export_path(path: &str, extensions: &[&str]) -> CmdResult<PathBuf> {
    let path = PathBuf::from(path);
    if !path.is_absolute() {
        return Err("export path must be absolute".into());
    }
    if path
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err("export path must not contain '..'".into());
    }
    let ok_ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| extensions.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false);
    if !ok_ext {
        return Err(format!(
            "export must be saved as one of: {}",
            extensions.join(", ")
        ));
    }
    Ok(path)
}

// --- Reference material ------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceEntry {
    pub product: String,
    pub url: String,
    pub page: Option<String>,
    pub cadence_seconds: i64,
    pub description: String,
}

#[tauri::command]
pub fn get_sources() -> Vec<SourceEntry> {
    Product::ALL
        .into_iter()
        .map(|p| SourceEntry {
            product: p.key().to_string(),
            url: p.url().to_string(),
            page: product_page(p),
            cadence_seconds: p.poll_seconds(),
            description: product_description(p).to_string(),
        })
        .collect()
}

fn product_page(p: Product) -> Option<String> {
    Some(
        match p {
            Product::SolarWindPlasma | Product::SolarWindMag => {
                "https://www.swpc.noaa.gov/products/real-time-solar-wind"
            }
            Product::GoesXray | Product::GoesInstrumentSources => {
                "https://www.swpc.noaa.gov/products/goes-x-ray-flux"
            }
            Product::PlanetaryKp | Product::PlanetaryKpForecast => {
                "https://www.swpc.noaa.gov/products/planetary-k-index"
            }
            Product::NoaaScales => "https://www.spaceweather.gov/noaa-scales-explanation",
            Product::Alerts => "https://www.spaceweather.gov/products/alerts-watches-and-warnings",
            Product::Aurora => "https://www.spaceweather.gov/products/aurora-30-minute-forecast",
            Product::ThreeDayForecast => "https://www.spaceweather.gov/products/3-day-forecast",
            Product::ThreeDayGeomagForecast => {
                "https://www.spaceweather.gov/products/3-day-geomagnetic-forecast"
            }
        }
        .to_string(),
    )
}

fn product_description(p: Product) -> &'static str {
    match p {
        Product::SolarWindPlasma => "Upstream solar-wind plasma: speed, density and temperature, 1-minute cadence, per spacecraft stream.",
        Product::SolarWindMag => "Upstream interplanetary magnetic field: |B| and Bz in GSM and GSE, 1-minute cadence.",
        Product::GoesXray => "GOES X-ray flux in both documented passbands, 1-minute cadence.",
        Product::GoesInstrumentSources => "Which GOES satellite is primary and secondary for each instrument.",
        Product::PlanetaryKp => "NOAA estimated planetary K-index, 3-hour intervals.",
        Product::PlanetaryKpForecast => "Planetary Kp with observed, estimated and predicted intervals separated.",
        Product::NoaaScales => "Current and forecast NOAA G, R and S scale status and published probabilities.",
        Product::Alerts => "SWPC alerts, watches and warnings with issuance, validity, cancellation and supersession.",
        Product::Aurora => "OVATION aurora model grid with its own observation and forecast times.",
        Product::ThreeDayForecast => "NOAA 3-day forecast: Kp breakdown, radio and radiation outlook.",
        Product::ThreeDayGeomagForecast => "NOAA 3-day geomagnetic forecast: Ap, probabilities and Kp breakdown.",
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lesson {
    pub id: String,
    pub title: String,
    pub question: String,
    pub steps: Vec<LessonStep>,
    pub explanation: String,
    pub sources: Vec<String>,
    /// The dataset the lesson runs against.
    pub dataset: String,
    pub attribution: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LessonStep {
    pub prompt: String,
    /// Series to focus, when the step focuses one.
    pub focus_series: Option<String>,
    /// Instant to select, when the step selects one.
    pub select_time: Option<DateTime<Utc>>,
    /// View to open: `observatory`, `aurora`, `sources`.
    pub view: String,
}

#[tauri::command]
pub fn get_lessons() -> Vec<Lesson> {
    crate::lessons::lessons()
}

// --- Window bounds -------------------------------------------------------------

/// Persist window bounds. Called by the shell on close; validated on restore.
pub async fn save_window_bounds(state: &AppState, bounds: crate::settings::WindowBounds) {
    let mut settings = state.settings.write().await;
    settings.window_bounds = Some(bounds);
    let _ = settings.save(&state.config_dir);
}

// --- Cache -------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStatus {
    pub size_bytes: u64,
    pub limit_bytes: u64,
    pub snapshot_count: i64,
    pub data_dir: String,
    pub config_dir: String,
}

#[tauri::command]
pub async fn cache_status(state: State<'_, AppState>) -> CmdResult<CacheStatus> {
    let settings = state.settings.read().await.clone();
    let store = state.store.lock().await;
    Ok(CacheStatus {
        size_bytes: store.size_bytes().map_err(|e| e.to_string())?,
        limit_bytes: settings.cache_limit_mb * 1024 * 1024,
        snapshot_count: store.snapshot_count().map_err(|e| e.to_string())?,
        data_dir: state.data_dir.display().to_string(),
        config_dir: state.config_dir.display().to_string(),
    })
}

/// Explicit user-initiated cleanup: trims snapshots to the retention setting.
#[tauri::command]
pub async fn clear_cache(state: State<'_, AppState>) -> CmdResult<CacheStatus> {
    {
        let store = state.store.lock().await;
        store.enforce_limits(1, 0).map_err(|e| e.to_string())?;
    }
    cache_status(state).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn write_export_rejects_paths_the_ui_should_not_be_able_to_reach() {
        assert!(write_export("relative/file.csv".into(), "x".into())
            .await
            .is_err());
        assert!(write_export("/tmp/../etc/passwd".into(), "x".into())
            .await
            .is_err());
        assert!(write_export("/tmp/swo-test-export.sh".into(), "x".into())
            .await
            .is_err());
        assert!(write_export("/tmp/swo-test-export".into(), "x".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn write_export_accepts_a_plausible_chosen_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("series.csv");
        let written = write_export(path.display().to_string(), "a,b\n1,2\n".into())
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(written).unwrap(), "a,b\n1,2\n");
    }

    #[test]
    fn every_product_has_a_page_reference_and_description() {
        for entry in get_sources() {
            assert!(entry.url.starts_with("https://"));
            assert!(entry.page.as_ref().unwrap().starts_with("https://"));
            assert!(entry.description.len() > 20, "{}", entry.product);
        }
    }

    #[test]
    fn settings_errors_are_explained_in_plain_language() {
        for e in [
            SettingsError::ThresholdOutOfRange,
            SettingsError::PersistenceOutOfRange,
            SettingsError::HysteresisOutOfRange,
            SettingsError::ClearanceOutOfRange,
            SettingsError::CadenceOutOfRange,
            SettingsError::StaleWindowOutOfRange,
            SettingsError::NonFinite,
        ] {
            let msg = describe(e.clone());
            assert!(msg.ends_with('.'), "{msg}");
            assert!(!msg.contains("OutOfRange"), "{msg}");
        }
    }
}
