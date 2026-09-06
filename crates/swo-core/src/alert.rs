//! Custom solar-wind speed alert (spec §13B).
//!
//! This is a **user-configured measurement condition detector**, not a NOAA
//! product. It never produces G/R/S levels, outage predictions, personal risk
//! scores or confidence percentages. It is a pure function of
//! (observations, settings, prior state, now) so every transition is testable
//! without a UI, a clock or a network.
//!
//! All timing decisions use provider measurement timestamps. Download counts,
//! render frames and repeated copies of the same sample are never treated as
//! the passage of time.

use crate::model::{Observation, Quality};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

/// Version of the comparison rules below. Stored on every episode so a later
/// rule change cannot silently rewrite the meaning of past episodes.
pub const RULE_VERSION: &str = "solar-wind-speed-threshold/1";

/// User-owned settings. Defaults are product defaults for reducing nuisance
/// alerts — they are not NOAA alert criteria and not physical impact
/// boundaries.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertSettings {
    pub enabled: bool,
    /// Entry threshold in km/s. Entry is `speed >= threshold`.
    pub entry_threshold_km_s: f64,
    /// How long the condition must hold continuously before it qualifies.
    pub persistence_minutes: i64,
    /// Clearing requires `speed < threshold - hysteresis` (km/s).
    pub hysteresis_km_s: f64,
    /// How long the below-band condition must hold to clear.
    pub clearance_minutes: i64,
    /// Nominal product cadence used to derive coverage expectations.
    pub cadence_seconds: i64,
    /// Evaluation pauses when the newest accepted sample is older than this.
    pub stale_after_minutes: i64,
    /// Incremented by the application whenever the user saves a change, so
    /// episodes record the configuration they were evaluated under.
    pub settings_version: u32,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            entry_threshold_km_s: 500.0,
            persistence_minutes: 10,
            hysteresis_km_s: 25.0,
            clearance_minutes: 5,
            cadence_seconds: 60,
            stale_after_minutes: 15,
            settings_version: 1,
        }
    }
}

/// Reasons a settings value is rejected. Invalid input is never silently
/// coerced (spec §13B).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SettingsError {
    ThresholdOutOfRange,
    PersistenceOutOfRange,
    HysteresisOutOfRange,
    ClearanceOutOfRange,
    CadenceOutOfRange,
    StaleWindowOutOfRange,
    NonFinite,
}

impl AlertSettings {
    /// Physically plausible bounds for a solar-wind speed threshold. The upper
    /// bound is generous (extreme events have exceeded 2000 km/s); the point is
    /// to reject typos and unit mistakes, not to make a scientific claim.
    pub fn validate(&self) -> Result<(), SettingsError> {
        if !self.entry_threshold_km_s.is_finite() || !self.hysteresis_km_s.is_finite() {
            return Err(SettingsError::NonFinite);
        }
        if !(200.0..=3000.0).contains(&self.entry_threshold_km_s) {
            return Err(SettingsError::ThresholdOutOfRange);
        }
        if !(1..=720).contains(&self.persistence_minutes) {
            return Err(SettingsError::PersistenceOutOfRange);
        }
        if !(0.0..=200.0).contains(&self.hysteresis_km_s)
            || self.hysteresis_km_s >= self.entry_threshold_km_s
        {
            return Err(SettingsError::HysteresisOutOfRange);
        }
        if !(1..=720).contains(&self.clearance_minutes) {
            return Err(SettingsError::ClearanceOutOfRange);
        }
        if !(1..=3600).contains(&self.cadence_seconds) {
            return Err(SettingsError::CadenceOutOfRange);
        }
        if !(1..=1440).contains(&self.stale_after_minutes) {
            return Err(SettingsError::StaleWindowOutOfRange);
        }
        Ok(())
    }

    /// Clearing bound: `threshold - hysteresis`.
    pub fn clear_below_km_s(&self) -> f64 {
        self.entry_threshold_km_s - self.hysteresis_km_s
    }

    /// Largest acceptable gap between consecutive accepted samples inside a
    /// coverage window: three nominal cadences. Documented so the UI can state
    /// it (spec §13B "documented gap tolerance derived from the actual product
    /// cadence").
    pub fn gap_tolerance(&self) -> Duration {
        Duration::seconds(self.cadence_seconds * 3)
    }
}

/// Evidence that a window contained enough valid data to make an assertion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Coverage {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub expected_samples: i64,
    pub accepted_samples: i64,
    pub largest_gap_seconds: i64,
    pub adequate: bool,
}

/// Minimum fraction of expected samples required in a coverage window.
const MIN_COVERAGE_FRACTION: f64 = 0.75;

fn assess_coverage(
    samples: &[Observation],
    start: DateTime<Utc>,
    end: DateTime<Utc>,
    settings: &AlertSettings,
) -> Coverage {
    let span = (end - start).num_seconds().max(0);
    let expected = (span / settings.cadence_seconds).max(1);
    let tol = settings.gap_tolerance();

    let in_window: Vec<&Observation> = samples
        .iter()
        .filter(|o| o.time >= start && o.time <= end && o.accepted().is_some())
        .collect();

    let accepted = in_window.len() as i64;
    let mut largest_gap = 0i64;
    let mut prev = start;
    for obs in &in_window {
        largest_gap = largest_gap.max((obs.time - prev).num_seconds());
        prev = obs.time;
    }
    largest_gap = largest_gap.max((end - prev).num_seconds());

    let adequate = accepted > 0
        && (accepted as f64) >= (expected as f64 * MIN_COVERAGE_FRACTION)
        && largest_gap <= tol.num_seconds();

    Coverage {
        window_start: start,
        window_end: end,
        expected_samples: expected,
        accepted_samples: accepted,
        largest_gap_seconds: largest_gap,
        adequate,
    }
}

/// A qualified threshold episode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Episode {
    /// Deterministic identity: onset instant + settings version. Restart
    /// reconciliation recomputes the same id from the same data.
    pub id: String,
    /// First sample of the continuous at-or-above-threshold run that qualified.
    pub qualified_onset: DateTime<Utc>,
    pub onset_speed_km_s: f64,
    /// Newest accepted observation attributed to this episode.
    pub last_observation_time: DateTime<Utc>,
    pub last_speed_km_s: f64,
    pub peak_speed_km_s: f64,
    /// Provider product and spacecraft that supplied the qualifying data.
    pub source_product: String,
    pub source_spacecraft: Option<String>,
    pub settings_version: u32,
    pub rule_version: String,
    /// Presentation attribute only. Dismissal never clears a condition and
    /// never disables the detector (spec §13B).
    pub acknowledged: bool,
    /// Set when the clearance rule was satisfied, with its evidence.
    pub cleared_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clearance_evidence: Option<Coverage>,
}

impl Episode {
    fn make_id(onset: DateTime<Utc>, settings_version: u32) -> String {
        format!("ep-{}-v{}", onset.timestamp(), settings_version)
    }
}

/// Why evaluation is paused.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PauseReason {
    /// Newest accepted sample is older than the stale window.
    StaleFeed,
    /// No accepted samples at all.
    NoData,
    /// Samples exist but the persistence window is not adequately covered.
    InsufficientCoverage,
}

/// Detector state (spec §13B state model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum AlertState {
    Disabled,
    Monitoring,
    Pending {
        /// Start of the current continuous at-or-above-threshold run.
        since: DateTime<Utc>,
        /// How much of the persistence requirement is satisfied so far.
        elapsed_seconds: i64,
        required_seconds: i64,
    },
    Active {
        episode: Episode,
    },
    /// Evaluation paused. Any previous episode is retained unchanged: stopping
    /// observations never implies a condition ended.
    DataUnavailable {
        reason: PauseReason,
        retained_episode: Option<Episode>,
    },
    /// Clearance rule satisfied; the episode is retained as history.
    Cleared {
        episode: Episode,
    },
}

/// Persistent detector memory, reconciled against fresh data on restart.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlertMemory {
    /// The episode currently open (active) if any.
    pub open_episode: Option<Episode>,
    /// Completed episodes, newest last.
    pub history: Vec<Episode>,
    /// Settings version the memory was last evaluated under. A change resets
    /// the pending window without rewriting history.
    pub settings_version: u32,
}

/// Result of one evaluation pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evaluation {
    pub state: AlertState,
    pub memory: AlertMemory,
    /// Coverage evidence for the window that was examined, when one was.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub coverage: Option<Coverage>,
    /// True only on the pass where an episode first becomes active. The UI
    /// shows one banner and announces once per episode on this signal.
    pub newly_active: bool,
}

/// Identity of the data feeding the detector, recorded on episodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SourceIdentity {
    pub product: String,
    pub spacecraft: Option<String>,
}

/// Evaluate the detector.
///
/// `samples` must be speed observations in km/s, already normalized (sorted,
/// de-duplicated). Out-of-order or duplicated input is tolerated: the function
/// re-sorts defensively and uses measurement timestamps only.
pub fn evaluate(
    samples: &[Observation],
    settings: &AlertSettings,
    prior: &AlertMemory,
    source: &SourceIdentity,
    now: DateTime<Utc>,
) -> Evaluation {
    let mut memory = prior.clone();

    if !settings.enabled {
        // Disabling stops evaluation but preserves history; it does not assert
        // that any past condition ended.
        if let Some(open) = memory.open_episode.take() {
            memory.history.push(open);
        }
        return Evaluation {
            state: AlertState::Disabled,
            memory,
            coverage: None,
            newly_active: false,
        };
    }

    // A settings change resets the pending evaluation window and detaches any
    // open episode into history rather than re-judging it under new rules.
    let settings_changed =
        memory.settings_version != 0 && memory.settings_version != settings.settings_version;
    if settings_changed {
        if let Some(open) = memory.open_episode.take() {
            memory.history.push(open);
        }
    }
    memory.settings_version = settings.settings_version;

    let mut sorted: Vec<Observation> = samples.to_vec();
    sorted.sort_by_key(|o| o.time);

    let Some(last) = sorted
        .iter()
        .rev()
        .find(|o| o.accepted().is_some())
        .cloned()
    else {
        return paused(PauseReason::NoData, memory, None);
    };
    let last_speed = last.accepted().expect("filtered on accepted");

    if (now - last.time) > Duration::minutes(settings.stale_after_minutes) {
        return paused(PauseReason::StaleFeed, memory, None);
    }

    // --- Clearance is checked first: an open episode can only end on evidence.
    if let Some(open) = memory.open_episode.clone() {
        let clear_start = last.time - Duration::minutes(settings.clearance_minutes);
        let cov = assess_coverage(&sorted, clear_start, last.time, settings);
        let all_below = sorted
            .iter()
            .filter(|o| o.time >= clear_start && o.time <= last.time)
            .filter_map(|o| o.accepted())
            .all(|v| v < settings.clear_below_km_s());

        if cov.adequate && all_below {
            let mut cleared = open;
            cleared.cleared_at = Some(last.time);
            cleared.clearance_evidence = Some(cov.clone());
            cleared.last_observation_time = last.time;
            cleared.last_speed_km_s = last_speed;
            memory.open_episode = None;
            memory.history.push(cleared.clone());
            return Evaluation {
                state: AlertState::Cleared { episode: cleared },
                memory,
                coverage: Some(cov),
                newly_active: false,
            };
        }

        if !cov.adequate {
            // Not enough evidence to assert anything about the episode.
            return paused(PauseReason::InsufficientCoverage, memory, None);
        }

        // Still above the clearing bound (including anywhere in the hysteresis
        // band): the episode keeps its qualified state and updates in place.
        let mut updated = memory.open_episode.take().unwrap_or(cleared_placeholder());
        updated.last_observation_time = last.time;
        updated.last_speed_km_s = last_speed;
        updated.peak_speed_km_s = updated
            .peak_speed_km_s
            .max(peak_since(&sorted, updated.qualified_onset));
        memory.open_episode = Some(updated.clone());
        return Evaluation {
            state: AlertState::Active { episode: updated },
            memory,
            coverage: Some(cov),
            newly_active: false,
        };
    }

    // --- No open episode: look for a qualifying sustained crossing.
    if last_speed < settings.entry_threshold_km_s {
        return Evaluation {
            state: AlertState::Monitoring,
            memory,
            coverage: None,
            newly_active: false,
        };
    }

    // Walk back over the continuous at-or-above-threshold run.
    let mut run_start = last.time;
    let mut onset_speed = last_speed;
    for obs in sorted.iter().rev() {
        let Some(v) = obs.accepted() else { continue };
        if v >= settings.entry_threshold_km_s {
            run_start = obs.time;
            onset_speed = v;
        } else {
            break;
        }
    }

    let required = Duration::minutes(settings.persistence_minutes);
    let elapsed = last.time - run_start;
    let cov = assess_coverage(&sorted, last.time - required, last.time, settings);

    if elapsed < required {
        return Evaluation {
            state: AlertState::Pending {
                since: run_start,
                elapsed_seconds: elapsed.num_seconds(),
                required_seconds: required.num_seconds(),
            },
            memory,
            coverage: Some(cov),
            newly_active: false,
        };
    }

    if !cov.adequate {
        // The run looks long enough, but the window is not adequately covered:
        // say so rather than manufacturing a sustained condition.
        return paused(PauseReason::InsufficientCoverage, memory, Some(cov));
    }

    let episode = Episode {
        id: Episode::make_id(run_start, settings.settings_version),
        qualified_onset: run_start,
        onset_speed_km_s: onset_speed,
        last_observation_time: last.time,
        last_speed_km_s: last_speed,
        peak_speed_km_s: peak_since(&sorted, run_start),
        source_product: source.product.clone(),
        source_spacecraft: source.spacecraft.clone(),
        settings_version: settings.settings_version,
        rule_version: RULE_VERSION.to_string(),
        acknowledged: false,
        cleared_at: None,
        clearance_evidence: None,
    };

    // Restart reconciliation: if this same episode is already in history
    // (same deterministic id), carry its acknowledgement forward rather than
    // showing the banner again.
    let previously_acknowledged = memory
        .history
        .iter()
        .find(|e| e.id == episode.id)
        .map(|e| e.acknowledged)
        .unwrap_or(false);

    let mut episode = episode;
    episode.acknowledged = previously_acknowledged;
    memory.history.retain(|e| e.id != episode.id);
    memory.open_episode = Some(episode.clone());

    Evaluation {
        state: AlertState::Active { episode },
        memory,
        coverage: Some(cov),
        newly_active: !previously_acknowledged,
    }
}

fn peak_since(samples: &[Observation], from: DateTime<Utc>) -> f64 {
    samples
        .iter()
        .filter(|o| o.time >= from)
        .filter_map(|o| o.accepted())
        .fold(f64::MIN, f64::max)
}

fn cleared_placeholder() -> Episode {
    // Unreachable in practice: guarded by the `if let Some(open)` above. Kept
    // total so the evaluator cannot panic on unexpected state.
    Episode {
        id: "ep-unknown".into(),
        qualified_onset: DateTime::<Utc>::MIN_UTC,
        onset_speed_km_s: f64::NAN,
        last_observation_time: DateTime::<Utc>::MIN_UTC,
        last_speed_km_s: f64::NAN,
        peak_speed_km_s: f64::NAN,
        source_product: String::new(),
        source_spacecraft: None,
        settings_version: 0,
        rule_version: RULE_VERSION.into(),
        acknowledged: false,
        cleared_at: None,
        clearance_evidence: None,
    }
}

fn paused(reason: PauseReason, memory: AlertMemory, coverage: Option<Coverage>) -> Evaluation {
    let retained = memory.open_episode.clone();
    Evaluation {
        state: AlertState::DataUnavailable {
            reason,
            retained_episode: retained,
        },
        memory,
        coverage,
        newly_active: false,
    }
}

/// Mark an episode acknowledged (dismissed). Presentation only.
pub fn acknowledge(memory: &mut AlertMemory, episode_id: &str) {
    if let Some(open) = memory.open_episode.as_mut() {
        if open.id == episode_id {
            open.acknowledged = true;
        }
    }
    for e in memory.history.iter_mut() {
        if e.id == episode_id {
            e.acknowledged = true;
        }
    }
}

/// Apply a provider correction to historical samples of an open episode
/// without producing a new intrusive banner (spec §13B).
pub fn apply_correction(memory: &mut AlertMemory, episode_id: &str, corrected_peak: f64) {
    if let Some(open) = memory.open_episode.as_mut() {
        if open.id == episode_id {
            open.peak_speed_km_s = corrected_peak;
        }
    }
    for e in memory.history.iter_mut() {
        if e.id == episode_id {
            e.peak_speed_km_s = corrected_peak;
        }
    }
}

/// Build a speed observation from provider inputs, applying the quality rules.
pub fn speed_observation(
    time: DateTime<Utc>,
    speed: Option<f64>,
    provider_quality_ok: bool,
) -> Observation {
    match speed {
        Some(v) if v.is_finite() && v > 0.0 => Observation::instant(
            time,
            Some(v),
            if provider_quality_ok {
                Quality::Good
            } else {
                Quality::Suspect
            },
        ),
        _ => Observation::instant(time, None, Quality::Missing),
    }
}

#[cfg(test)]
#[path = "alert_tests.rs"]
mod tests;
