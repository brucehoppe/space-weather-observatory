//! Local settings. No account, no cloud, no telemetry: a single JSON file in
//! the platform's per-user configuration directory.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use swo_core::alert::AlertSettings;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub schema_version: u32,
    /// IANA time zone name, or `UTC`. "Tonight" wording requires a known zone.
    pub display_time_zone: String,
    /// Optional user-selected region, used only to qualify wording. Never a
    /// city-level forecast derived from planetary indices.
    pub region_label: Option<String>,
    pub alert: AlertSettings,
    /// Upper bound on the local cache, enforced on startup and after refresh.
    pub cache_limit_mb: u64,
    /// How many snapshots to retain for replay.
    pub snapshot_retention: u32,
    /// Honour the OS reduced-motion preference; may also be forced on here.
    pub force_reduced_motion: bool,
    /// Remembered window bounds, validated against current monitors on load.
    pub window_bounds: Option<WindowBounds>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowBounds {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub maximized: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            schema_version: swo_core::SCHEMA_VERSION,
            display_time_zone: "UTC".into(),
            region_label: None,
            alert: AlertSettings::default(),
            cache_limit_mb: 256,
            snapshot_retention: 48,
            force_reduced_motion: false,
            window_bounds: None,
        }
    }
}

impl Settings {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("settings.json")
    }

    /// Load settings, falling back to defaults when the file is absent or
    /// corrupt. A corrupt file is preserved as `.corrupt` rather than deleted.
    pub fn load(config_dir: &Path) -> Self {
        let path = Self::path(config_dir);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match serde_json::from_str::<Settings>(&raw) {
            Ok(mut s) => {
                if s.alert.validate().is_err() {
                    s.alert = AlertSettings::default();
                }
                s
            }
            Err(_) => {
                let _ = std::fs::rename(&path, path.with_extension("json.corrupt"));
                Self::default()
            }
        }
    }

    pub fn save(&self, config_dir: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(config_dir)?;
        let path = Self::path(config_dir);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        // Atomic replace so a crash mid-write cannot corrupt settings.
        std::fs::rename(tmp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_round_trip_through_disk() {
        let dir = tempfile::tempdir().unwrap();
        let s = Settings::default();
        s.save(dir.path()).unwrap();
        assert_eq!(Settings::load(dir.path()), s);
    }

    #[test]
    fn a_corrupt_file_falls_back_to_defaults_and_is_preserved() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(Settings::path(dir.path()), "{ not json").unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s, Settings::default());
        assert!(dir.path().join("settings.json.corrupt").exists());
    }

    #[test]
    fn invalid_stored_alert_settings_are_replaced_not_used() {
        let dir = tempfile::tempdir().unwrap();
        let mut s = Settings::default();
        s.alert.entry_threshold_km_s = 5.0; // out of range
        std::fs::write(
            Settings::path(dir.path()),
            serde_json::to_string(&s).unwrap(),
        )
        .unwrap();
        let loaded = Settings::load(dir.path());
        assert_eq!(loaded.alert, AlertSettings::default());
    }

    #[test]
    fn missing_fields_take_defaults_rather_than_failing() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            Settings::path(dir.path()),
            r#"{"display_time_zone":"Europe/Helsinki"}"#,
        )
        .unwrap();
        let s = Settings::load(dir.path());
        assert_eq!(s.display_time_zone, "Europe/Helsinki");
        assert_eq!(s.alert, AlertSettings::default());
    }
}
