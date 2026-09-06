//! Deterministic acceptance tests for the solar-wind alert (spec §13B).
//! No clock, no network, no UI: every case is a fixed sample list.

use super::*;
use crate::model::Quality;

fn base() -> DateTime<Utc> {
    DateTime::from_timestamp(1_757_000_000, 0).unwrap()
}

fn t(min: i64) -> DateTime<Utc> {
    base() + Duration::minutes(min)
}

fn src() -> SourceIdentity {
    SourceIdentity {
        product: "rtsw_wind_1m".into(),
        spacecraft: Some("SOLAR1".into()),
    }
}

/// One accepted sample per minute over `mins`, speed from `f`.
fn ramp(mins: i64, f: impl Fn(i64) -> f64) -> Vec<Observation> {
    (0..mins)
        .map(|m| speed_observation(t(m), Some(f(m)), true))
        .collect()
}

fn settings() -> AlertSettings {
    AlertSettings::default()
}

fn eval(
    samples: &[Observation],
    s: &AlertSettings,
    prior: &AlertMemory,
    now_min: i64,
) -> Evaluation {
    evaluate(samples, s, prior, &src(), t(now_min))
}

fn mem() -> AlertMemory {
    AlertMemory {
        settings_version: 1,
        ..Default::default()
    }
}

// --- Threshold and persistence ------------------------------------------------

#[test]
fn below_threshold_is_monitoring() {
    let s = ramp(30, |_| 420.0);
    let e = eval(&s, &settings(), &mem(), 29);
    assert_eq!(e.state, AlertState::Monitoring);
    assert!(!e.newly_active);
}

#[test]
fn exact_entry_boundary_counts_as_at_or_above() {
    // Exactly 500.0 for 10 full minutes qualifies: entry is `>=`.
    let s = ramp(31, |m| if m >= 20 { 500.0 } else { 400.0 });
    let e = eval(&s, &settings(), &mem(), 30);
    match e.state {
        AlertState::Active { ref episode } => assert_eq!(episode.qualified_onset, t(20)),
        other => panic!("expected Active at exactly the threshold, got {other:?}"),
    }
}

#[test]
fn one_hundredth_below_threshold_does_not_enter() {
    let s = ramp(31, |m| if m >= 20 { 499.99 } else { 400.0 });
    assert_eq!(
        eval(&s, &settings(), &mem(), 30).state,
        AlertState::Monitoring
    );
}

#[test]
fn brief_spike_never_becomes_active() {
    // Above threshold for 4 minutes only, with a 10-minute requirement.
    let s = ramp(40, |m| if (20..24).contains(&m) { 620.0 } else { 430.0 });
    assert_eq!(
        eval(&s, &settings(), &mem(), 39).state,
        AlertState::Monitoring
    );
}

#[test]
fn spike_in_progress_reports_pending_with_remaining_time() {
    let s = ramp(25, |m| if m >= 21 { 610.0 } else { 430.0 });
    match eval(&s, &settings(), &mem(), 24).state {
        AlertState::Pending {
            since,
            elapsed_seconds,
            required_seconds,
        } => {
            assert_eq!(since, t(21));
            assert_eq!(elapsed_seconds, 180);
            assert_eq!(required_seconds, 600);
        }
        other => panic!("expected Pending, got {other:?}"),
    }
}

#[test]
fn sustained_crossing_activates_and_records_evidence() {
    let s = ramp(45, |m| {
        if m >= 20 {
            550.0 + (m - 20) as f64
        } else {
            430.0
        }
    });
    let e = eval(&s, &settings(), &mem(), 44);
    match e.state {
        AlertState::Active { ref episode } => {
            assert_eq!(episode.qualified_onset, t(20));
            assert_eq!(episode.onset_speed_km_s, 550.0);
            assert_eq!(episode.peak_speed_km_s, 574.0);
            assert_eq!(episode.source_product, "rtsw_wind_1m");
            assert_eq!(episode.source_spacecraft.as_deref(), Some("SOLAR1"));
            assert_eq!(episode.rule_version, RULE_VERSION);
        }
        other => panic!("expected Active, got {other:?}"),
    }
    assert!(e.newly_active, "first qualification announces once");
    assert!(e.coverage.as_ref().unwrap().adequate);
}

// --- Hysteresis and clearance -------------------------------------------------

#[test]
fn hysteresis_band_retains_the_qualified_state() {
    // Drops to 480: below the 500 entry threshold but above 500-25=475.
    let mut s = ramp(40, |m| if m >= 10 { 560.0 } else { 430.0 });
    let e1 = eval(&s, &settings(), &mem(), 39);
    let m1 = e1.memory;
    s.extend((40..50).map(|m| speed_observation(t(m), Some(480.0), true)));
    let e2 = eval(&s, &settings(), &m1, 49);
    match e2.state {
        AlertState::Active { ref episode } => assert_eq!(episode.last_speed_km_s, 480.0),
        other => panic!("hysteresis band must not clear the episode, got {other:?}"),
    }
    assert!(!e2.newly_active, "an in-place update is not a new alert");
}

#[test]
fn clearance_requires_below_band_for_the_full_clearance_window() {
    let mut s = ramp(40, |m| if m >= 10 { 560.0 } else { 430.0 });
    let m1 = eval(&s, &settings(), &mem(), 39).memory;

    // Four minutes below the clearing bound: not yet five.
    s.extend((40..44).map(|m| speed_observation(t(m), Some(470.0), true)));
    match eval(&s, &settings(), &m1, 43).state {
        AlertState::Active { .. } => {}
        other => panic!("must not clear before the clearance window elapses, got {other:?}"),
    }

    // Full window below 475.
    s.extend((44..50).map(|m| speed_observation(t(m), Some(470.0), true)));
    let e = eval(&s, &settings(), &m1, 49);
    match e.state {
        AlertState::Cleared { ref episode } => {
            assert!(episode.cleared_at.is_some());
            assert!(episode.clearance_evidence.as_ref().unwrap().adequate);
        }
        other => panic!("expected Cleared, got {other:?}"),
    }
    assert!(e.memory.open_episode.is_none());
    assert_eq!(
        e.memory.history.len(),
        1,
        "cleared episode is retained as history"
    );
}

#[test]
fn exact_clearance_boundary_does_not_clear() {
    // Exactly at 475.0 == threshold - hysteresis. Clearing requires strictly below.
    let mut s = ramp(40, |m| if m >= 10 { 560.0 } else { 430.0 });
    let m1 = eval(&s, &settings(), &mem(), 39).memory;
    s.extend((40..50).map(|m| speed_observation(t(m), Some(475.0), true)));
    match eval(&s, &settings(), &m1, 49).state {
        AlertState::Active { .. } => {}
        other => panic!("clearing is `< threshold - hysteresis`, got {other:?}"),
    }
}

#[test]
fn clearance_then_new_crossing_creates_a_distinct_episode() {
    let mut s = ramp(40, |m| if m >= 10 { 560.0 } else { 430.0 });
    let first = eval(&s, &settings(), &mem(), 39);
    let first_id = match &first.state {
        AlertState::Active { episode } => episode.id.clone(),
        other => panic!("{other:?}"),
    };
    s.extend((40..50).map(|m| speed_observation(t(m), Some(400.0), true)));
    let cleared = eval(&s, &settings(), &first.memory, 49);
    assert!(matches!(cleared.state, AlertState::Cleared { .. }));

    s.extend((50..75).map(|m| speed_observation(t(m), Some(600.0), true)));
    let second = eval(&s, &settings(), &cleared.memory, 74);
    match second.state {
        AlertState::Active { ref episode } => {
            assert_ne!(
                episode.id, first_id,
                "a genuinely new crossing is a new episode"
            );
            assert!(second.newly_active);
        }
        other => panic!("expected a second Active episode, got {other:?}"),
    }
}

// --- Data quality, gaps, ordering, corrections --------------------------------

#[test]
fn duplicate_samples_do_not_manufacture_persistence() {
    // The same one sample repeated 30 times is one minute of coverage, not 30.
    let s: Vec<Observation> = (0..30)
        .map(|_| speed_observation(t(20), Some(700.0), true))
        .collect();
    let e = eval(&s, &settings(), &mem(), 20);
    assert!(
        matches!(e.state, AlertState::Pending { .. }),
        "repeated copies of one sample are not elapsed time, got {:?}",
        e.state
    );
}

#[test]
fn out_of_order_samples_are_handled_by_measurement_time() {
    let mut s = ramp(45, |m| if m >= 20 { 560.0 } else { 430.0 });
    s.reverse();
    match eval(&s, &settings(), &mem(), 44).state {
        AlertState::Active { ref episode } => assert_eq!(episode.qualified_onset, t(20)),
        other => panic!("expected Active despite input ordering, got {other:?}"),
    }
}

#[test]
fn cadence_gap_inside_the_window_reports_insufficient_data() {
    // Above threshold at both ends, but a 20-minute hole in the middle of the
    // persistence window.
    let mut s: Vec<Observation> = (0..10)
        .map(|m| speed_observation(t(m), Some(600.0), true))
        .collect();
    s.extend((30..34).map(|m| speed_observation(t(m), Some(600.0), true)));
    let e = eval(&s, &settings(), &mem(), 33);
    match e.state {
        AlertState::DataUnavailable {
            reason: PauseReason::InsufficientCoverage,
            ..
        } => {}
        other => panic!("expected insufficient recent data, got {other:?}"),
    }
}

#[test]
fn null_and_invalid_values_are_missing_not_zero() {
    let o = speed_observation(t(0), None, true);
    assert_eq!(o.quality, Quality::Missing);
    assert_eq!(o.accepted(), None);
    assert_eq!(
        speed_observation(t(0), Some(-9999.0), true).quality,
        Quality::Missing
    );
    assert_eq!(
        speed_observation(t(0), Some(f64::NAN), true).quality,
        Quality::Missing
    );
}

#[test]
fn provider_flagged_samples_are_suspect_but_usable_and_marked() {
    let o = speed_observation(t(0), Some(600.0), false);
    assert_eq!(o.quality, Quality::Suspect);
    assert_eq!(o.accepted(), Some(600.0));
}

#[test]
fn stale_feed_pauses_evaluation_and_retains_the_episode() {
    let s = ramp(40, |m| if m >= 10 { 700.0 } else { 430.0 });
    let m1 = eval(&s, &settings(), &mem(), 39).memory;
    // No new samples for an hour.
    let e = eval(&s, &settings(), &m1, 99);
    match e.state {
        AlertState::DataUnavailable {
            reason: PauseReason::StaleFeed,
            ref retained_episode,
        } => {
            assert!(
                retained_episode.is_some(),
                "an outage never ends an episode"
            );
        }
        other => panic!("expected StaleFeed pause, got {other:?}"),
    }
    assert!(e.memory.open_episode.is_some());
}

#[test]
fn feed_outage_then_recovery_resumes_the_same_episode() {
    let mut s = ramp(40, |m| if m >= 10 { 700.0 } else { 430.0 });
    let first = eval(&s, &settings(), &mem(), 39);
    let id = match &first.state {
        AlertState::Active { episode } => episode.id.clone(),
        o => panic!("{o:?}"),
    };

    // 45-minute outage, then fresh above-threshold coverage.
    s.extend((85..100).map(|m| speed_observation(t(m), Some(700.0), true)));
    let resumed = eval(&s, &settings(), &first.memory, 99);
    match resumed.state {
        AlertState::Active { ref episode } => {
            assert_eq!(
                episode.id, id,
                "the same episode resumes rather than duplicating"
            );
            assert_eq!(episode.last_observation_time, t(99));
        }
        other => panic!("expected the episode to resume, got {other:?}"),
    }
    assert!(
        !resumed.newly_active,
        "recovery is not a second intrusive alert"
    );
}

#[test]
fn no_data_at_all_is_reported_as_such() {
    let e = eval(&[], &settings(), &mem(), 0);
    assert!(matches!(
        e.state,
        AlertState::DataUnavailable {
            reason: PauseReason::NoData,
            ..
        }
    ));
}

#[test]
fn retrospective_correction_updates_history_without_a_new_banner() {
    let s = ramp(45, |m| if m >= 20 { 560.0 } else { 430.0 });
    let e = eval(&s, &settings(), &mem(), 44);
    let mut memory = e.memory;
    let id = memory.open_episode.as_ref().unwrap().id.clone();
    apply_correction(&mut memory, &id, 588.0);
    assert_eq!(memory.open_episode.as_ref().unwrap().peak_speed_km_s, 588.0);

    let again = eval(&s, &settings(), &memory, 44);
    assert!(
        !again.newly_active,
        "a correction must not re-raise the banner"
    );
}

// --- Dismissal, restart, settings ---------------------------------------------

#[test]
fn one_alert_per_episode_across_refreshes() {
    let s = ramp(45, |m| if m >= 20 { 560.0 } else { 430.0 });
    let first = eval(&s, &settings(), &mem(), 44);
    assert!(first.newly_active);
    let second = eval(&s, &settings(), &first.memory, 44);
    assert!(
        !second.newly_active,
        "refreshing must update in place, not re-announce"
    );
}

#[test]
fn dismissal_persists_across_refresh_and_restart_and_cannot_clear() {
    let s = ramp(45, |m| if m >= 20 { 560.0 } else { 430.0 });
    let e = eval(&s, &settings(), &mem(), 44);
    let mut memory = e.memory;
    let id = memory.open_episode.as_ref().unwrap().id.clone();
    acknowledge(&mut memory, &id);
    assert!(memory.open_episode.as_ref().unwrap().acknowledged);

    // Simulated restart: memory reloaded from storage, fresh data re-evaluated.
    let after_restart = eval(&s, &settings(), &memory, 44);
    match after_restart.state {
        AlertState::Active { ref episode } => {
            assert!(episode.acknowledged, "dismissal survives restart");
            assert_eq!(
                episode.id, id,
                "the condition is still active after dismissal"
            );
        }
        other => panic!("dismissal must not clear the condition, got {other:?}"),
    }
    assert!(!after_restart.newly_active);
}

#[test]
fn restart_with_stale_memory_does_not_resurrect_an_old_banner() {
    // Memory holds a cleared episode; fresh data is quiet.
    let quiet = ramp(30, |_| 380.0);
    let mut memory = mem();
    memory.history.push(Episode {
        id: "ep-old-v1".into(),
        qualified_onset: t(-500),
        onset_speed_km_s: 700.0,
        last_observation_time: t(-460),
        last_speed_km_s: 690.0,
        peak_speed_km_s: 720.0,
        source_product: "rtsw_wind_1m".into(),
        source_spacecraft: Some("ACE".into()),
        settings_version: 1,
        rule_version: RULE_VERSION.into(),
        acknowledged: true,
        cleared_at: Some(t(-455)),
        clearance_evidence: None,
    });
    let e = eval(&quiet, &settings(), &memory, 29);
    assert_eq!(e.state, AlertState::Monitoring);
    assert_eq!(
        e.memory.history.len(),
        1,
        "history is retained, not replayed"
    );
}

#[test]
fn changing_settings_resets_pending_and_leaves_past_episodes_intact() {
    let s = ramp(45, |m| if m >= 20 { 560.0 } else { 430.0 });
    let first = eval(&s, &settings(), &mem(), 44);
    let old_id = match &first.state {
        AlertState::Active { episode } => episode.id.clone(),
        o => panic!("{o:?}"),
    };

    let mut changed = settings();
    changed.entry_threshold_km_s = 550.0;
    changed.settings_version = 2;
    let after = eval(&s, &changed, &first.memory, 44);

    assert!(
        after.memory.history.iter().any(|e| e.id == old_id),
        "the previous episode is preserved under the settings it ran with"
    );
    match after.state {
        AlertState::Active { ref episode } => {
            assert_eq!(episode.settings_version, 2);
            assert_ne!(
                episode.id, old_id,
                "re-evaluation under new settings is a new episode"
            );
        }
        other => panic!("expected re-evaluation under new settings, got {other:?}"),
    }
    let preserved = after
        .memory
        .history
        .iter()
        .find(|e| e.id == old_id)
        .unwrap();
    assert_eq!(
        preserved.settings_version, 1,
        "past episodes are not rewritten"
    );
}

#[test]
fn disabling_stops_evaluation_and_keeps_history() {
    let s = ramp(45, |m| if m >= 20 { 700.0 } else { 430.0 });
    let active = eval(&s, &settings(), &mem(), 44);
    let mut off = settings();
    off.enabled = false;
    let e = eval(&s, &off, &active.memory, 44);
    assert_eq!(e.state, AlertState::Disabled);
    assert_eq!(e.memory.history.len(), 1);
    assert!(e.memory.open_episode.is_none());
}

// --- Settings validation ------------------------------------------------------

#[test]
fn settings_are_validated_not_coerced() {
    let mut s = settings();
    assert!(s.validate().is_ok());

    s.entry_threshold_km_s = 50.0;
    assert_eq!(s.validate(), Err(SettingsError::ThresholdOutOfRange));

    s = settings();
    s.entry_threshold_km_s = f64::NAN;
    assert_eq!(s.validate(), Err(SettingsError::NonFinite));

    s = settings();
    s.hysteresis_km_s = 600.0;
    assert_eq!(s.validate(), Err(SettingsError::HysteresisOutOfRange));

    s = settings();
    s.persistence_minutes = 0;
    assert_eq!(s.validate(), Err(SettingsError::PersistenceOutOfRange));

    s = settings();
    s.clearance_minutes = 5000;
    assert_eq!(s.validate(), Err(SettingsError::ClearanceOutOfRange));
}

#[test]
fn documented_defaults_match_the_specification() {
    let d = AlertSettings::default();
    assert_eq!(d.entry_threshold_km_s, 500.0);
    assert_eq!(d.persistence_minutes, 10);
    assert_eq!(d.hysteresis_km_s, 25.0);
    assert_eq!(d.clearance_minutes, 5);
    assert_eq!(d.clear_below_km_s(), 475.0);
    assert_eq!(d.gap_tolerance(), Duration::seconds(180));
}

// --- Source changes -----------------------------------------------------------

#[test]
fn spacecraft_change_is_recorded_on_the_episode() {
    let s = ramp(45, |m| if m >= 20 { 560.0 } else { 430.0 });
    let other = SourceIdentity {
        product: "rtsw_wind_1m".into(),
        spacecraft: Some("ACE".into()),
    };
    let e = evaluate(&s, &settings(), &mem(), &other, t(44));
    match e.state {
        AlertState::Active { ref episode } => {
            assert_eq!(episode.source_spacecraft.as_deref(), Some("ACE"))
        }
        other => panic!("{other:?}"),
    }
}
