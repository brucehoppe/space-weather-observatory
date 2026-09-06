//! Space Weather Observatory desktop application.
//!
//! The backend owns all acquisition, storage and evaluation. The webview holds
//! no credentials, makes no network requests of its own, and reaches the
//! backend only through the narrowly typed commands in `commands`.

pub mod commands;
pub mod demo;
pub mod imagery;
pub mod lessons;
pub mod providers;
pub mod settings;
pub mod snapshot;
pub mod store;

use chrono::Utc;
use std::path::PathBuf;
use std::sync::Arc;
use swo_core::alert::{AlertMemory, SourceIdentity};
use tokio::sync::RwLock;

use crate::providers::{Fetcher, Product};
use crate::settings::Settings;
use crate::snapshot::{Dashboard, Mode, Payloads};
use crate::store::Store;

/// Everything the running application holds.
pub struct AppState {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub store: Arc<tokio::sync::Mutex<Store>>,
    pub fetcher: Arc<Fetcher>,
    pub settings: RwLock<Settings>,
    /// Live payloads, one per product.
    pub live: RwLock<Payloads>,
    /// Live alert memory, persisted to the store on change.
    pub alert_memory: RwLock<AlertMemory>,
    /// Replay/demo state, kept strictly separate from live state so a replay
    /// can never overwrite live episode history (spec §13B).
    pub replay: RwLock<Option<ReplaySession>>,
}

pub struct ReplaySession {
    pub mode: Mode,
    pub payloads: Payloads,
    /// Alert memory for the replay only; discarded when replay ends.
    pub memory: AlertMemory,
    pub label: String,
}

impl AppState {
    pub fn new(config_dir: PathBuf, data_dir: PathBuf) -> Result<Self, String> {
        let store = Store::open(&data_dir.join("cache").join("observatory.sqlite3"))
            .map_err(|e| format!("could not open local store: {e}"))?;
        let settings = Settings::load(&config_dir);
        let memory = store.load_alert_memory().unwrap_or_default();
        // Bound the cache on startup, before anything is added to it.
        let _ = store.enforce_limits(
            settings.snapshot_retention,
            settings.cache_limit_mb * 1024 * 1024,
        );
        Ok(Self {
            config_dir,
            data_dir,
            store: Arc::new(tokio::sync::Mutex::new(store)),
            fetcher: Arc::new(Fetcher::new().map_err(|e| e.to_string())?),
            settings: RwLock::new(settings),
            live: RwLock::new(Payloads::new()),
            alert_memory: RwLock::new(memory),
            replay: RwLock::new(None),
        })
    }

    /// Load the newest stored payload for each product, so the application can
    /// launch and be useful with no network at all (spec §10).
    pub async fn hydrate_from_cache(&self) {
        let store = self.store.lock().await;
        let mut live = self.live.write().await;
        for product in Product::ALL {
            if let Ok(Some(snapshot)) = store.latest_snapshot(product.key()) {
                live.insert(product.key().to_string(), snapshot);
            }
        }
    }

    /// Identity of the stream currently feeding the detector.
    pub async fn wind_source(&self) -> SourceIdentity {
        let live = self.live.read().await;
        let spacecraft = live
            .get(Product::SolarWindPlasma.key())
            .and_then(|s| swo_core::parse::rtsw::parse_wind(&s.payload).ok())
            .and_then(|streams| {
                streams
                    .iter()
                    .find(|s| s.active)
                    .map(|s| s.spacecraft.clone())
            });
        SourceIdentity {
            product: Product::SolarWindPlasma.key().to_string(),
            spacecraft,
        }
    }

    pub async fn dashboard(&self) -> Dashboard {
        let now = Utc::now();
        if let Some(session) = self.replay.read().await.as_ref() {
            return snapshot::assemble(session.mode, &session.payloads, now, session.label.clone());
        }
        let live = self.live.read().await;
        let id = live
            .values()
            .map(|s| s.sha256.as_str())
            .collect::<Vec<_>>()
            .join("");
        snapshot::assemble(Mode::Live, &live, now, store::sha256_hex(&id))
    }
}

/// Resolve per-user directories. Local data never lives in the install folder.
fn app_dirs() -> Result<(PathBuf, PathBuf), String> {
    const QUALIFIER: &str = "SpaceWeatherObservatory";
    let config = dirs::config_dir()
        .ok_or("no per-user configuration directory available")?
        .join(QUALIFIER);
    let data = dirs::data_dir()
        .ok_or("no per-user application data directory available")?
        .join(QUALIFIER);
    std::fs::create_dir_all(&config).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    Ok((config, data))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let (config_dir, data_dir) = app_dirs().expect("per-user application directories");
    let state = AppState::new(config_dir, data_dir).expect("application state");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(state)
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use tauri::Manager;
                let state = handle.state::<AppState>();
                // Offline-first: show cached data immediately, then refresh.
                state.hydrate_from_cache().await;
                commands::refresh_all(&state).await;
                commands::start_polling(handle.clone());
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_dashboard,
            commands::refresh,
            commands::get_settings,
            commands::save_settings,
            commands::reset_alert_settings,
            commands::evaluate_alert,
            commands::acknowledge_episode,
            commands::get_aurora_grid,
            commands::get_sun_images,
            commands::list_snapshots,
            commands::enter_replay,
            commands::enter_demo,
            commands::exit_replay,
            commands::get_alert_scenarios,
            commands::evaluate_scenario,
            commands::export_series,
            commands::write_export,
            commands::get_sources,
            commands::get_lessons,
            commands::cache_status,
            commands::clear_cache,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Space Weather Observatory");
}
