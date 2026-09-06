//! GOES X-ray flux (1–8 Å / 0.5–4 Å) and the instrument-source registry.
//!
//! Source: <https://services.swpc.noaa.gov/json/goes/primary/xrays-1-day.json>,
//! reached from <https://www.swpc.noaa.gov/products/goes-x-ray-flux>.
//! `instrument-sources.json` records which satellite is primary/secondary for
//! each instrument, and that assignment changes over time.

use super::{is_sentinel, parse_swpc_time, ParseError};
use crate::flare::XrayBand;
use crate::model::{Observation, Quality};
use serde::Deserialize;

pub const PRIMARY_XRAYS_1DAY_URL: &str =
    "https://services.swpc.noaa.gov/json/goes/primary/xrays-1-day.json";
pub const INSTRUMENT_SOURCES_URL: &str =
    "https://services.swpc.noaa.gov/json/goes/instrument-sources.json";
/// Documented cadence of the 1-day X-ray product.
pub const CADENCE_SECONDS: i64 = 60;

#[derive(Debug, Deserialize)]
struct XrayRow {
    time_tag: String,
    satellite: i64,
    flux: Option<f64>,
    energy: String,
    /// Present on the science-grade product; `true` means the sample is
    /// contaminated by electrons and must be treated as suspect.
    #[serde(default)]
    electron_contaminaton: Option<bool>,
}

/// One passband of one satellite.
#[derive(Debug, Clone, PartialEq)]
pub struct XrayChannel {
    pub satellite: String,
    pub band: XrayBand,
    /// Provider's verbatim energy label, shown in the UI.
    pub energy_label: String,
    pub flux_w_m2: Vec<Observation>,
}

/// Parse into one channel per (satellite, band). Rows whose `energy` string is
/// not a documented passband are rejected rather than guessed into a band.
pub fn parse_xrays(payload: &str) -> Result<Vec<XrayChannel>, ParseError> {
    let rows: Vec<XrayRow> = serde_json::from_str(payload)?;
    if rows.is_empty() {
        return Err(ParseError::Empty("goes xrays"));
    }
    let mut channels: Vec<XrayChannel> = Vec::new();
    for row in rows {
        let band =
            XrayBand::from_provider_energy(&row.energy).ok_or_else(|| ParseError::Schema {
                product: "goes xrays",
                detail: format!("unknown passband {:?}", row.energy),
            })?;
        let time = parse_swpc_time(&row.time_tag)?;
        let satellite = format!("GOES-{}", row.satellite);
        let idx = match channels
            .iter()
            .position(|c| c.satellite == satellite && c.band == band)
        {
            Some(i) => i,
            None => {
                channels.push(XrayChannel {
                    satellite: satellite.clone(),
                    band,
                    energy_label: row.energy.clone(),
                    flux_w_m2: Vec::new(),
                });
                channels.len() - 1
            }
        };
        let contaminated = row.electron_contaminaton.unwrap_or(false);
        let obs = match row.flux {
            // Zero and negative flux cannot be shown on a log axis and are not
            // physically meaningful here; they are recorded as missing so the
            // UI can state how many samples were excluded (spec §7).
            Some(v) if !is_sentinel(v) && v > 0.0 => Observation::instant(
                time,
                Some(v),
                if contaminated {
                    Quality::Suspect
                } else {
                    Quality::Good
                },
            ),
            _ => Observation::instant(time, None, Quality::Missing),
        }
        .with_instrument(&satellite);
        channels[idx].flux_w_m2.push(obs);
    }
    Ok(channels)
}

pub fn long_band(channels: &[XrayChannel]) -> Option<&XrayChannel> {
    channels.iter().find(|c| c.band == XrayBand::Long)
}

#[derive(Debug, Deserialize)]
struct InstrumentSourceRow {
    time_tag: String,
    xrays: SourceAssignment,
}

#[derive(Debug, Deserialize)]
struct SourceAssignment {
    primary: i64,
    secondary: i64,
}

/// Which satellite was primary for X-rays, as of the newest entry.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct XraySourceAssignment {
    pub effective_from: chrono::DateTime<chrono::Utc>,
    pub primary: String,
    pub secondary: String,
}

pub fn parse_instrument_sources(payload: &str) -> Result<XraySourceAssignment, ParseError> {
    let rows: Vec<InstrumentSourceRow> = serde_json::from_str(payload)?;
    let newest = rows
        .iter()
        .map(|r| parse_swpc_time(&r.time_tag).map(|t| (t, r)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .max_by_key(|(t, _)| *t)
        .ok_or(ParseError::Empty("goes instrument-sources"))?;
    Ok(XraySourceAssignment {
        effective_from: newest.0,
        primary: format!("GOES-{}", newest.1.xrays.primary),
        secondary: format!("GOES-{}", newest.1.xrays.secondary),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flare::classify;

    const XRAYS: &str = include_str!("../../../../fixtures/captured/goes_primary_xrays_1day.json");
    const SOURCES: &str =
        include_str!("../../../../fixtures/captured/goes_instrument_sources.json");

    #[test]
    fn fixture_splits_into_both_documented_passbands() {
        let ch = parse_xrays(XRAYS).unwrap();
        assert_eq!(
            ch.len(),
            2,
            "one channel per passband for the primary satellite"
        );
        assert!(ch.iter().any(|c| c.band == XrayBand::Long));
        assert!(ch.iter().any(|c| c.band == XrayBand::Short));
    }

    #[test]
    fn satellite_identity_is_carried_on_every_sample() {
        let ch = parse_xrays(XRAYS).unwrap();
        let long = long_band(&ch).unwrap();
        assert!(long.satellite.starts_with("GOES-"));
        assert!(long
            .flux_w_m2
            .iter()
            .all(|o| o.instrument.as_deref() == Some(long.satellite.as_str())));
    }

    #[test]
    fn real_fluxes_classify_within_the_published_range() {
        let ch = parse_xrays(XRAYS).unwrap();
        let long = long_band(&ch).unwrap();
        let classes: Vec<char> = long
            .flux_w_m2
            .iter()
            .filter_map(|o| o.value)
            .filter_map(|v| classify(v, XrayBand::Long))
            .map(|c| c.letter)
            .collect();
        assert!(!classes.is_empty());
        assert!(classes.iter().all(|c| "ABCMX".contains(*c)));
    }

    #[test]
    fn zero_and_null_flux_are_excluded_rather_than_plotted_at_the_axis_floor() {
        let payload = r#"[
            {"time_tag":"2026-09-06T00:00:00Z","satellite":18,"flux":0.0,"energy":"0.1-0.8nm"},
            {"time_tag":"2026-09-06T00:01:00Z","satellite":18,"flux":null,"energy":"0.1-0.8nm"},
            {"time_tag":"2026-09-06T00:02:00Z","satellite":18,"flux":1e-6,"energy":"0.1-0.8nm"}]"#;
        let ch = parse_xrays(payload).unwrap();
        let long = long_band(&ch).unwrap();
        assert_eq!(long.flux_w_m2[0].quality, Quality::Missing);
        assert_eq!(long.flux_w_m2[1].quality, Quality::Missing);
        assert_eq!(long.flux_w_m2[2].value, Some(1e-6));
    }

    #[test]
    fn electron_contaminated_samples_are_marked_suspect() {
        let payload = r#"[{"time_tag":"2026-09-06T00:00:00Z","satellite":18,"flux":1e-6,
            "energy":"0.1-0.8nm","electron_contaminaton":true}]"#;
        let ch = parse_xrays(payload).unwrap();
        assert_eq!(ch[0].flux_w_m2[0].quality, Quality::Suspect);
    }

    #[test]
    fn an_undocumented_passband_is_rejected() {
        let payload = r#"[{"time_tag":"2026-09-06T00:00:00Z","satellite":18,"flux":1e-6,"energy":"0.2-0.9nm"}]"#;
        assert!(parse_xrays(payload).is_err());
    }

    #[test]
    fn instrument_source_assignment_is_read_from_the_newest_entry() {
        let a = parse_instrument_sources(SOURCES).unwrap();
        assert!(a.primary.starts_with("GOES-"));
        assert_ne!(a.primary, a.secondary);
    }
}
