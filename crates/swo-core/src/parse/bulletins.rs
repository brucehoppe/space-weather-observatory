//! SWPC alerts, watches and warnings.
//!
//! Source: <https://services.swpc.noaa.gov/products/alerts.json>, described at
//! <https://www.spaceweather.gov/products/alerts-watches-and-warnings>.
//!
//! Each entry carries a fixed-format text body. This module extracts the
//! structured fields SWPC actually prints and derives a status at a given
//! evaluation time. Rules enforced here (spec §13A):
//!
//! - A warning stops being active when its stated validity passes.
//! - `CANCEL WARNING` and `EXTENDED WARNING` bodies reference the serial
//!   number they act on; that reference is honoured.
//! - `THIS SUPERSEDES ANY/ALL PRIOR WATCHES IN EFFECT` supersedes earlier
//!   watches of the same message code.
//! - ALERT / SUMMARY bodies state no validity window and are recorded as
//!   point-in-time `Issued` records, never as conditions "in effect".
//! - A missing bulletin is never evidence of quiet conditions.

use super::{parse_swpc_time, ParseError};
use crate::model::{ForecastRecord, ForecastStatus, NoaaScale, ScaleDomain};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub const URL: &str = "https://services.swpc.noaa.gov/products/alerts.json";
pub const PAGE_URL: &str = "https://www.spaceweather.gov/products/alerts-watches-and-warnings";

#[derive(Debug, Deserialize)]
struct RawAlert {
    product_id: String,
    issue_datetime: String,
    message: String,
}

/// Structured fields lifted out of a bulletin body.
#[derive(Debug, Clone, PartialEq)]
pub struct Bulletin {
    pub product_id: String,
    /// SWPC "Space Weather Message Code", e.g. `WATA20`.
    pub message_code: Option<String>,
    pub serial: Option<String>,
    pub issued_at: DateTime<Utc>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_to: Option<DateTime<Utc>>,
    pub headline: String,
    pub kind: BulletinKind,
    pub scale: Option<NoaaScale>,
    /// Serial number this bulletin cancels, if it is a cancellation.
    pub cancels_serial: Option<String>,
    /// Serial number this bulletin extends/replaces, if it is an extension.
    pub extends_serial: Option<String>,
    pub supersedes_prior: bool,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BulletinKind {
    Watch,
    Warning,
    Alert,
    Summary,
    Cancellation,
    Other,
}

fn field<'a>(body: &'a str, prefix: &str) -> Option<&'a str> {
    body.lines()
        .find(|l| l.trim_start().starts_with(prefix))
        .map(|l| l.trim_start()[prefix.len()..].trim())
        .filter(|s| !s.is_empty())
}

fn scale_token(token: &str) -> Option<NoaaScale> {
    let mut chars = token.chars();
    let domain = match chars.next()? {
        'G' => ScaleDomain::G,
        'R' => ScaleDomain::R,
        'S' => ScaleDomain::S,
        _ => return None,
    };
    let level: u8 = chars.next()?.to_digit(10)? as u8;
    if level > 5 || chars.next().is_some_and(|c| c.is_ascii_alphanumeric()) {
        return None;
    }
    Some(NoaaScale { domain, level })
}

/// Read the level SWPC itself prints. Two shapes occur, both provider
/// statements: an explicit `NOAA Scale: S1 - Minor` field, and the
/// `Category G1 Predicted` phrasing used in watch/warning headlines. Nothing
/// here derives a level from measurements.
fn parse_scale(body: &str) -> Option<NoaaScale> {
    if let Some(line) = body
        .lines()
        .map(|l| l.trim())
        .find(|l| l.to_ascii_lowercase().starts_with("noaa scale:"))
    {
        if let Some(s) = line
            .split(':')
            .nth(1)
            .and_then(|v| v.split_whitespace().next())
            .and_then(scale_token)
        {
            return Some(s);
        }
    }
    // "WATCH: Geomagnetic Storm Category G1 Predicted"
    let toks: Vec<&str> = body.split_whitespace().collect();
    for w in toks.windows(2) {
        if w[0].eq_ignore_ascii_case("category") {
            if let Some(s) = scale_token(w[1]) {
                return Some(s);
            }
        }
    }
    None
}

fn headline(body: &str) -> String {
    // The first line beginning with a recognised keyword is the human headline.
    const KEYS: [&str; 6] = [
        "WATCH:",
        "WARNING:",
        "ALERT:",
        "SUMMARY:",
        "EXTENDED WARNING:",
        "CANCEL WARNING:",
    ];
    body.lines()
        .map(|l| l.trim())
        .find(|l| KEYS.iter().any(|k| l.starts_with(k)))
        .map(|l| l.to_string())
        .unwrap_or_else(|| {
            body.lines()
                .map(|l| l.trim())
                .find(|l| !l.is_empty() && !l.contains(':'))
                .unwrap_or("Space weather bulletin")
                .to_string()
        })
}

fn kind_of(headline: &str) -> BulletinKind {
    let h = headline.to_ascii_uppercase();
    if h.starts_with("CANCEL") {
        BulletinKind::Cancellation
    } else if h.starts_with("WATCH") {
        BulletinKind::Watch
    } else if h.contains("WARNING") {
        BulletinKind::Warning
    } else if h.starts_with("ALERT") {
        BulletinKind::Alert
    } else if h.starts_with("SUMMARY") {
        BulletinKind::Summary
    } else {
        BulletinKind::Other
    }
}

/// Parse one bulletin body plus its envelope timestamp.
pub fn parse_one(
    product_id: &str,
    issue_datetime: &str,
    message: &str,
) -> Result<Bulletin, ParseError> {
    // Bodies use CRLF; normalize so line prefixes match.
    let body = message.replace("\r\n", "\n").replace('\r', "\n");
    // Prefer the body's own "Issue Time" over the envelope when both exist:
    // the body is the provider's own statement of issuance.
    let issued_at = match field(&body, "Issue Time:") {
        Some(raw) => parse_swpc_time(raw.trim_end_matches(" UTC"))?,
        None => parse_swpc_time(issue_datetime)?,
    };
    let valid_from = field(&body, "Valid From:")
        .map(|s| parse_swpc_time(s.trim_end_matches(" UTC")))
        .transpose()?;
    let valid_to = field(&body, "Now Valid Until:")
        .or_else(|| field(&body, "Valid To:"))
        .or_else(|| field(&body, "Valid Until:"))
        .map(|s| parse_swpc_time(s.trim_end_matches(" UTC")))
        .transpose()?;

    let head = headline(&body);
    Ok(Bulletin {
        product_id: product_id.to_string(),
        message_code: field(&body, "Space Weather Message Code:").map(str::to_string),
        serial: field(&body, "Serial Number:").map(str::to_string),
        issued_at,
        valid_from,
        valid_to,
        kind: kind_of(&head),
        headline: head,
        scale: parse_scale(&body),
        cancels_serial: field(&body, "Cancel Serial Number:").map(str::to_string),
        extends_serial: field(&body, "Extension to Serial Number:").map(str::to_string),
        supersedes_prior: body.to_ascii_uppercase().contains("THIS SUPERSEDES"),
        text: body,
    })
}

pub fn parse(payload: &str) -> Result<Vec<Bulletin>, ParseError> {
    let rows: Vec<RawAlert> = serde_json::from_str(payload)?;
    if rows.is_empty() {
        return Err(ParseError::Empty("alerts"));
    }
    rows.iter()
        .map(|r| parse_one(&r.product_id, &r.issue_datetime, &r.message))
        .collect()
}

/// Decide each bulletin's status at `now`, using the whole set so that
/// cancellations, extensions and supersessions can be honoured.
pub fn resolve_statuses(bulletins: &[Bulletin], now: DateTime<Utc>) -> Vec<ForecastRecord> {
    let mut out = Vec::with_capacity(bulletins.len());
    for b in bulletins {
        let cancelled = b.serial.as_ref().is_some_and(|serial| {
            bulletins.iter().any(|other| {
                other.kind == BulletinKind::Cancellation
                    && other.cancels_serial.as_ref() == Some(serial)
                    && other.message_code_family() == b.message_code_family()
            })
        });
        let extended = b.serial.as_ref().is_some_and(|serial| {
            bulletins.iter().any(|other| {
                other.extends_serial.as_ref() == Some(serial)
                    && other.message_code_family() == b.message_code_family()
            })
        });
        let superseded_by_later_watch = b.kind == BulletinKind::Watch
            && bulletins.iter().any(|other| {
                other.kind == BulletinKind::Watch
                    && other.supersedes_prior
                    && other.message_code_family() == b.message_code_family()
                    && other.issued_at > b.issued_at
            });

        let status = if cancelled {
            ForecastStatus::Cancelled
        } else if extended || superseded_by_later_watch {
            ForecastStatus::Superseded
        } else {
            match (b.valid_from, b.valid_to) {
                (_, Some(to)) if now > to => ForecastStatus::Expired,
                (Some(from), _) if now < from => ForecastStatus::Active, // issued ahead of its window
                (_, Some(_)) => ForecastStatus::Active,
                // No validity window stated: a point-in-time record.
                (_, None) => match b.kind {
                    BulletinKind::Watch => ForecastStatus::Active,
                    _ => ForecastStatus::Issued,
                },
            }
        };

        out.push(ForecastRecord {
            provider: "NOAA SWPC".into(),
            product_id: b
                .message_code
                .clone()
                .unwrap_or_else(|| b.product_id.clone()),
            serial: b.serial.clone(),
            issued_at: b.issued_at,
            valid_from: b.valid_from,
            valid_to: b.valid_to,
            text: b.text.clone(),
            headline: b.headline.clone(),
            scale: b.scale,
            status,
            source_url: PAGE_URL.to_string(),
        });
    }
    out
}

impl Bulletin {
    /// Message codes share a family prefix across alert/warning/summary
    /// variants (e.g. `WARPX1` / `EXTPX1`). Cancellations and extensions apply
    /// within a family, so compare on the trailing product token.
    fn message_code_family(&self) -> String {
        match &self.message_code {
            Some(code) if code.len() > 3 => code[3..].to_string(),
            Some(code) => code.clone(),
            None => self.product_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    const ALERTS: &str = include_str!("../../../../fixtures/captured/alerts.json");

    fn at(y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, mo, d, h, mi, 0).unwrap()
    }

    #[test]
    fn captured_bulletins_all_parse() {
        let b = parse(ALERTS).unwrap();
        assert!(b.len() > 10);
        assert!(b.iter().all(|x| !x.headline.is_empty()));
        assert!(
            b.iter().all(|x| x.serial.is_some()),
            "every SWPC bulletin has a serial"
        );
    }

    #[test]
    fn watch_extracts_code_serial_and_supersede_marker() {
        let b = parse(ALERTS).unwrap();
        let watch = b
            .iter()
            .find(|x| x.kind == BulletinKind::Watch)
            .expect("fixture has a watch");
        assert!(watch.message_code.as_deref().unwrap().starts_with("WAT"));
        assert!(
            watch.supersedes_prior,
            "the fixture watch supersedes prior watches"
        );
        assert_eq!(
            watch.scale,
            Some(NoaaScale {
                domain: ScaleDomain::G,
                level: 1
            })
        );
    }

    #[test]
    fn extended_warning_records_its_validity_window_and_the_serial_it_extends() {
        let b = parse(ALERTS).unwrap();
        let ext = b
            .iter()
            .find(|x| x.extends_serial.is_some())
            .expect("fixture has an extended warning");
        assert!(ext.valid_from.is_some() && ext.valid_to.is_some());
        assert!(ext.valid_to.unwrap() > ext.valid_from.unwrap());
    }

    #[test]
    fn an_expired_warning_never_reads_as_current() {
        let b = parse(ALERTS).unwrap();
        // Evaluate far in the future: nothing with a validity window is active.
        let recs = resolve_statuses(&b, at(2030, 1, 1, 0, 0));
        assert!(
            recs.iter()
                .filter(|r| r.valid_to.is_some())
                .all(|r| r.status != ForecastStatus::Active),
            "a stated validity window must expire"
        );
    }

    #[test]
    fn a_cancelled_warning_is_marked_cancelled_not_active() {
        let b = parse(ALERTS).unwrap();
        let cancel = b
            .iter()
            .find(|x| x.kind == BulletinKind::Cancellation)
            .expect("fixture has a cancellation");
        let target = cancel.cancels_serial.clone().unwrap();
        let recs = resolve_statuses(&b, cancel.issued_at);
        let cancelled = recs
            .iter()
            .find(|r| r.serial.as_ref() == Some(&target) && r.product_id.starts_with("WAR"))
            .expect("the cancelled warning is present in the fixture");
        assert_eq!(cancelled.status, ForecastStatus::Cancelled);
    }

    #[test]
    fn point_in_time_alerts_are_issued_records_not_conditions_in_effect() {
        let b = parse(ALERTS).unwrap();
        let recs = resolve_statuses(&b, at(2026, 9, 6, 18, 0));
        let alert = recs
            .iter()
            .find(|r| r.headline.starts_with("ALERT:"))
            .expect("fixture has alerts");
        assert_eq!(alert.status, ForecastStatus::Issued);
    }

    #[test]
    fn scale_labels_come_only_from_the_bulletin_text() {
        let b = parse_one(
            "TEST",
            "2026-09-06 12:00:00.000",
            "Space Weather Message Code: WATA20\r\nSerial Number: 1\r\nIssue Time: 2026 Sep 06 1200 UTC\r\n\r\nWATCH: Geomagnetic Storm Category G2 Predicted\r\n",
        )
        .unwrap();
        assert_eq!(
            b.scale,
            Some(NoaaScale {
                domain: ScaleDomain::G,
                level: 2
            }),
            "SWPC's own `Category G2` phrasing is a provider statement"
        );

        // A level the application would have to infer is never produced.
        let derived = parse_one(
            "TEST",
            "2026-09-06 12:00:00.000",
            "Serial Number: 1\nIssue Time: 2026 Sep 06 1200 UTC\n\nALERT: Solar wind speed above 700 km/s\n",
        )
        .unwrap();
        assert_eq!(
            derived.scale, None,
            "no level is inferred from a measurement"
        );
    }

    #[test]
    fn a_body_issue_time_wins_over_the_envelope() {
        let b = parse_one(
            "TEST",
            "2026-09-06 23:59:59.000",
            "Space Weather Message Code: WARK04\nSerial Number: 5\nIssue Time: 2026 Sep 06 1200 UTC\n\nWARNING: Geomagnetic K-index of 4 expected\nValid From: 2026 Sep 06 1200 UTC\nNow Valid Until: 2026 Sep 06 1800 UTC\n",
        )
        .unwrap();
        assert_eq!(b.issued_at, at(2026, 9, 6, 12, 0));
        assert_eq!(b.valid_to, Some(at(2026, 9, 6, 18, 0)));
    }

    #[test]
    fn validity_boundaries_are_exact() {
        let b = parse_one(
            "TEST",
            "2026-09-06 12:00:00.000",
            "Serial Number: 5\nIssue Time: 2026 Sep 06 1200 UTC\n\nWARNING: test\nValid From: 2026 Sep 06 1200 UTC\nNow Valid Until: 2026 Sep 06 1800 UTC\n",
        )
        .unwrap();
        let set = vec![b];
        assert_eq!(
            resolve_statuses(&set, at(2026, 9, 6, 18, 0))[0].status,
            ForecastStatus::Active
        );
        assert_eq!(
            resolve_statuses(&set, at(2026, 9, 6, 18, 1))[0].status,
            ForecastStatus::Expired
        );
    }

    #[test]
    fn a_malformed_payload_is_an_error_not_silence() {
        assert!(parse("[]").is_err());
        assert!(parse("not json").is_err());
    }
}
