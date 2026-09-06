//! NOAA G / R / S scale status and short-range probabilities.
//!
//! Source: <https://services.swpc.noaa.gov/products/noaa-scales.json>, described
//! at <https://www.swpc.noaa.gov/noaa-scales-explanation>.
//!
//! The payload is keyed by day offset: `"0"` is current observed status,
//! `"1"`..`"3"` are forecast days. The three domains are always kept apart; the
//! application never combines them into a single storm score (spec §6).

use super::{parse_swpc_time, ParseError};
use crate::model::{NoaaScale, ScaleDomain};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use std::collections::BTreeMap;

pub const URL: &str = "https://services.swpc.noaa.gov/products/noaa-scales.json";

#[derive(Debug, Deserialize)]
struct RawDay {
    #[serde(rename = "DateStamp")]
    date_stamp: Option<String>,
    #[serde(rename = "TimeStamp")]
    time_stamp: Option<String>,
    #[serde(rename = "R")]
    r: Option<RawR>,
    #[serde(rename = "S")]
    s: Option<RawS>,
    #[serde(rename = "G")]
    g: Option<RawG>,
}

#[derive(Debug, Deserialize)]
struct RawR {
    #[serde(rename = "Scale")]
    scale: Option<String>,
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "MinorProb")]
    minor_prob: Option<String>,
    #[serde(rename = "MajorProb")]
    major_prob: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawS {
    #[serde(rename = "Scale")]
    scale: Option<String>,
    #[serde(rename = "Text")]
    text: Option<String>,
    #[serde(rename = "Prob")]
    prob: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawG {
    #[serde(rename = "Scale")]
    scale: Option<String>,
    #[serde(rename = "Text")]
    text: Option<String>,
}

/// One domain's entry for one day.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DomainStatus {
    pub domain: ScaleDomain,
    /// Present only when the provider states a level for this day.
    pub scale: Option<NoaaScale>,
    /// The provider's own wording, e.g. `none`, `minor`.
    pub text: Option<String>,
    /// Provider-supplied probabilities in percent, with their own labels.
    /// Never invented, never converted into a confidence score.
    pub probabilities: Vec<(String, f64)>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ScaleDay {
    /// 0 = current status; 1..3 = forecast days; -1 = the previous day, which
    /// SWPC also publishes.
    pub day_offset: i8,
    /// Provider timestamp for the entry, when supplied.
    pub time: Option<DateTime<Utc>>,
    pub g: DomainStatus,
    pub r: DomainStatus,
    pub s: DomainStatus,
}

fn level(raw: &Option<String>, domain: ScaleDomain) -> Option<NoaaScale> {
    let s = raw.as_ref()?.trim();
    let level: u8 = s.parse().ok()?;
    if level > 5 {
        return None;
    }
    Some(NoaaScale { domain, level })
}

fn percent(raw: &Option<String>, label: &str) -> Option<(String, f64)> {
    let v: f64 = raw.as_ref()?.trim().parse().ok()?;
    if !(0.0..=100.0).contains(&v) {
        return None;
    }
    Some((label.to_string(), v))
}

pub fn parse(payload: &str) -> Result<Vec<ScaleDay>, ParseError> {
    let raw: BTreeMap<String, RawDay> = serde_json::from_str(payload)?;
    if raw.is_empty() {
        return Err(ParseError::Empty("noaa-scales"));
    }
    let mut days = Vec::new();
    for (key, day) in raw {
        let day_offset: i8 = key.parse().map_err(|_| ParseError::Schema {
            product: "noaa-scales",
            detail: format!("unexpected key {key:?}"),
        })?;
        let time = match (&day.date_stamp, &day.time_stamp) {
            (Some(d), Some(t)) => Some(parse_swpc_time(&format!("{d} {t}"))?),
            _ => None,
        };
        let g = day.g.as_ref();
        let r = day.r.as_ref();
        let s = day.s.as_ref();
        days.push(ScaleDay {
            day_offset,
            time,
            g: DomainStatus {
                domain: ScaleDomain::G,
                scale: g.and_then(|x| level(&x.scale, ScaleDomain::G)),
                text: g.and_then(|x| x.text.clone()),
                probabilities: Vec::new(),
            },
            r: DomainStatus {
                domain: ScaleDomain::R,
                scale: r.and_then(|x| level(&x.scale, ScaleDomain::R)),
                text: r.and_then(|x| x.text.clone()),
                probabilities: r
                    .map(|x| {
                        [
                            percent(&x.minor_prob, "R1–R2"),
                            percent(&x.major_prob, "R3 or greater"),
                        ]
                        .into_iter()
                        .flatten()
                        .collect()
                    })
                    .unwrap_or_default(),
            },
            s: DomainStatus {
                domain: ScaleDomain::S,
                scale: s.and_then(|x| level(&x.scale, ScaleDomain::S)),
                text: s.and_then(|x| x.text.clone()),
                probabilities: s
                    .map(|x| percent(&x.prob, "S1 or greater").into_iter().collect())
                    .unwrap_or_default(),
            },
        });
    }
    days.sort_by_key(|d| d.day_offset);
    Ok(days)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCALES: &str = include_str!("../../../../fixtures/captured/noaa_scales.json");

    #[test]
    fn fixture_yields_current_status_plus_forecast_days() {
        let days = parse(SCALES).unwrap();
        assert!(days.len() >= 2);
        assert!(
            days.iter().any(|d| d.day_offset < 0),
            "SWPC also publishes the previous day"
        );
        let today = days
            .iter()
            .find(|d| d.day_offset == 0)
            .expect("day 0 is current status");
        assert!(
            today.time.is_some(),
            "current status carries a provider timestamp"
        );
    }

    #[test]
    fn the_three_domains_stay_separate() {
        let days = parse(SCALES).unwrap();
        let today = days.iter().find(|d| d.day_offset == 0).unwrap();
        assert_eq!(today.g.domain, ScaleDomain::G);
        assert_eq!(today.r.domain, ScaleDomain::R);
        assert_eq!(today.s.domain, ScaleDomain::S);
    }

    #[test]
    fn probabilities_keep_their_provider_definitions() {
        let days = parse(SCALES).unwrap();
        let with_prob = days
            .iter()
            .find(|d| !d.r.probabilities.is_empty())
            .expect("fixture has probabilities");
        for (label, v) in &with_prob.r.probabilities {
            assert!(!label.is_empty());
            assert!((0.0..=100.0).contains(v));
        }
    }

    #[test]
    fn a_missing_level_is_none_not_zero() {
        let payload = r#"{"1":{"DateStamp":"2026-09-07","TimeStamp":"00:00:00",
            "R":{"Scale":null,"Text":null,"MinorProb":"20","MajorProb":"1"},
            "S":{"Scale":null,"Text":null,"Prob":null},
            "G":{"Scale":null,"Text":null}}}"#;
        let days = parse(payload).unwrap();
        assert_eq!(days.len(), 1);
        assert_eq!(
            days[0].g.scale, None,
            "no stated level must not become level 0"
        );
        assert_eq!(
            days[0].r.probabilities.len(),
            2,
            "both published R probabilities are kept"
        );
    }

    #[test]
    fn a_published_level_zero_is_preserved_as_zero() {
        let days = parse(SCALES).unwrap();
        // Day 0 of the fixture publishes explicit "0" levels meaning "none".
        let today = days.iter().find(|d| d.day_offset == 0).unwrap();
        assert!(today.g.scale.is_some());
        assert_eq!(today.g.scale.unwrap().level, 0);
    }
}
