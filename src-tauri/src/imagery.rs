//! Real solar imagery (spec §13A).
//!
//! Source: the Helioviewer API (<https://api.helioviewer.org/>), which serves
//! NASA SDO/AIA imagery together with each frame's **actual acquisition time**.
//! That timestamp is carried through and displayed; the application never
//! presents a current image as historical or vice versa, and never animates
//! different wavelengths as successive moments.

use chrono::{DateTime, Duration, NaiveDateTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::providers::{FetchError, Fetcher};

pub const API_BASE: &str = "https://api.helioviewer.org/v2";
pub const CREDIT: &str =
    "NASA/SDO and the AIA, EVE, and HMI science teams, via the Helioviewer Project";

/// A supported AIA passband. False-colour is stated explicitly: these are
/// extreme-ultraviolet images rendered in a conventional colour table, not
/// photographs of visible light.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Passband {
    Aia193,
    Aia304,
}

impl Passband {
    pub const ALL: [Passband; 2] = [Passband::Aia193, Passband::Aia304];

    /// Helioviewer source id for the SDO/AIA channel.
    pub fn source_id(self) -> u32 {
        match self {
            Passband::Aia193 => 11,
            Passband::Aia304 => 13,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Passband::Aia193 => "SDO/AIA 193 Å",
            Passband::Aia304 => "SDO/AIA 304 Å",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Passband::Aia193 => {
                "Extreme ultraviolet at 193 Å, false colour. Shows the hot corona and coronal holes."
            }
            Passband::Aia304 => {
                "Extreme ultraviolet at 304 Å, false colour. Shows the chromosphere and transition region."
            }
        }
    }

    /// Provider-appropriate refresh: SDO/AIA frames appear at roughly this rate
    /// in the Helioviewer archive.
    pub fn refresh_seconds(self) -> i64 {
        600
    }
}

/// Metadata for one image, as reported by the provider.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SunImage {
    pub passband: Passband,
    pub label: String,
    pub description: String,
    /// The frame's own acquisition time, from the provider.
    pub acquired_at: DateTime<Utc>,
    /// When this application retrieved it.
    pub retrieved_at: DateTime<Utc>,
    pub provider_image_id: String,
    pub credit: String,
    pub source_url: String,
    /// `data:` URI of the rendered image. Kept out of the network layer so the
    /// webview never fetches a remote host itself.
    pub data_uri: String,
    /// False colour is always stated.
    pub false_colour: bool,
}

impl SunImage {
    pub fn age(&self, now: DateTime<Utc>) -> Duration {
        now - self.acquired_at
    }
}

#[derive(Debug, Deserialize)]
struct ClosestImage {
    id: String,
    date: String,
}

fn parse_provider_date(raw: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(raw.trim(), "%Y-%m-%d %H:%M:%S")
        .ok()
        .map(|n| Utc.from_utc_datetime(&n))
}

/// Fetch the frame closest to `at` for one passband.
///
/// `at` is an explicit request time. Callers pass the wall clock for live
/// imagery and a replay instant for historical imagery, so a clock change can
/// never silently relabel an image.
pub async fn fetch_image(
    fetcher: &Fetcher,
    passband: Passband,
    at: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<SunImage, FetchError> {
    let meta_url = format!(
        "{API_BASE}/getClosestImage/?date={}&sourceId={}",
        at.format("%Y-%m-%dT%H:%M:%SZ"),
        passband.source_id()
    );
    let body = fetcher.fetch_text(&meta_url).await?;
    let meta: ClosestImage = serde_json::from_str(&body)
        .map_err(|e| FetchError::Transport(format!("unexpected Helioviewer metadata: {e}")))?;
    let acquired_at = parse_provider_date(&meta.date).ok_or_else(|| {
        FetchError::Transport(format!("unparseable acquisition time {:?}", meta.date))
    })?;

    let image_url = format!(
        "{API_BASE}/downloadImage/?id={}&scale=8&x0=0&y0=0&width=768&height=768&display=true&watermark=true",
        meta.id
    );
    let bytes = fetcher.fetch_bytes(&image_url).await?;
    let mime = if bytes.starts_with(&[0xFF, 0xD8]) {
        "image/jpeg"
    } else {
        "image/png"
    };

    Ok(SunImage {
        passband,
        label: passband.label().to_string(),
        description: passband.description().to_string(),
        acquired_at,
        retrieved_at: now,
        provider_image_id: meta.id,
        credit: CREDIT.to_string(),
        source_url: image_url,
        data_uri: format!("data:{mime};base64,{}", base64(&bytes)),
        false_colour: true,
    })
}

/// Minimal base64 encoder: avoids a dependency for one small use.
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passbands_have_distinct_source_ids_and_labels() {
        let ids: Vec<u32> = Passband::ALL.iter().map(|p| p.source_id()).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        assert!(Passband::Aia193.label().contains("193"));
        assert!(Passband::Aia304.label().contains("304"));
    }

    #[test]
    fn every_passband_states_that_the_colour_is_false_colour() {
        for p in Passband::ALL {
            assert!(p.description().contains("false colour"), "{:?}", p);
        }
    }

    #[test]
    fn provider_acquisition_times_parse_as_utc() {
        let t = parse_provider_date("2026-09-06 17:00:05").unwrap();
        assert_eq!(t.to_rfc3339(), "2026-09-06T17:00:05+00:00");
        assert!(parse_provider_date("not a date").is_none());
    }

    #[test]
    fn base64_matches_known_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64(&[0xFF, 0xD8, 0xFF]), "/9j/");
    }

    #[test]
    fn image_age_is_measured_from_the_acquisition_time_not_the_download() {
        let acquired = Utc::now() - Duration::hours(2);
        let img = SunImage {
            passband: Passband::Aia193,
            label: "x".into(),
            description: "x".into(),
            acquired_at: acquired,
            retrieved_at: Utc::now(),
            provider_image_id: "1".into(),
            credit: CREDIT.into(),
            source_url: "https://api.helioviewer.org/".into(),
            data_uri: String::new(),
            false_colour: true,
        };
        let now = Utc::now();
        assert!(img.age(now) >= Duration::hours(2) - Duration::seconds(5));
    }

    #[test]
    fn imagery_hosts_are_on_the_allow_list() {
        crate::providers::check_host(API_BASE).unwrap();
    }
}
