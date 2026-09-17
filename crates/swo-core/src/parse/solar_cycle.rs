//! Solar Cycle 25 progression: observed monthly sunspot-number and F10.7
//! indices, plus the official consensus prediction panel's expected range.
//!
//! Source: <https://www.spaceweather.gov/products/solar-cycle-progression>.
//! Cadence: monthly rows, revised as preliminary recent months are refined.
//! The provider marks a value not yet computable (a smoothed figure near the
//! present, which needs trailing months that do not exist yet) with a `-1`
//! sentinel; this parser turns that into `None`, the same rule this
//! application applies to every other provider sentinel.

use super::ParseError;
use chrono::NaiveDate;
use serde::Deserialize;

pub const OBSERVED_URL: &str =
    "https://services.swpc.noaa.gov/json/solar-cycle/observed-solar-cycle-indices.json";
pub const PREDICTED_URL: &str =
    "https://services.swpc.noaa.gov/json/solar-cycle/predicted-solar-cycle.json";

fn month(raw: &str) -> Result<NaiveDate, ParseError> {
    NaiveDate::parse_from_str(&format!("{raw}-01"), "%Y-%m-%d").map_err(|_| ParseError::Schema {
        product: "solar-cycle",
        detail: format!("unparseable month {raw:?}"),
    })
}

/// The provider's `-1` (occasionally `-1.0`) "not yet available" sentinel.
/// Every real published value in these products is non-negative.
fn sentinel(v: f64) -> Option<f64> {
    if v < 0.0 {
        None
    } else {
        Some(v)
    }
}

#[derive(Debug, Deserialize)]
struct RawObserved {
    #[serde(rename = "time-tag")]
    time_tag: String,
    ssn: f64,
    smoothed_ssn: f64,
    observed_swpc_ssn: f64,
    smoothed_swpc_ssn: f64,
    #[serde(rename = "f10.7")]
    f10_7: f64,
    #[serde(rename = "smoothed_f10.7")]
    smoothed_f10_7: f64,
}

/// One observed month. SWPC publishes both the international sunspot number
/// (`ssn`) and its own provisional figure (`observed_swpc_ssn`); both are kept,
/// never averaged into a single number. Only `ssn` is published for the whole
/// history back to 1749; `observed_swpc_ssn` and `f10_7` are absent (`-1`
/// sentinel) for most of it and become real only in recent decades.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ObservedMonth {
    pub month: NaiveDate,
    pub ssn: f64,
    pub smoothed_ssn: Option<f64>,
    pub observed_swpc_ssn: Option<f64>,
    pub smoothed_swpc_ssn: Option<f64>,
    pub f10_7: Option<f64>,
    pub smoothed_f10_7: Option<f64>,
}

pub fn parse_observed(payload: &str) -> Result<Vec<ObservedMonth>, ParseError> {
    let raw: Vec<RawObserved> = serde_json::from_str(payload)?;
    if raw.is_empty() {
        return Err(ParseError::Empty("observed-solar-cycle-indices"));
    }
    raw.into_iter()
        .map(|r| {
            Ok(ObservedMonth {
                month: month(&r.time_tag)?,
                ssn: r.ssn,
                smoothed_ssn: sentinel(r.smoothed_ssn),
                observed_swpc_ssn: sentinel(r.observed_swpc_ssn),
                smoothed_swpc_ssn: sentinel(r.smoothed_swpc_ssn),
                f10_7: sentinel(r.f10_7),
                smoothed_f10_7: sentinel(r.smoothed_f10_7),
            })
        })
        .collect()
}

#[derive(Debug, Deserialize)]
struct RawPredicted {
    #[serde(rename = "time-tag")]
    time_tag: String,
    predicted_ssn: f64,
    high_ssn: f64,
    low_ssn: f64,
    #[serde(rename = "predicted_f10.7")]
    predicted_f10_7: f64,
    #[serde(rename = "high_f10.7")]
    high_f10_7: f64,
    #[serde(rename = "low_f10.7")]
    low_f10_7: f64,
}

/// One predicted month from the official consensus panel. `high`/`low` are
/// the panel's own stated expected range, not a computed confidence interval.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct PredictedMonth {
    pub month: NaiveDate,
    pub predicted_ssn: f64,
    pub high_ssn: f64,
    pub low_ssn: f64,
    pub predicted_f10_7: f64,
    pub high_f10_7: f64,
    pub low_f10_7: f64,
}

pub fn parse_predicted(payload: &str) -> Result<Vec<PredictedMonth>, ParseError> {
    let raw: Vec<RawPredicted> = serde_json::from_str(payload)?;
    if raw.is_empty() {
        return Err(ParseError::Empty("predicted-solar-cycle"));
    }
    raw.into_iter()
        .map(|r| {
            Ok(PredictedMonth {
                month: month(&r.time_tag)?,
                predicted_ssn: r.predicted_ssn,
                high_ssn: r.high_ssn,
                low_ssn: r.low_ssn,
                predicted_f10_7: r.predicted_f10_7,
                high_f10_7: r.high_f10_7,
                low_f10_7: r.low_f10_7,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const OBSERVED: &str =
        include_str!("../../../../fixtures/captured/observed_solar_cycle_indices.json");
    const PREDICTED: &str = include_str!("../../../../fixtures/captured/predicted_solar_cycle.json");

    #[test]
    fn observed_fixture_parses_a_long_monthly_history() {
        let months = parse_observed(OBSERVED).unwrap();
        assert!(months.len() > 1000, "the provider publishes since 1749");
        assert_eq!(months[0].month, NaiveDate::from_ymd_opt(1749, 1, 1).unwrap());
        for pair in months.windows(2) {
            assert!(pair[1].month > pair[0].month, "months must be increasing");
        }
    }

    #[test]
    fn a_negative_one_sentinel_becomes_missing_not_a_real_value() {
        let months = parse_observed(OBSERVED).unwrap();
        let first = &months[0];
        assert_eq!(
            first.smoothed_ssn, None,
            "1749 predates the smoothed/SWPC series entirely"
        );
        assert_eq!(first.smoothed_swpc_ssn, None);
        assert_eq!(first.smoothed_f10_7, None);
        assert_eq!(
            first.observed_swpc_ssn, None,
            "the SWPC-specific SSN series does not extend back to 1749 either"
        );
        assert_eq!(
            first.f10_7, None,
            "F10.7 radio flux monitoring did not exist in 1749"
        );

        let recent = months
            .iter()
            .rev()
            .find(|m| m.smoothed_ssn.is_some())
            .expect("older months have a computable smoothed value");
        assert!(recent.smoothed_ssn.unwrap() >= 0.0);
    }

    #[test]
    fn the_most_recent_observed_month_has_real_nonnegative_modern_series() {
        let months = parse_observed(OBSERVED).unwrap();
        let latest = months.last().unwrap();
        assert!(latest.ssn >= 0.0);
        assert!(
            latest.observed_swpc_ssn.is_some_and(|v| v >= 0.0),
            "the modern SWPC SSN series should be populated for the latest month"
        );
        assert!(
            latest.f10_7.is_some_and(|v| v >= 0.0),
            "F10.7 should be populated for the latest month"
        );
    }

    #[test]
    fn predicted_fixture_parses_the_consensus_panel_range() {
        let months = parse_predicted(PREDICTED).unwrap();
        assert!(!months.is_empty());
        for m in &months {
            assert!(
                m.low_ssn <= m.predicted_ssn && m.predicted_ssn <= m.high_ssn,
                "predicted SSN must fall within the panel's own stated range for {}",
                m.month
            );
            assert!(m.low_f10_7 <= m.predicted_f10_7 && m.predicted_f10_7 <= m.high_f10_7);
        }
        for pair in months.windows(2) {
            assert!(pair[1].month > pair[0].month);
        }
    }

    #[test]
    fn an_unparseable_month_is_an_error_not_a_default() {
        let bad = r#"[{"time-tag":"not-a-month","ssn":1.0,"smoothed_ssn":-1.0,"observed_swpc_ssn":1.0,"smoothed_swpc_ssn":-1.0,"f10.7":1.0,"smoothed_f10.7":-1.0}]"#;
        assert!(parse_observed(bad).is_err());
    }
}
