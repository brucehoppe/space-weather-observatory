//! Real-Time Solar Wind (RTSW) plasma and magnetometer products.
//!
//! Source: <https://services.swpc.noaa.gov/json/rtsw/rtsw_wind_1m.json> and
//! `rtsw_mag_1m.json`, reached from
//! <https://www.swpc.noaa.gov/products/real-time-solar-wind>.
//!
//! The payload interleaves rows from **several spacecraft** (observed:
//! `SOLAR1`, `ACE`, `IMAP`) and marks the stream currently feeding SWPC's
//! real-time products with `active: true`. The spacecraft is therefore data,
//! not a constant: it is carried through as provenance and never baked into a
//! product name (spec §4).

use super::{is_sentinel, parse_swpc_time, ParseError};
use crate::model::{Observation, Quality};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub const WIND_URL: &str = "https://services.swpc.noaa.gov/json/rtsw/rtsw_wind_1m.json";
pub const MAG_URL: &str = "https://services.swpc.noaa.gov/json/rtsw/rtsw_mag_1m.json";
/// Documented cadence of both RTSW 1-minute products.
pub const CADENCE_SECONDS: i64 = 60;

#[derive(Debug, Deserialize)]
struct WindRow {
    time_tag: String,
    active: bool,
    source: String,
    proton_speed: Option<f64>,
    proton_density: Option<f64>,
    proton_temperature: Option<f64>,
    /// Provider aggregate quality flag; 0 means no objection.
    overall_quality: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct MagRow {
    time_tag: String,
    active: bool,
    source: String,
    bt: Option<f64>,
    bz_gsm: Option<f64>,
    bz_gse: Option<f64>,
    overall_quality: Option<f64>,
}

/// Plasma and field samples for one spacecraft stream.
#[derive(Debug, Clone, PartialEq)]
pub struct RtswWind {
    /// Spacecraft/stream identity exactly as the provider states it.
    pub spacecraft: String,
    /// Whether the provider marks this stream as the active real-time source.
    pub active: bool,
    pub speed_km_s: Vec<Observation>,
    pub density_per_cm3: Vec<Observation>,
    pub temperature_k: Vec<Observation>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RtswMag {
    pub spacecraft: String,
    pub active: bool,
    /// Total field magnitude |B| in nT.
    pub bt_nt: Vec<Observation>,
    /// Bz in **GSM**, the frame relevant to coupling with Earth's field.
    pub bz_gsm_nt: Vec<Observation>,
    /// Bz in GSE, retained so the frame difference is inspectable.
    pub bz_gse_nt: Vec<Observation>,
}

fn quality(flag: Option<f64>) -> Quality {
    match flag {
        Some(0.0) => Quality::Good,
        Some(_) => Quality::Suspect,
        None => Quality::Suspect,
    }
}

fn observation(
    time: DateTime<Utc>,
    value: Option<f64>,
    flag: Option<f64>,
    instrument: &str,
) -> Observation {
    match value {
        Some(v) if !is_sentinel(v) => {
            Observation::instant(time, Some(v), quality(flag)).with_instrument(instrument)
        }
        _ => Observation::instant(time, None, Quality::Missing).with_instrument(instrument),
    }
}

/// Parse the plasma product into one entry per spacecraft stream.
pub fn parse_wind(payload: &str) -> Result<Vec<RtswWind>, ParseError> {
    let rows: Vec<WindRow> = serde_json::from_str(payload)?;
    if rows.is_empty() {
        return Err(ParseError::Empty("rtsw_wind_1m"));
    }
    let mut streams: Vec<RtswWind> = Vec::new();
    for row in rows {
        let time = parse_swpc_time(&row.time_tag)?;
        let idx = match streams.iter().position(|s| s.spacecraft == row.source) {
            Some(i) => i,
            None => {
                streams.push(RtswWind {
                    spacecraft: row.source.clone(),
                    active: row.active,
                    speed_km_s: Vec::new(),
                    density_per_cm3: Vec::new(),
                    temperature_k: Vec::new(),
                });
                streams.len() - 1
            }
        };
        // `active` can flip mid-payload when SWPC switches streams; the latest
        // row wins so the UI reports the current assignment.
        streams[idx].active = row.active;
        let s = &row.source;
        streams[idx]
            .speed_km_s
            .push(observation(time, row.proton_speed, row.overall_quality, s));
        streams[idx].density_per_cm3.push(observation(
            time,
            row.proton_density,
            row.overall_quality,
            s,
        ));
        streams[idx].temperature_k.push(observation(
            time,
            row.proton_temperature,
            row.overall_quality,
            s,
        ));
    }
    Ok(streams)
}

pub fn parse_mag(payload: &str) -> Result<Vec<RtswMag>, ParseError> {
    let rows: Vec<MagRow> = serde_json::from_str(payload)?;
    if rows.is_empty() {
        return Err(ParseError::Empty("rtsw_mag_1m"));
    }
    let mut streams: Vec<RtswMag> = Vec::new();
    for row in rows {
        let time = parse_swpc_time(&row.time_tag)?;
        let idx = match streams.iter().position(|s| s.spacecraft == row.source) {
            Some(i) => i,
            None => {
                streams.push(RtswMag {
                    spacecraft: row.source.clone(),
                    active: row.active,
                    bt_nt: Vec::new(),
                    bz_gsm_nt: Vec::new(),
                    bz_gse_nt: Vec::new(),
                });
                streams.len() - 1
            }
        };
        streams[idx].active = row.active;
        let s = &row.source;
        streams[idx]
            .bt_nt
            .push(observation(time, row.bt, row.overall_quality, s));
        streams[idx]
            .bz_gsm_nt
            .push(observation(time, row.bz_gsm, row.overall_quality, s));
        streams[idx]
            .bz_gse_nt
            .push(observation(time, row.bz_gse, row.overall_quality, s));
    }
    Ok(streams)
}

/// The stream the provider marks active, if any. Returns `None` rather than
/// falling back to an arbitrary spacecraft.
pub fn active_wind(streams: &[RtswWind]) -> Option<&RtswWind> {
    streams.iter().find(|s| s.active)
}

pub fn active_mag(streams: &[RtswMag]) -> Option<&RtswMag> {
    streams.iter().find(|s| s.active)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIND: &str = include_str!("../../../../fixtures/captured/rtsw_wind_1m.json");
    const MAG: &str = include_str!("../../../../fixtures/captured/rtsw_mag_1m.json");

    #[test]
    fn wind_fixture_parses_into_per_spacecraft_streams() {
        let streams = parse_wind(WIND).expect("captured original must parse");
        assert!(
            streams.len() >= 2,
            "the product interleaves multiple spacecraft"
        );
        let names: Vec<&str> = streams.iter().map(|s| s.spacecraft.as_str()).collect();
        assert!(names.contains(&"SOLAR1"), "observed streams: {names:?}");
        let active = active_wind(&streams).expect("one stream is marked active");
        assert!(active.speed_km_s.len() > 1000, "24h at 1-minute cadence");
    }

    #[test]
    fn only_one_stream_is_reported_active() {
        let streams = parse_wind(WIND).unwrap();
        assert_eq!(streams.iter().filter(|s| s.active).count(), 1);
    }

    #[test]
    fn mag_fixture_yields_both_frames_separately() {
        let streams = parse_mag(MAG).unwrap();
        let active = active_mag(&streams).unwrap();
        assert_eq!(active.bz_gsm_nt.len(), active.bz_gse_nt.len());
        // The two frames genuinely differ; conflating them would be a science bug.
        let differs = active
            .bz_gsm_nt
            .iter()
            .zip(&active.bz_gse_nt)
            .any(|(a, b)| a.value != b.value);
        assert!(
            differs,
            "GSM and GSE Bz must not be treated as interchangeable"
        );
    }

    #[test]
    fn signed_bz_retains_negative_values() {
        let streams = parse_mag(MAG).unwrap();
        let active = active_mag(&streams).unwrap();
        assert!(
            active
                .bz_gsm_nt
                .iter()
                .filter_map(|o| o.value)
                .any(|v| v < 0.0),
            "southward Bz must survive parsing as a negative number"
        );
    }

    #[test]
    fn null_measurements_become_missing_not_zero() {
        let payload = r#"[{"time_tag":"2026-09-06T17:57:00","active":true,"source":"SOLAR1",
            "proton_speed":null,"proton_density":5.0,"proton_temperature":null,"overall_quality":0}]"#;
        let s = &parse_wind(payload).unwrap()[0];
        assert_eq!(s.speed_km_s[0].quality, Quality::Missing);
        assert_eq!(s.speed_km_s[0].value, None);
        assert_eq!(s.density_per_cm3[0].value, Some(5.0));
    }

    #[test]
    fn sentinel_values_become_missing() {
        let payload = r#"[{"time_tag":"2026-09-06T17:57:00","active":true,"source":"ACE",
            "proton_speed":-9999,"proton_density":-9999.0,"proton_temperature":1.0,"overall_quality":0}]"#;
        let s = &parse_wind(payload).unwrap()[0];
        assert_eq!(s.speed_km_s[0].quality, Quality::Missing);
        assert_eq!(s.density_per_cm3[0].quality, Quality::Missing);
    }

    #[test]
    fn nonzero_quality_flag_marks_the_sample_suspect_not_good() {
        let payload = r#"[{"time_tag":"2026-09-06T17:57:00","active":true,"source":"ACE",
            "proton_speed":400,"proton_density":5,"proton_temperature":1,"overall_quality":3}]"#;
        let s = &parse_wind(payload).unwrap()[0];
        assert_eq!(s.speed_km_s[0].quality, Quality::Suspect);
        assert_eq!(s.speed_km_s[0].accepted(), Some(400.0));
    }

    #[test]
    fn instrument_provenance_is_attached_to_every_sample() {
        let streams = parse_wind(WIND).unwrap();
        for s in &streams {
            assert!(s
                .speed_km_s
                .iter()
                .all(|o| o.instrument.as_deref() == Some(s.spacecraft.as_str())));
        }
    }

    #[test]
    fn a_changed_schema_is_an_error_not_an_empty_dataset() {
        assert!(parse_wind(r#"{"unexpected":"object"}"#).is_err());
        assert!(
            parse_wind("[]").is_err(),
            "empty must be distinguishable from parsed-and-quiet"
        );
        assert!(parse_wind(r#"[{"time_tag":"nonsense","active":true,"source":"X"}]"#).is_err());
    }
}
