//! The attributed frozen demonstration dataset and the labelled alert
//! scenarios (spec §9, §13B).
//!
//! Two distinct things live here, and they are never mixed:
//!
//! 1. **Frozen real data.** The captured NOAA SWPC originals from
//!    `fixtures/captured/` — real measurements from 5–6 September 2026, used
//!    for offline launch, replay and the three lessons. Nothing in them is
//!    modified: the lessons are written around what the data actually shows.
//!
//! 2. **Labelled synthetic alert scenarios.** The frozen real interval is
//!    geomagnetically quiet, so it cannot demonstrate a threshold episode. The
//!    scenarios below are explicitly synthetic, always surfaced as
//!    "Demo: solar-wind alert", and never written into live episode history.

use chrono::{DateTime, Duration, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use swo_core::alert::speed_observation;
use swo_core::model::Observation;

use crate::providers::Product;
use crate::store::{sha256_hex, Snapshot as StoredSnapshot};

/// Attribution shown wherever the frozen dataset is used.
pub const ATTRIBUTION: &str =
    "Frozen demonstration dataset: NOAA Space Weather Prediction Center products captured 2026-09-06T18:04Z. \
     U.S. Government work, not subject to domestic copyright.";

/// The instant the frozen dataset was captured.
pub fn captured_at() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 6, 18, 4, 33).unwrap()
}

macro_rules! fixture {
    ($name:literal) => {
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../fixtures/captured/",
            $name
        ))
    };
}

/// The frozen payloads, in the same shape the live path produces.
pub fn payloads() -> crate::snapshot::Payloads {
    use swo_core::parse::{bulletins, forecast_text, goes, kp, ovation, rtsw, scales};
    let entries: [(Product, &str, &str); 10] = [
        (
            Product::SolarWindPlasma,
            fixture!("rtsw_wind_1m.json"),
            rtsw::WIND_URL,
        ),
        (
            Product::SolarWindMag,
            fixture!("rtsw_mag_1m.json"),
            rtsw::MAG_URL,
        ),
        (
            Product::GoesXray,
            fixture!("goes_primary_xrays_1day.json"),
            goes::PRIMARY_XRAYS_1DAY_URL,
        ),
        (
            Product::GoesInstrumentSources,
            fixture!("goes_instrument_sources.json"),
            goes::INSTRUMENT_SOURCES_URL,
        ),
        (
            Product::PlanetaryKp,
            fixture!("planetary_k_index.json"),
            kp::KP_URL,
        ),
        (
            Product::PlanetaryKpForecast,
            fixture!("planetary_k_index_forecast.json"),
            kp::KP_FORECAST_URL,
        ),
        (
            Product::NoaaScales,
            fixture!("noaa_scales.json"),
            scales::URL,
        ),
        (Product::Alerts, fixture!("alerts.json"), bulletins::URL),
        (
            Product::Aurora,
            fixture!("ovation_aurora_latest.json"),
            ovation::URL,
        ),
        (
            Product::ThreeDayForecast,
            fixture!("3-day-forecast.txt"),
            forecast_text::THREE_DAY_URL,
        ),
    ];
    entries
        .into_iter()
        .enumerate()
        .map(|(i, (product, body, url))| {
            (
                product.key().to_string(),
                StoredSnapshot {
                    id: -(i as i64 + 1),
                    product: product.key().to_string(),
                    source_url: url.to_string(),
                    retrieved_at: captured_at(),
                    sha256: sha256_hex(body),
                    payload: body.to_string(),
                },
            )
        })
        .collect()
}

/// A labelled synthetic scenario for the alert demonstration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlertScenario {
    pub id: String,
    pub title: String,
    /// What the scenario is meant to demonstrate.
    pub expectation: String,
    /// Always true: these samples are synthetic and labelled as such.
    pub synthetic: bool,
    pub samples: Vec<Observation>,
}

fn scenario(
    id: &str,
    title: &str,
    expectation: &str,
    minutes: i64,
    f: impl Fn(i64) -> Option<f64>,
) -> AlertScenario {
    let start = captured_at() - Duration::minutes(minutes);
    AlertScenario {
        id: id.into(),
        title: title.into(),
        expectation: expectation.into(),
        synthetic: true,
        samples: (0..minutes)
            .map(|m| speed_observation(start + Duration::minutes(m), f(m), true))
            .collect(),
    }
}

/// The scenarios the release evidence walks through (spec §13B).
pub fn alert_scenarios() -> Vec<AlertScenario> {
    let n = 90;
    vec![
        scenario(
            "quiet",
            "Quiet: below the threshold",
            "Detector stays in Monitoring; no banner appears.",
            n,
            |_| Some(390.0),
        ),
        scenario(
            "brief-spike",
            "Brief spike: above the threshold for four minutes",
            "Detector reaches Pending and returns to Monitoring; no alert is emitted.",
            n,
            |m| Some(if (60..64).contains(&m) { 640.0 } else { 420.0 }),
        ),
        scenario(
            "sustained",
            "Sustained crossing: above the threshold for forty minutes",
            "Detector qualifies an episode after the persistence window and shows one banner.",
            n,
            |m| Some(if m >= 45 { 560.0 + (m - 45) as f64 } else { 430.0 }),
        ),
        scenario(
            "hysteresis",
            "Fluctuating inside the hysteresis band",
            "A qualified episode stays active while readings sit between 475 and 500 km/s.",
            n,
            |m| Some(match m {
                0..=30 => 430.0,
                31..=60 => 560.0,
                _ => if m % 2 == 0 { 480.0 } else { 495.0 },
            }),
        ),
        scenario(
            "outage",
            "Feed outage during an active episode",
            "Evaluation pauses with the episode retained; the outage never reads as the storm ending.",
            n,
            |m| match m {
                0..=30 => Some(430.0),
                31..=60 => Some(600.0),
                _ => None,
            },
        ),
        scenario(
            "clearance",
            "Clearance and a second crossing",
            "The episode clears on evidence, then a new crossing opens a distinct episode.",
            160,
            |m| Some(match m {
                0..=20 => 430.0,
                21..=60 => 580.0,
                61..=90 => 440.0,
                91..=120 => 430.0,
                _ => 610.0,
            }),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use swo_core::alert::{
        evaluate, AlertMemory, AlertSettings, AlertState, PauseReason, SourceIdentity,
    };

    fn src() -> SourceIdentity {
        SourceIdentity {
            product: "demo".into(),
            spacecraft: Some("DEMO".into()),
        }
    }

    fn run(s: &AlertScenario) -> swo_core::alert::Evaluation {
        let settings = AlertSettings::default();
        let now = s.samples.last().map(|o| o.time).unwrap_or_else(captured_at);
        let mut memory = AlertMemory {
            settings_version: settings.settings_version,
            ..Default::default()
        };
        let mut last = None;
        // Step through the scenario the way the running app would, so state
        // carries forward between evaluations.
        for i in (10..=s.samples.len()).step_by(5) {
            let window = &s.samples[..i];
            let at = window.last().unwrap().time;
            let e = evaluate(
                window,
                &settings,
                &memory,
                &src(),
                at.max(window.last().unwrap().time),
            );
            memory = e.memory.clone();
            last = Some(e);
        }
        let _ = now;
        last.expect("scenarios have samples")
    }

    #[test]
    fn the_frozen_dataset_parses_into_a_complete_dashboard() {
        let p = payloads();
        let d = crate::snapshot::assemble(
            crate::snapshot::Mode::Demo,
            &p,
            captured_at(),
            "demo".into(),
        );
        assert!(d.series.contains_key("noaa-swpc:rtsw_wind_1m:proton_speed"));
        assert!(d.aurora.is_some());
        assert!(d.three_day.is_some());
        assert_eq!(d.mode, crate::snapshot::Mode::Demo);
    }

    #[test]
    fn every_scenario_is_labelled_synthetic() {
        for s in alert_scenarios() {
            assert!(s.synthetic, "{} must be labelled synthetic", s.id);
            assert!(!s.samples.is_empty());
        }
    }

    #[test]
    fn the_quiet_scenario_never_alerts() {
        let e = run(&alert_scenarios()[0]);
        assert_eq!(e.state, AlertState::Monitoring);
        assert!(e.memory.history.is_empty());
    }

    #[test]
    fn the_brief_spike_scenario_does_not_qualify() {
        let e = run(&alert_scenarios()[1]);
        assert_eq!(e.state, AlertState::Monitoring);
        assert!(
            e.memory.open_episode.is_none(),
            "a four-minute spike must not open an episode"
        );
    }

    #[test]
    fn the_sustained_scenario_qualifies_exactly_one_episode() {
        let e = run(&alert_scenarios()[2]);
        assert!(matches!(e.state, AlertState::Active { .. }));
        assert!(e.memory.open_episode.is_some());
        assert!(e.memory.history.is_empty(), "one episode, not several");
    }

    #[test]
    fn the_hysteresis_scenario_keeps_the_episode_open() {
        let e = run(&alert_scenarios()[3]);
        assert!(
            matches!(e.state, AlertState::Active { .. }),
            "got {:?}",
            e.state
        );
    }

    #[test]
    fn the_outage_scenario_pauses_without_ending_the_episode() {
        let e = run(&alert_scenarios()[4]);
        match e.state {
            AlertState::DataUnavailable {
                reason,
                ref retained_episode,
            } => {
                assert!(matches!(
                    reason,
                    PauseReason::StaleFeed | PauseReason::InsufficientCoverage
                ));
                assert!(
                    retained_episode.is_some(),
                    "an outage must never end an episode"
                );
            }
            other => panic!("expected a paused detector, got {other:?}"),
        }
    }

    #[test]
    fn the_clearance_scenario_produces_two_distinct_episodes() {
        let e = run(&alert_scenarios()[5]);
        assert!(
            matches!(e.state, AlertState::Active { .. }),
            "got {:?}",
            e.state
        );
        assert_eq!(
            e.memory.history.len(),
            1,
            "the first episode is retained as history"
        );
        let open = e.memory.open_episode.as_ref().unwrap();
        assert_ne!(open.id, e.memory.history[0].id);
    }

    #[test]
    fn the_frozen_dataset_is_genuinely_quiet_so_it_is_not_used_for_alert_demos() {
        // Guards the honesty claim in this module's documentation: the real
        // captured interval contains no threshold crossing.
        let p = payloads();
        let d = crate::snapshot::assemble(
            crate::snapshot::Mode::Demo,
            &p,
            captured_at(),
            "demo".into(),
        );
        let speed = &d.series["noaa-swpc:rtsw_wind_1m:proton_speed"];
        let max = speed
            .samples
            .iter()
            .filter_map(|o| o.value)
            .fold(f64::MIN, f64::max);
        assert!(max < 500.0, "captured interval peaks at {max} km/s");
    }
}
