//! Display aggregation that preserves peaks (spec §5).
//!
//! Long intervals contain more samples than a chart has pixels. Averaging would
//! flatten a flare maximum, so buckets keep the minimum and maximum of each
//! bucket in time order and mark the series as downsampled. Raw values remain
//! available for inspection and export.

use crate::model::{Aggregation, Observation, Series};
use chrono::Duration;

/// Min/max decimation. Emits up to two samples per bucket (the bucket's
/// extremes, in their original time order), each retaining its real timestamp
/// and quality. Never invents a value.
pub fn downsample_minmax(series: &Series, bucket: Duration) -> Series {
    let bucket_ms = bucket.num_milliseconds().max(1);
    let mut out: Vec<Observation> = Vec::new();
    let mut bucket_start: Option<i64> = None;
    let mut lo: Option<Observation> = None;
    let mut hi: Option<Observation> = None;

    let flush =
        |lo: &mut Option<Observation>, hi: &mut Option<Observation>, out: &mut Vec<Observation>| {
            match (lo.take(), hi.take()) {
                (Some(a), Some(b)) => {
                    if a.time == b.time {
                        out.push(a);
                    } else if a.time < b.time {
                        out.push(a);
                        out.push(b);
                    } else {
                        out.push(b);
                        out.push(a);
                    }
                }
                (Some(a), None) | (None, Some(a)) => out.push(a),
                (None, None) => {}
            }
        };

    for obs in &series.samples {
        let key = obs.time.timestamp_millis().div_euclid(bucket_ms);
        if bucket_start != Some(key) {
            flush(&mut lo, &mut hi, &mut out);
            bucket_start = Some(key);
        }
        match obs.accepted() {
            None => {
                // Preserve gap markers so the chart still breaks the line.
                flush(&mut lo, &mut hi, &mut out);
                out.push(obs.clone());
                bucket_start = None;
            }
            Some(v) => {
                if lo.as_ref().and_then(|o| o.value).is_none_or(|lv| v < lv) {
                    lo = Some(obs.clone());
                }
                if hi.as_ref().and_then(|o| o.value).is_none_or(|hv| v > hv) {
                    hi = Some(obs.clone());
                }
            }
        }
    }
    flush(&mut lo, &mut hi, &mut out);

    let mut result = series.clone();
    result.samples = out;
    result.aggregation = Aggregation::Downsampled {
        method: "min-max decimation (bucket extremes retained in time order; no averaging)".into(),
        bucket_seconds: bucket.num_seconds().max(1),
    };
    result
}

/// Choose a bucket so that a series renders in roughly `target_points` marks.
/// Returns `None` when the raw series is already small enough — raw data is
/// preferred whenever it fits.
pub fn bucket_for(sample_count: usize, span: Duration, target_points: usize) -> Option<Duration> {
    if sample_count <= target_points || target_points == 0 {
        return None;
    }
    let ms = span.num_milliseconds().max(1) / target_points as i64;
    Some(Duration::milliseconds(ms.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Provenance, Quality, SeriesId, TimePrecision};
    use chrono::{DateTime, Utc};

    fn t(min: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_757_000_000 + min * 60, 0).unwrap()
    }

    fn series(vals: &[(i64, Option<f64>)]) -> Series {
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
            samples: vals
                .iter()
                .map(|(m, v)| Observation {
                    time: t(*m),
                    interval_seconds: None,
                    time_precision: TimePrecision::Instant,
                    value: *v,
                    quality: if v.is_some() {
                        Quality::Good
                    } else {
                        Quality::Missing
                    },
                    instrument: None,
                })
                .collect(),
        }
    }

    #[test]
    fn peak_survives_downsampling() {
        // A single 60-minute bucket containing one large spike.
        let mut vals: Vec<(i64, Option<f64>)> = (0..60).map(|m| (m, Some(1.0))).collect();
        vals[30] = (30, Some(999.0));
        let out = downsample_minmax(&series(&vals), Duration::minutes(60));
        let max = out
            .samples
            .iter()
            .filter_map(|o| o.value)
            .fold(f64::MIN, f64::max);
        assert_eq!(max, 999.0, "a flare maximum must never be averaged away");
        assert!(
            out.samples.len() <= 4,
            "at most two marks per bucket touched"
        );
        assert!(out.samples.len() < 60, "the series is genuinely reduced");
    }

    #[test]
    fn downsampled_series_is_labelled_as_such() {
        let out = downsample_minmax(
            &series(&[(0, Some(1.0)), (1, Some(2.0))]),
            Duration::minutes(60),
        );
        match out.aggregation {
            Aggregation::Downsampled { bucket_seconds, .. } => assert_eq!(bucket_seconds, 3600),
            other => panic!("expected downsampled marker, got {other:?}"),
        }
    }

    #[test]
    fn timestamps_are_real_sample_times_not_bucket_edges() {
        let out = downsample_minmax(
            &series(&[(3, Some(1.0)), (7, Some(9.0))]),
            Duration::minutes(60),
        );
        let times: Vec<_> = out.samples.iter().map(|o| o.time).collect();
        assert!(times.contains(&t(3)) && times.contains(&t(7)));
    }

    #[test]
    fn gaps_are_preserved_through_aggregation() {
        let out = downsample_minmax(
            &series(&[(0, Some(1.0)), (1, None), (2, Some(2.0))]),
            Duration::minutes(60),
        );
        assert!(out.samples.iter().any(|o| o.quality == Quality::Missing));
    }

    #[test]
    fn raw_data_is_kept_when_it_already_fits() {
        assert!(bucket_for(100, Duration::hours(1), 500).is_none());
        assert!(bucket_for(100_000, Duration::days(7), 1500).is_some());
    }
}
