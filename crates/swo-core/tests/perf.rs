//! Performance evidence with realistic seven-day, one-minute-cadence series.
//!
//! Run with `cargo test --release -p swo-core --test perf -- --nocapture` and
//! copy the printed figures into docs/release-readiness.md together with the
//! machine they were measured on. Assertions are deliberately loose upper
//! bounds so CI catches a regression of an order of magnitude, not jitter.

use chrono::{DateTime, Duration, Utc};
use std::time::Instant;
use swo_core::aggregate::downsample_minmax;
use swo_core::alert::{evaluate, AlertMemory, AlertSettings, SourceIdentity};
use swo_core::model::*;
use swo_core::timeline::{default_gap_tolerance, default_tolerance, nearest_within, segments};

const SEVEN_DAYS_MINUTES: i64 = 7 * 24 * 60;

fn seven_day_series() -> Series {
    let start: DateTime<Utc> = DateTime::from_timestamp(1_757_000_000, 0).unwrap();
    let samples: Vec<Observation> = (0..SEVEN_DAYS_MINUTES)
        .map(|m| {
            // Realistic texture: slow variation, noise, occasional missing rows.
            let v = 380.0 + 60.0 * ((m as f64) / 720.0).sin() + ((m * 7919) % 23) as f64;
            let missing = m % 97 == 0;
            Observation::instant(
                start + Duration::minutes(m),
                if missing { None } else { Some(v) },
                if missing {
                    Quality::Missing
                } else {
                    Quality::Good
                },
            )
        })
        .collect();
    Series {
        id: SeriesId::new("noaa-swpc", "rtsw_wind_1m", "proton_speed"),
        label: "Solar-wind speed".into(),
        unit: "km/s".into(),
        frame: None,
        nominal_cadence_seconds: 60,
        aggregation: Aggregation::Raw,
        provenance: Provenance {
            source_url: "https://services.swpc.noaa.gov/json/rtsw/rtsw_wind_1m.json".into(),
            retrieved_at: start + Duration::minutes(SEVEN_DAYS_MINUTES),
            payload_sha256: None,
            issued_at: None,
        },
        samples,
    }
}

fn p95(mut xs: Vec<f64>) -> f64 {
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    xs[(xs.len() as f64 * 0.95) as usize]
}

#[test]
fn selection_lookup_p95_is_well_under_budget() {
    let s = seven_day_series();
    let tol = default_tolerance(s.nominal_cadence_seconds);
    let mut times = Vec::new();
    for i in 0..500 {
        let at = s.samples[(i * 20) % s.samples.len()].time + Duration::seconds(17);
        let t0 = Instant::now();
        let _ = nearest_within(&s, at, tol);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    let p = p95(times);
    println!(
        "nearest_within over {} samples: p95 {:.3} ms",
        s.samples.len(),
        p
    );
    assert!(p < 20.0, "selection lookup p95 {p} ms");
}

#[test]
fn segmenting_a_week_is_fast() {
    let s = seven_day_series();
    let t0 = Instant::now();
    let segs = segments(&s.samples, default_gap_tolerance(60));
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!(
        "segments over {} samples -> {} segments in {:.3} ms",
        s.samples.len(),
        segs.len(),
        ms
    );
    assert!(segs.len() > 100, "missing rows must break segments");
    assert!(ms < 50.0);
}

#[test]
fn downsampling_a_week_to_chart_width_is_fast_and_keeps_the_peak() {
    let mut s = seven_day_series();
    s.samples[5000].value = Some(999.0);
    let t0 = Instant::now();
    let d = downsample_minmax(&s, Duration::minutes(7));
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let max = d
        .samples
        .iter()
        .filter_map(|o| o.value)
        .fold(f64::MIN, f64::max);
    println!(
        "downsample {} -> {} samples in {:.3} ms",
        s.samples.len(),
        d.samples.len(),
        ms
    );
    assert_eq!(max, 999.0);
    assert!(ms < 50.0);
}

#[test]
fn alert_evaluation_over_a_week_is_fast() {
    let s = seven_day_series();
    let settings = AlertSettings::default();
    let src = SourceIdentity {
        product: "rtsw_wind_1m".into(),
        spacecraft: Some("SOLAR1".into()),
    };
    let now = s.samples.last().unwrap().time;
    let mut times = Vec::new();
    let mut memory = AlertMemory {
        settings_version: 1,
        ..Default::default()
    };
    for _ in 0..50 {
        let t0 = Instant::now();
        let e = evaluate(&s.samples, &settings, &memory, &src, now);
        times.push(t0.elapsed().as_secs_f64() * 1000.0);
        memory = e.memory;
    }
    let p = p95(times);
    println!(
        "alert evaluate over {} samples: p95 {:.3} ms",
        s.samples.len(),
        p
    );
    assert!(p < 50.0);
}

#[test]
fn json_serialization_of_a_week_stays_bounded() {
    let s = seven_day_series();
    let t0 = Instant::now();
    let json = serde_json::to_string(&s).unwrap();
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    println!("serialize week: {} bytes in {:.3} ms", json.len(), ms);
    assert!(
        json.len() < 4 * 1024 * 1024,
        "one week of one series must stay under 4 MB on the wire"
    );
}
