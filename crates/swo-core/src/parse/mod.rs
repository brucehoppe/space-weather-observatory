//! Parsers for the machine-readable products resolved in `docs/sources.md`.
//!
//! Each parser is written against a captured original in `fixtures/captured/`
//! and is tested against it. Parsers never guess: an unexpected schema is an
//! error, not an empty scientific dataset (spec §4).

pub mod bulletins;
pub mod forecast_text;
pub mod goes;
pub mod kp;
pub mod ovation;
pub mod rtsw;
pub mod scales;
pub mod solar_cycle;

use chrono::{DateTime, NaiveDateTime, TimeZone, Utc};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("payload is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unexpected schema for {product}: {detail}")]
    Schema {
        product: &'static str,
        detail: String,
    },
    #[error("unparseable timestamp {0:?}")]
    Timestamp(String),
    #[error("empty payload for {0}")]
    Empty(&'static str),
}

/// SWPC emits several timestamp shapes. All are UTC; some omit the marker.
/// Nothing here guesses a local time zone.
pub fn parse_swpc_time(raw: &str) -> Result<DateTime<Utc>, ParseError> {
    let s = raw.trim();
    // "2026-09-06T17:57:00Z" / with fractional seconds / with offset
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.with_timezone(&Utc));
    }
    for fmt in [
        "%Y-%m-%dT%H:%M:%S%.f", // 2026-09-06T17:57:00
        "%Y-%m-%d %H:%M:%S%.f", // 2026-09-06 12:12:15.890
        "%Y-%m-%d %H:%M:%S",
        "%Y %b %d %H%M", // 2026 Sep 06 1212  (bulletin bodies)
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(s, fmt) {
            return Ok(Utc.from_utc_datetime(&naive));
        }
    }
    Err(ParseError::Timestamp(raw.to_string()))
}

/// SWPC uses -9999 (and variants) as a "no data" sentinel in several products.
pub fn is_sentinel(v: f64) -> bool {
    !v.is_finite() || v <= -9998.0
}

#[cfg(test)]
mod time_tests {
    use super::*;

    #[test]
    fn all_observed_swpc_timestamp_shapes_parse_as_utc() {
        let cases = [
            "2026-09-06T17:55:00Z",
            "2026-09-06T17:57:00",
            "2026-09-06 12:12:15.890",
            "2026 Sep 06 1212",
        ];
        for c in cases {
            let t = parse_swpc_time(c).unwrap_or_else(|e| panic!("{c}: {e}"));
            assert_eq!(t.timezone(), Utc);
        }
    }

    #[test]
    fn a_naive_timestamp_is_read_as_utc_not_local() {
        let t = parse_swpc_time("2026-09-06T17:57:00").unwrap();
        assert_eq!(t.to_rfc3339(), "2026-09-06T17:57:00+00:00");
    }

    #[test]
    fn unparseable_timestamps_are_errors_not_defaults() {
        assert!(parse_swpc_time("yesterday").is_err());
        assert!(parse_swpc_time("").is_err());
    }

    #[test]
    fn sentinels_are_recognized() {
        assert!(is_sentinel(-9999.0));
        assert!(is_sentinel(f64::NAN));
        assert!(!is_sentinel(0.0));
        assert!(!is_sentinel(346.8));
    }
}
