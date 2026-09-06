//! Time alignment rules shared by every view (spec §5).
//!
//! One selected time is shared across views. Different products have different
//! cadences, so cross-series lookup is always tolerance-bounded and returns
//! "no nearby sample" rather than an unrelated measurement.

use crate::model::{Observation, Quality, Series};
use chrono::{DateTime, Duration, Utc};

/// Result of asking a series for the value at the shared selected time.
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup<'a> {
    /// A real sample within tolerance.
    Sample(&'a Observation),
    /// Samples exist but none within tolerance of the selected time.
    NoNearbySample,
    /// The series has no samples at all.
    Empty,
}

/// Nearest accepted sample to `at`, within `tolerance`. Never interpolates.
pub fn nearest_within<'a>(
    series: &'a Series,
    at: DateTime<Utc>,
    tolerance: Duration,
) -> Lookup<'a> {
    if series.samples.is_empty() {
        return Lookup::Empty;
    }
    let mut best: Option<(&Observation, i64)> = None;
    for obs in &series.samples {
        if obs.accepted().is_none() {
            continue;
        }
        let delta = (obs.time - at).num_milliseconds().abs();
        if delta <= tolerance.num_milliseconds() {
            match best {
                Some((_, d)) if d <= delta => {}
                _ => best = Some((obs, delta)),
            }
        }
    }
    match best {
        Some((obs, _)) => Lookup::Sample(obs),
        None => Lookup::NoNearbySample,
    }
}

/// Default lookup tolerance for a product: one nominal cadence, floored at 60s.
/// Documented rather than implicit so the UI can state it.
pub fn default_tolerance(nominal_cadence_seconds: i64) -> Duration {
    Duration::seconds(nominal_cadence_seconds.max(60))
}

/// A contiguous run of samples. Charts draw one polyline per segment so lines
/// are never drawn across a significant data gap (spec §5).
#[derive(Debug, Clone, PartialEq)]
pub struct Segment {
    pub start_index: usize,
    pub end_index: usize,
}

/// Split samples into segments, breaking where the gap between consecutive
/// accepted samples exceeds `gap_tolerance`, or where a sample is missing.
pub fn segments(samples: &[Observation], gap_tolerance: Duration) -> Vec<Segment> {
    let mut out: Vec<Segment> = Vec::new();
    let mut current: Option<Segment> = None;
    let mut prev_time: Option<DateTime<Utc>> = None;

    for (i, obs) in samples.iter().enumerate() {
        let usable = obs.accepted().is_some();
        if !usable {
            if let Some(seg) = current.take() {
                out.push(seg);
            }
            prev_time = None;
            continue;
        }
        let breaks = match prev_time {
            Some(p) => (obs.time - p) > gap_tolerance,
            None => true,
        };
        if breaks {
            if let Some(seg) = current.take() {
                out.push(seg);
            }
            current = Some(Segment {
                start_index: i,
                end_index: i,
            });
        } else if let Some(seg) = current.as_mut() {
            seg.end_index = i;
        }
        prev_time = Some(obs.time);
    }
    if let Some(seg) = current {
        out.push(seg);
    }
    out
}

/// Gap tolerance for segmenting: 2.5x nominal cadence. A single missed sample
/// keeps the line connected; a real outage breaks it.
pub fn default_gap_tolerance(nominal_cadence_seconds: i64) -> Duration {
    Duration::milliseconds((nominal_cadence_seconds as f64 * 2500.0) as i64)
}

/// Normalize a raw sample list: sort ascending, and resolve duplicate
/// timestamps by keeping the last-supplied row (provider corrections arrive
/// later in the same payload). Stable so ordering of equal keys is preserved.
pub fn normalize_samples(mut samples: Vec<Observation>) -> Vec<Observation> {
    samples.sort_by_key(|o| o.time);
    let mut out: Vec<Observation> = Vec::with_capacity(samples.len());
    for obs in samples {
        match out.last_mut() {
            Some(prev) if prev.time == obs.time => {
                // Later row wins, except that a real value is never replaced by
                // a missing one at the same timestamp.
                if !(obs.quality == Quality::Missing && prev.quality != Quality::Missing) {
                    *prev = obs;
                }
            }
            _ => out.push(obs),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Aggregation, Provenance, Quality, SeriesId};

    fn t(min: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_757_000_000 + min * 60, 0).unwrap()
    }

    fn obs(min: i64, v: Option<f64>) -> Observation {
        let q = if v.is_some() {
            Quality::Good
        } else {
            Quality::Missing
        };
        Observation::instant(t(min), v, q)
    }

    fn series(samples: Vec<Observation>) -> Series {
        Series {
            id: SeriesId::new("p", "prod", "m"),
            label: "m".into(),
            unit: "u".into(),
            frame: None,
            nominal_cadence_seconds: 60,
            aggregation: Aggregation::Raw,
            provenance: Provenance {
                source_url: "https://example.invalid".into(),
                retrieved_at: t(0),
                payload_sha256: None,
                issued_at: None,
            },
            samples,
        }
    }

    #[test]
    fn nearest_returns_closest_accepted_sample() {
        let s = series(vec![obs(0, Some(1.0)), obs(5, Some(2.0))]);
        match nearest_within(&s, t(4), Duration::seconds(120)) {
            Lookup::Sample(o) => assert_eq!(o.value, Some(2.0)),
            other => panic!("expected sample, got {other:?}"),
        }
    }

    #[test]
    fn nearest_reports_no_nearby_sample_instead_of_substituting() {
        let s = series(vec![obs(0, Some(1.0))]);
        assert_eq!(
            nearest_within(&s, t(60), Duration::seconds(120)),
            Lookup::NoNearbySample
        );
    }

    #[test]
    fn nearest_skips_missing_values() {
        let s = series(vec![obs(0, Some(1.0)), obs(1, None)]);
        match nearest_within(&s, t(1), Duration::seconds(120)) {
            Lookup::Sample(o) => assert_eq!(o.value, Some(1.0)),
            other => panic!("expected fallback to accepted sample, got {other:?}"),
        }
    }

    #[test]
    fn empty_series_is_distinguishable_from_no_nearby_sample() {
        assert_eq!(
            nearest_within(&series(vec![]), t(0), Duration::seconds(60)),
            Lookup::Empty
        );
    }

    #[test]
    fn segments_break_across_gaps_and_missing_values() {
        let samples = vec![
            obs(0, Some(1.0)),
            obs(1, Some(2.0)),
            // 10-minute outage
            obs(11, Some(3.0)),
            obs(12, None),
            obs(13, Some(4.0)),
        ];
        let segs = segments(&samples, default_gap_tolerance(60));
        assert_eq!(
            segs.len(),
            3,
            "gap and missing value must both break the line"
        );
        assert_eq!(
            segs[0],
            Segment {
                start_index: 0,
                end_index: 1
            }
        );
        assert_eq!(
            segs[1],
            Segment {
                start_index: 2,
                end_index: 2
            }
        );
        assert_eq!(
            segs[2],
            Segment {
                start_index: 4,
                end_index: 4
            }
        );
    }

    #[test]
    fn normalize_sorts_and_resolves_duplicates_keeping_latest_real_value() {
        let out = normalize_samples(vec![
            obs(5, Some(5.0)),
            obs(0, Some(1.0)),
            obs(5, Some(9.0)),
            obs(0, None),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(
            out[0].value,
            Some(1.0),
            "a real value is not replaced by a null"
        );
        assert_eq!(out[1].value, Some(9.0), "a later correction wins");
    }
}
