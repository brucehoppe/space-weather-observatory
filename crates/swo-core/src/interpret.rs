//! Versioned plain-language interpretation layer (spec §13A).
//!
//! This module turns **sourced** statuses and forecasts into short text. It is
//! a reviewed, versioned template layer — not a model, not an LLM, and not a
//! forecast of its own. Rules it enforces:
//!
//! - Forecast statements are attributed to the forecasting provider; anything
//!   derived from observations is labelled `Interpretation`.
//! - Effects are attached only to the G/R/S domain that owns them, with the
//!   geographic qualifier the source states.
//! - No probabilities are invented. A probability appears only when a source
//!   supplied it, together with the source's event definition and period.
//! - No personal outcome is predicted (phone, flight, power, GPS).
//! - Absence of a bulletin is never rendered as "all quiet".

use crate::model::{NoaaScale, ScaleDomain};
use serde::{Deserialize, Serialize};

/// Version of the wording and rules below. Recorded on every generated
/// statement so a headline can be traced to the rules that produced it.
pub const RULE_VERSION: &str = "interpretation-rules/1";

/// Where a statement's authority comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Basis {
    /// Restates a published forecast product; attributed to the provider.
    ProviderForecast,
    /// Restates a published observed status; attributed to the provider.
    ProviderObservation,
    /// This application's reading of measurements. Labelled as interpretation.
    Interpretation,
}

impl Basis {
    pub fn label(self) -> &'static str {
        match self {
            Basis::ProviderForecast => "NOAA SWPC forecast",
            Basis::ProviderObservation => "NOAA SWPC observation",
            Basis::Interpretation => "Interpretation",
        }
    }
}

/// Activity domains the app is willing to comment on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Activity {
    AuroraViewing,
    HfRadio,
    Gnss,
    SatelliteAndPower,
}

impl Activity {
    pub fn label(self) -> &'static str {
        match self {
            Activity::AuroraViewing => "Aurora viewing",
            Activity::HfRadio => "HF radio",
            Activity::Gnss => "Precision GNSS / navigation",
            Activity::SatelliteAndPower => "Satellite and power systems",
        }
    }
}

/// A generated statement, with everything needed to audit it later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Statement {
    pub headline: String,
    pub detail: String,
    pub basis: Basis,
    /// The activity this statement is about, when it is about one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity: Option<Activity>,
    /// Geographic qualifier exactly as the source expresses it, when it has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub rule_version: String,
    /// Identity of the input this was generated from, for the evidence trail.
    pub source_ref: String,
}

fn statement(
    headline: impl Into<String>,
    detail: impl Into<String>,
    basis: Basis,
    activity: Option<Activity>,
    region: Option<String>,
    source_ref: impl Into<String>,
) -> Statement {
    Statement {
        headline: headline.into(),
        detail: detail.into(),
        basis,
        activity,
        region,
        rule_version: RULE_VERSION.to_string(),
        source_ref: source_ref.into(),
    }
}

/// Effects for a published G level. Wording follows the NOAA scale
/// descriptions at <https://www.spaceweather.gov/noaa-scales-explanation>;
/// each entry keeps NOAA's own geographic qualifier.
pub fn geomagnetic_effects(scale: NoaaScale, source_ref: &str, basis: Basis) -> Vec<Statement> {
    debug_assert_eq!(scale.domain, ScaleDomain::G);
    let g = format!("G{}", scale.level);
    match scale.level {
        0 => vec![statement(
            "No geomagnetic storm level published",
            "NOAA publishes no G-level for this period. That is a statement about geomagnetic storms only; other space-weather domains are reported separately.",
            basis,
            None,
            None,
            source_ref,
        )],
        1 => vec![
            statement(
                format!("{g} (Minor) geomagnetic storm"),
                "Aurora may be visible at high latitudes. NOAA describes the area of impact as primarily poleward of 60 degrees geomagnetic latitude. Whether aurora is actually visible from a given place also depends on darkness, cloud and local conditions.",
                basis,
                Some(Activity::AuroraViewing),
                Some("Primarily poleward of 60° geomagnetic latitude".into()),
                source_ref,
            ),
            statement(
                format!("{g}: weak power-grid fluctuations possible"),
                "NOAA describes weak power-grid fluctuations and minor impact on satellite operations at this level. This is a description of the scale, not a prediction about any particular system.",
                basis,
                Some(Activity::SatelliteAndPower),
                None,
                source_ref,
            ),
        ],
        2 => vec![
            statement(
                format!("{g} (Moderate) geomagnetic storm"),
                "NOAA describes aurora as possible as low as 55 degrees geomagnetic latitude. Local visibility still depends on darkness, cloud and viewing conditions.",
                basis,
                Some(Activity::AuroraViewing),
                Some("Possible as low as 55° geomagnetic latitude".into()),
                source_ref,
            ),
            statement(
                format!("{g}: HF radio fading at higher latitudes"),
                "NOAA describes HF radio propagation fading at higher latitudes at this level.",
                basis,
                Some(Activity::HfRadio),
                Some("Higher latitudes".into()),
                source_ref,
            ),
        ],
        3 => vec![
            statement(
                format!("{g} (Strong) geomagnetic storm"),
                "NOAA describes aurora as possible as low as 50 degrees geomagnetic latitude, with intermittent satellite navigation and low-frequency radio navigation problems.",
                basis,
                Some(Activity::AuroraViewing),
                Some("Possible as low as 50° geomagnetic latitude".into()),
                source_ref,
            ),
            statement(
                format!("{g}: intermittent satellite navigation problems"),
                "NOAA describes intermittent satellite navigation and low-frequency radio navigation problems at this level. Effects on any specific receiver depend on equipment and location.",
                basis,
                Some(Activity::Gnss),
                None,
                source_ref,
            ),
        ],
        4 | 5 => vec![
            statement(
                format!("{g} geomagnetic storm"),
                "NOAA describes aurora as possible at middle and, at the highest levels, low latitudes, with widespread voltage-control problems and possible degradation of satellite navigation for hours.",
                basis,
                Some(Activity::AuroraViewing),
                Some("Middle and, at the highest levels, low latitudes".into()),
                source_ref,
            ),
            statement(
                format!("{g}: significant infrastructure effects described by NOAA"),
                "NOAA describes widespread voltage-control problems, possible transformer damage at the highest level, and satellite navigation degradation. Operators of affected systems follow their own procedures; this application makes no claim about any specific outage.",
                basis,
                Some(Activity::SatelliteAndPower),
                None,
                source_ref,
            ),
        ],
        _ => Vec::new(),
    }
}

/// Effects for a published R (radio blackout) level. Radio-blackout effects
/// belong to the sunlit side of Earth and to HF radio — never to aurora.
pub fn radio_effects(scale: NoaaScale, source_ref: &str, basis: Basis) -> Vec<Statement> {
    debug_assert_eq!(scale.domain, ScaleDomain::R);
    if scale.level == 0 {
        return vec![statement(
            "No radio blackout level published",
            "NOAA publishes no R-level for this period.",
            basis,
            None,
            None,
            source_ref,
        )];
    }
    let r = format!("R{}", scale.level);
    let detail = match scale.level {
        1 => "NOAA describes weak or minor degradation of HF radio communication on the sunlit side of Earth, with occasional loss of radio contact.",
        2 => "NOAA describes limited blackout of HF radio communication on the sunlit side, with loss of radio contact for tens of minutes.",
        3 => "NOAA describes a wide-area blackout of HF radio communication on the sunlit side, with loss of radio contact for about an hour.",
        _ => "NOAA describes an HF radio blackout on most of the sunlit side of Earth, lasting for a number of hours.",
    };
    vec![statement(
        format!("{r} radio blackout"),
        detail,
        basis,
        Some(Activity::HfRadio),
        Some("Sunlit side of Earth".into()),
        source_ref,
    )]
}

/// Effects for a published S (solar radiation storm) level.
pub fn radiation_effects(scale: NoaaScale, source_ref: &str, basis: Basis) -> Vec<Statement> {
    debug_assert_eq!(scale.domain, ScaleDomain::S);
    if scale.level == 0 {
        return vec![statement(
            "No solar radiation storm level published",
            "NOAA publishes no S-level for this period.",
            basis,
            None,
            None,
            source_ref,
        )];
    }
    let s = format!("S{}", scale.level);
    vec![statement(
        format!("{s} solar radiation storm"),
        // Deliberately framed as high-altitude / space systems and polar
        // aviation, never as a surface health alert (spec §13A).
        "NOAA describes effects on satellite operations and on high-frequency radio in polar regions, and radiation exposure considerations for high-altitude polar flights and crewed spaceflight. This is not a health alert for people at ground level.",
        basis,
        Some(Activity::SatelliteAndPower),
        Some("Polar regions and high altitude".into()),
        source_ref,
    )]
}

/// Observation-based commentary on solar-wind speed. Always `Interpretation`,
/// never a forecast, and never a causal claim (spec §13B template wording).
pub fn solar_wind_speed_note(
    speed_km_s: f64,
    bz_gsm_nt: Option<f64>,
    source_ref: &str,
) -> Statement {
    let elevated = speed_km_s >= 500.0;
    let headline = if elevated {
        "Faster solar wind is arriving upstream of Earth"
    } else {
        "Solar-wind speed is within its ordinary range"
    };
    let mut detail = String::from(
        "Its geomagnetic effect also depends on the magnetic field's orientation and persistence. See the official outlook below.",
    );
    if let Some(bz) = bz_gsm_nt {
        // Bz adds context only. It is not an automatic causal link.
        let orientation = if bz < 0.0 { "southward" } else { "northward" };
        detail = format!(
            "The measured Bz is {bz:.1} nT ({orientation}) in the GSM frame at its own observation time. {detail}"
        );
    } else {
        detail = format!("Bz is unavailable or stale at this time. {detail}");
    }
    statement(
        headline,
        detail,
        Basis::Interpretation,
        None,
        None,
        source_ref,
    )
}

/// Wording for a provider forecast of a storm. Always "forecast"/"possible",
/// never "will happen", and never an invented arrival countdown.
pub fn forecast_summary(
    scale: NoaaScale,
    period_label: &str,
    issued_label: &str,
    source_ref: &str,
) -> Statement {
    let level = match scale.domain {
        ScaleDomain::G => format!("G{}", scale.level),
        ScaleDomain::R => format!("R{}", scale.level),
        ScaleDomain::S => format!("S{}", scale.level),
    };
    statement(
        format!("NOAA forecasts {level} conditions are possible for {period_label}"),
        format!(
            "{} forecast issued {issued_label}, covering {period_label}. Forecast conditions may not occur; observed conditions are shown separately with their own timestamps.",
            scale.domain.label()
        ),
        Basis::ProviderForecast,
        None,
        None,
        source_ref,
    )
}

/// Wording used when a product is missing or a fetch failed. Never "quiet".
pub fn unavailable(product: &str) -> Statement {
    statement(
        format!("{product} unavailable"),
        "This product could not be retrieved. That is a gap in information, not evidence of quiet conditions.",
        Basis::Interpretation,
        None,
        None,
        product,
    )
}

/// Wording when fresh observations disagree with a still-valid forecast. Both
/// are shown; the official forecast is never silently modified.
pub fn disagreement(observation_label: &str, forecast_label: &str) -> Statement {
    statement(
        "Observations and the current forecast differ",
        format!(
            "The latest observations show {observation_label}, while the official forecast still in effect states {forecast_label}. Both are shown with their own timestamps; the official product is not altered here."
        ),
        Basis::Interpretation,
        None,
        None,
        "observation-vs-forecast",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn g(level: u8) -> NoaaScale {
        NoaaScale {
            domain: ScaleDomain::G,
            level,
        }
    }

    #[test]
    fn every_statement_records_its_rule_version_and_source() {
        let s = geomagnetic_effects(g(2), "noaa-scales:day0", Basis::ProviderObservation);
        assert!(!s.is_empty());
        for st in s {
            assert_eq!(st.rule_version, RULE_VERSION);
            assert_eq!(st.source_ref, "noaa-scales:day0");
        }
    }

    #[test]
    fn geographic_qualifiers_track_the_published_level() {
        let g1 = &geomagnetic_effects(g(1), "x", Basis::ProviderForecast)[0];
        let g3 = &geomagnetic_effects(g(3), "x", Basis::ProviderForecast)[0];
        assert!(g1.region.as_ref().unwrap().contains("60°"));
        assert!(g3.region.as_ref().unwrap().contains("50°"));
    }

    #[test]
    fn radio_blackout_effects_never_mention_aurora() {
        for level in 1..=5 {
            for st in radio_effects(
                NoaaScale {
                    domain: ScaleDomain::R,
                    level,
                },
                "x",
                Basis::ProviderObservation,
            ) {
                assert_eq!(st.activity, Some(Activity::HfRadio));
                assert!(!st.detail.to_lowercase().contains("aurora"));
            }
        }
    }

    #[test]
    fn radiation_storm_wording_is_not_a_surface_health_alert() {
        let st = &radiation_effects(
            NoaaScale {
                domain: ScaleDomain::S,
                level: 3,
            },
            "x",
            Basis::ProviderObservation,
        )[0];
        assert!(st
            .detail
            .contains("not a health alert for people at ground level"));
    }

    #[test]
    fn level_zero_is_stated_as_no_published_level_not_as_all_clear() {
        let st = &geomagnetic_effects(g(0), "x", Basis::ProviderObservation)[0];
        assert!(st.detail.contains("geomagnetic storms only"));
        assert!(!st.headline.to_lowercase().contains("quiet"));
    }

    #[test]
    fn missing_products_never_read_as_quiet_conditions() {
        let st = unavailable("Official outlook");
        assert!(st.detail.contains("not evidence of quiet conditions"));
    }

    #[test]
    fn observation_commentary_is_labelled_interpretation_and_not_a_forecast() {
        let st = solar_wind_speed_note(620.0, Some(-4.2), "rtsw_wind_1m");
        assert_eq!(st.basis, Basis::Interpretation);
        assert!(st.detail.contains("southward"));
        assert!(st.detail.contains("official outlook"));
        assert!(!st.headline.to_lowercase().contains("will "));
    }

    #[test]
    fn missing_bz_is_stated_rather_than_omitted() {
        let st = solar_wind_speed_note(620.0, None, "rtsw_wind_1m");
        assert!(st.detail.contains("Bz is unavailable or stale"));
    }

    #[test]
    fn forecast_wording_stays_conditional_and_attributed() {
        let st = forecast_summary(
            g(1),
            "08 Sep 2026 (UTC)",
            "06 Sep 2026 12:30 UTC",
            "3-day-forecast",
        );
        assert_eq!(st.basis, Basis::ProviderForecast);
        assert!(st.headline.contains("possible"));
        assert!(st.detail.contains("may not occur"));
    }

    #[test]
    fn no_statement_predicts_a_personal_outcome() {
        let mut all: Vec<Statement> = Vec::new();
        for l in 0..=5 {
            all.extend(geomagnetic_effects(g(l), "x", Basis::ProviderForecast));
            all.extend(radio_effects(
                NoaaScale {
                    domain: ScaleDomain::R,
                    level: l,
                },
                "x",
                Basis::ProviderForecast,
            ));
            all.extend(radiation_effects(
                NoaaScale {
                    domain: ScaleDomain::S,
                    level: l,
                },
                "x",
                Basis::ProviderForecast,
            ));
        }
        all.push(solar_wind_speed_note(700.0, Some(-10.0), "x"));
        let banned = [
            "your phone",
            "your flight",
            "your gps",
            "your power",
            "you will",
            "guaranteed",
        ];
        for st in &all {
            let text = format!("{} {}", st.headline, st.detail).to_lowercase();
            for b in banned {
                assert!(
                    !text.contains(b),
                    "statement makes a personal prediction: {text}"
                );
            }
        }
    }

    #[test]
    fn no_statement_invents_a_probability() {
        for l in 1..=5 {
            for st in geomagnetic_effects(g(l), "x", Basis::ProviderForecast) {
                let text = format!("{} {}", st.headline, st.detail);
                assert!(
                    !text.contains('%'),
                    "probabilities must come from a source: {text}"
                );
            }
        }
    }

    #[test]
    fn disagreement_shows_both_sides_without_editing_the_official_product() {
        let st = disagreement("Kp 5 estimated", "G1 possible");
        assert!(st.detail.contains("Kp 5 estimated") && st.detail.contains("G1 possible"));
        assert!(st.detail.contains("not altered here"));
    }
}
