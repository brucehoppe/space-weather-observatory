//! Planetary K-index products.
//!
//! Sources, reached from <https://www.swpc.noaa.gov/products/planetary-k-index>:
//! - `products/noaa-planetary-k-index.json` — provider-estimated Kp with the
//!   running a-index and contributing station count.
//! - `products/noaa-planetary-k-index-forecast.json` — a single series whose
//!   `observed` field separates `observed`, `estimated` and `predicted` values.
//!
//! Kp is an **interval-valued** index: each value describes a 3-hour UT
//! interval, not an instant, and it is not a continuously sampled local
//! magnetic field (spec §7). Interval start and length are preserved.

use super::{parse_swpc_time, ParseError};
use crate::model::{Observation, Quality, TimePrecision};
use serde::Deserialize;

pub const KP_URL: &str = "https://services.swpc.noaa.gov/products/noaa-planetary-k-index.json";
pub const KP_FORECAST_URL: &str =
    "https://services.swpc.noaa.gov/products/noaa-planetary-k-index-forecast.json";
/// Kp intervals are three hours.
pub const INTERVAL_SECONDS: i64 = 3 * 3600;

/// How the provider characterises a Kp value. These are never merged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KpKind {
    /// Derived from reporting observatories after the fact.
    Observed,
    /// NOAA's estimate for a recent interval, pending definitive values.
    Estimated,
    /// A forecast for a future interval.
    Predicted,
}

impl KpKind {
    fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "observed" => Some(KpKind::Observed),
            "estimated" => Some(KpKind::Estimated),
            "predicted" => Some(KpKind::Predicted),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            KpKind::Observed => "observed",
            KpKind::Estimated => "NOAA estimate",
            KpKind::Predicted => "forecast",
        }
    }

    pub fn is_forecast(self) -> bool {
        matches!(self, KpKind::Predicted)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct KpInterval {
    pub observation: Observation,
    pub kind: KpKind,
    /// NOAA G-scale label supplied with the value, e.g. `G1`. Only present when
    /// the provider states one; never derived by this application.
    pub noaa_scale: Option<String>,
    pub station_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct KpRow {
    time_tag: String,
    #[serde(rename = "Kp")]
    kp: Option<f64>,
    station_count: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct KpForecastRow {
    time_tag: String,
    kp: Option<f64>,
    observed: String,
    noaa_scale: Option<String>,
}

fn interval_observation(time_raw: &str, kp: Option<f64>) -> Result<Observation, ParseError> {
    let time = parse_swpc_time(time_raw)?;
    let (value, quality) = match kp {
        Some(v) if v.is_finite() && (0.0..=9.0).contains(&v) => (Some(v), Quality::Good),
        Some(_) => (None, Quality::Missing),
        None => (None, Quality::Missing),
    };
    Ok(Observation {
        time,
        interval_seconds: Some(INTERVAL_SECONDS),
        time_precision: TimePrecision::Interval,
        value,
        quality,
        instrument: None,
    })
}

/// Parse the estimated-Kp product. Every value here is a NOAA estimate, so all
/// entries are tagged `Estimated` rather than being called observations.
pub fn parse_kp(payload: &str) -> Result<Vec<KpInterval>, ParseError> {
    let rows: Vec<KpRow> = serde_json::from_str(payload)?;
    if rows.is_empty() {
        return Err(ParseError::Empty("planetary k-index"));
    }
    rows.into_iter()
        .map(|r| {
            Ok(KpInterval {
                observation: interval_observation(&r.time_tag, r.kp)?,
                kind: KpKind::Estimated,
                noaa_scale: None,
                station_count: r.station_count,
            })
        })
        .collect()
}

/// Parse the combined observed/estimated/forecast product.
pub fn parse_kp_forecast(payload: &str) -> Result<Vec<KpInterval>, ParseError> {
    let rows: Vec<KpForecastRow> = serde_json::from_str(payload)?;
    if rows.is_empty() {
        return Err(ParseError::Empty("planetary k-index forecast"));
    }
    rows.into_iter()
        .map(|r| {
            let kind = KpKind::parse(&r.observed).ok_or_else(|| ParseError::Schema {
                product: "planetary k-index forecast",
                detail: format!("unknown observed state {:?}", r.observed),
            })?;
            Ok(KpInterval {
                observation: interval_observation(&r.time_tag, r.kp)?,
                kind,
                noaa_scale: r.noaa_scale.filter(|s| !s.trim().is_empty()),
                station_count: None,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KP: &str = include_str!("../../../../fixtures/captured/planetary_k_index.json");
    const KPF: &str = include_str!("../../../../fixtures/captured/planetary_k_index_forecast.json");

    #[test]
    fn kp_values_are_interval_valued_with_a_three_hour_span() {
        let kp = parse_kp(KP).unwrap();
        assert!(!kp.is_empty());
        for i in &kp {
            assert_eq!(i.observation.time_precision, TimePrecision::Interval);
            assert_eq!(i.observation.interval_seconds, Some(10_800));
        }
    }

    #[test]
    fn consecutive_intervals_are_exactly_three_hours_apart() {
        let kp = parse_kp(KP).unwrap();
        for w in kp.windows(2) {
            let d = w[1].observation.time - w[0].observation.time;
            assert_eq!(d.num_seconds(), 10_800, "interval boundaries must line up");
        }
    }

    #[test]
    fn forecast_product_separates_observed_estimated_and_predicted() {
        let f = parse_kp_forecast(KPF).unwrap();
        let observed = f.iter().filter(|i| i.kind == KpKind::Observed).count();
        let predicted = f.iter().filter(|i| i.kind == KpKind::Predicted).count();
        assert!(
            observed > 0 && predicted > 0,
            "the product mixes both and they must stay separate"
        );
        // Every predicted interval must be later than every observed one.
        let last_observed = f
            .iter()
            .filter(|i| i.kind == KpKind::Observed)
            .map(|i| i.observation.time)
            .max()
            .unwrap();
        let first_predicted = f
            .iter()
            .filter(|i| i.kind == KpKind::Predicted)
            .map(|i| i.observation.time)
            .min()
            .unwrap();
        assert!(first_predicted > last_observed);
    }

    #[test]
    fn published_scale_labels_are_preserved_and_never_invented() {
        let f = parse_kp_forecast(KPF).unwrap();
        assert!(
            f.iter().any(|i| i.noaa_scale.is_some()),
            "fixture contains published G labels"
        );
        for i in &f {
            if let Some(s) = &i.noaa_scale {
                assert!(
                    s.starts_with('G'),
                    "only the provider's own label is carried: {s}"
                );
            }
        }
    }

    #[test]
    fn out_of_range_kp_is_missing_not_clamped() {
        let payload = r#"[{"time_tag":"2026-09-06T00:00:00","Kp":99.0,"station_count":8},
                          {"time_tag":"2026-09-06T03:00:00","Kp":null,"station_count":null}]"#;
        let kp = parse_kp(payload).unwrap();
        assert_eq!(kp[0].observation.quality, Quality::Missing);
        assert_eq!(kp[1].observation.quality, Quality::Missing);
    }

    #[test]
    fn an_unknown_observed_state_is_rejected() {
        let payload = r#"[{"time_tag":"2026-09-06T00:00:00","kp":3.0,"observed":"guessed","noaa_scale":null}]"#;
        assert!(parse_kp_forecast(payload).is_err());
    }

    #[test]
    fn estimated_product_is_not_labelled_as_definitive_observation() {
        let kp = parse_kp(KP).unwrap();
        assert!(kp.iter().all(|i| i.kind == KpKind::Estimated));
    }
}
