//! NOAA 3-Day Forecast and 3-Day Geomagnetic Forecast (fixed-format text).
//!
//! Sources, reached from <https://www.spaceweather.gov/products/3-day-forecast>
//! and <https://www.spaceweather.gov/products/3-day-geomagnetic-forecast>:
//! - `https://services.swpc.noaa.gov/text/3-day-forecast.txt`
//! - `https://services.swpc.noaa.gov/text/3-day-geomag-forecast.txt`
//!
//! These are the published outlooks for "tonight and the next three days"
//! (spec §13A). The issuance time and the forecast period are read from the
//! product; horizons the product does not cover are reported as unavailable
//! rather than extrapolated.

use super::ParseError;
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};

pub const THREE_DAY_URL: &str = "https://services.swpc.noaa.gov/text/3-day-forecast.txt";
pub const GEOMAG_URL: &str = "https://services.swpc.noaa.gov/text/3-day-geomag-forecast.txt";
pub const THREE_DAY_PAGE: &str = "https://www.spaceweather.gov/products/3-day-forecast";

/// One forecast Kp value for one 3-hour UT interval of one day.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct KpForecastCell {
    pub interval_start: DateTime<Utc>,
    pub interval_seconds: i64,
    pub kp: f64,
    /// NOAA scale annotation printed alongside the value, e.g. `G1`.
    pub noaa_scale: Option<String>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ThreeDayForecast {
    pub issued_at: DateTime<Utc>,
    /// Days the product actually covers, in order.
    pub covered_days: Vec<NaiveDate>,
    pub kp: Vec<KpForecastCell>,
    /// Provider's own rationale paragraph, when present.
    pub rationale: Option<String>,
    pub source_url: String,
}

impl ThreeDayForecast {
    /// End of the forecast period; anything later is not covered.
    pub fn valid_to(&self) -> Option<DateTime<Utc>> {
        self.kp
            .iter()
            .map(|c| c.interval_start + Duration::seconds(c.interval_seconds))
            .max()
    }

    /// Whether the product covers `t`. Used so unavailable horizons render as
    /// unavailable instead of being extrapolated.
    pub fn covers(&self, t: DateTime<Utc>) -> bool {
        match (
            self.kp.iter().map(|c| c.interval_start).min(),
            self.valid_to(),
        ) {
            (Some(from), Some(to)) => t >= from && t < to,
            _ => false,
        }
    }
}

fn parse_issue_line(line: &str) -> Result<DateTime<Utc>, ParseError> {
    // ":Issued: 2026 Sep 06 1230 UTC"
    let raw = line
        .split_once(':')
        .and_then(|(_, rest)| rest.split_once(':'))
        .map(|(_, v)| v.trim())
        .ok_or_else(|| ParseError::Schema {
            product: "3-day forecast",
            detail: line.into(),
        })?;
    super::parse_swpc_time(raw.trim_end_matches(" UTC"))
}

fn month_number(m: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    MONTHS
        .iter()
        .position(|x| x.eq_ignore_ascii_case(m))
        .map(|i| i as u32 + 1)
}

/// Parse the Kp breakdown table shared by both products.
///
/// The table header names the days (`Sep 06  Sep 07  Sep 08`) and each row is a
/// UT interval (`00-03UT`) followed by one value per day, optionally annotated
/// with a scale such as `(G1)`.
fn parse_kp_table(text: &str, issued: DateTime<Utc>) -> (Vec<NaiveDate>, Vec<KpForecastCell>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut days: Vec<NaiveDate> = Vec::new();
    let mut cells: Vec<KpForecastCell> = Vec::new();

    for (i, line) in lines.iter().enumerate() {
        let l = line.trim();
        if !(l.contains("Kp index") && (l.contains("breakdown") || l.contains("forecast"))) {
            continue;
        }
        // Header row: the next non-empty line naming the days.
        let Some(header) = lines[i + 1..].iter().find(|l| !l.trim().is_empty()) else {
            continue;
        };
        let toks: Vec<&str> = header.split_whitespace().collect();
        let mut parsed_days = Vec::new();
        let mut k = 0;
        while k + 1 < toks.len() {
            if let (Some(mo), Ok(d)) = (month_number(toks[k]), toks[k + 1].parse::<u32>()) {
                // The year is not printed in the table; take it from the issue
                // time and roll forward if the table crosses a year boundary.
                let year = issued.date_naive().year_for(mo, issued.date_naive());
                if let Some(date) = NaiveDate::from_ymd_opt(year, mo, d) {
                    parsed_days.push(date);
                }
                k += 2;
            } else {
                k += 1;
            }
        }
        if parsed_days.is_empty() {
            continue;
        }
        days = parsed_days;

        for row in &lines[i + 2..] {
            let r = row.trim();
            if r.is_empty() {
                if !cells.is_empty() {
                    break;
                }
                continue;
            }
            let Some((slot, rest)) = r.split_once("UT") else {
                if !cells.is_empty() {
                    break;
                }
                continue;
            };
            let Some((start_h, _)) = slot.trim().split_once('-') else {
                continue;
            };
            let Ok(hour) = start_h.trim().parse::<u32>() else {
                continue;
            };

            // Values, with optional "(G1)" annotations attached to the previous value.
            let mut day_idx = 0usize;
            let mut pending: Option<usize> = None;
            for tok in rest.split_whitespace() {
                if let Some(scale) = tok.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
                    if let Some(idx) = pending {
                        cells[idx].noaa_scale = Some(scale.to_string());
                    }
                    continue;
                }
                let Ok(kp) = tok.parse::<f64>() else { continue };
                if day_idx >= days.len() {
                    break;
                }
                if let Some(date) = days.get(day_idx) {
                    if let Some(start) = date.and_hms_opt(hour, 0, 0) {
                        cells.push(KpForecastCell {
                            interval_start: Utc.from_utc_datetime(&start),
                            interval_seconds: 3 * 3600,
                            kp,
                            noaa_scale: None,
                        });
                        pending = Some(cells.len() - 1);
                    }
                }
                day_idx += 1;
            }
        }
        break;
    }
    (days, cells)
}

/// Helper trait so the year for a printed "Sep 06" can roll over sensibly.
trait YearFor {
    fn year_for(&self, month: u32, issued: NaiveDate) -> i32;
}

impl YearFor for NaiveDate {
    fn year_for(&self, month: u32, issued: NaiveDate) -> i32 {
        use chrono::Datelike;
        // A forecast issued in December can name January dates.
        if issued.month() == 12 && month == 1 {
            issued.year() + 1
        } else {
            issued.year()
        }
    }
}

fn parse_common(text: &str, url: &str) -> Result<ThreeDayForecast, ParseError> {
    let issue_line = text
        .lines()
        .find(|l| l.trim_start().starts_with(":Issued:"))
        .ok_or(ParseError::Schema {
            product: "3-day forecast",
            detail: "missing :Issued: line".into(),
        })?;
    let issued_at = parse_issue_line(issue_line)?;
    let (covered_days, kp) = parse_kp_table(text, issued_at);
    if kp.is_empty() {
        return Err(ParseError::Schema {
            product: "3-day forecast",
            detail: "no Kp breakdown table found".into(),
        });
    }
    let rationale = text
        .lines()
        .position(|l| l.trim_start().starts_with("Rationale:"))
        .map(|i| {
            text.lines()
                .skip(i)
                .take_while(|l| !l.trim().is_empty())
                .collect::<Vec<_>>()
                .join(" ")
                .trim_start_matches("Rationale:")
                .trim()
                .to_string()
        });
    Ok(ThreeDayForecast {
        issued_at,
        covered_days,
        kp,
        rationale,
        source_url: url.to_string(),
    })
}

pub fn parse_three_day(text: &str) -> Result<ThreeDayForecast, ParseError> {
    parse_common(text, THREE_DAY_URL)
}

pub fn parse_geomag(text: &str) -> Result<ThreeDayForecast, ParseError> {
    parse_common(text, GEOMAG_URL)
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE_DAY: &str = include_str!("../../../../fixtures/captured/3-day-forecast.txt");
    const GEOMAG: &str = include_str!("../../../../fixtures/captured/3-day-geomag-forecast.txt");

    #[test]
    fn three_day_forecast_yields_three_days_of_eight_intervals() {
        let f = parse_three_day(THREE_DAY).unwrap();
        assert_eq!(f.covered_days.len(), 3);
        assert_eq!(f.kp.len(), 24, "3 days x 8 three-hour intervals");
    }

    #[test]
    fn intervals_are_contiguous_three_hour_ut_blocks() {
        let f = parse_three_day(THREE_DAY).unwrap();
        let mut times: Vec<_> = f.kp.iter().map(|c| c.interval_start).collect();
        times.sort();
        times.dedup();
        assert_eq!(times.len(), 24, "no duplicated or missing interval");
        for w in times.windows(2) {
            assert_eq!((w[1] - w[0]).num_seconds(), 10_800);
        }
    }

    #[test]
    fn issuance_time_is_read_from_the_product() {
        let f = parse_three_day(THREE_DAY).unwrap();
        assert_eq!(f.issued_at.to_rfc3339(), "2026-09-06T12:30:00+00:00");
        assert!(f.issued_at < f.kp.iter().map(|c| c.interval_start).max().unwrap());
    }

    #[test]
    fn published_scale_annotations_attach_to_the_right_interval() {
        let f = parse_three_day(THREE_DAY).unwrap();
        let flagged: Vec<_> = f.kp.iter().filter(|c| c.noaa_scale.is_some()).collect();
        assert!(!flagged.is_empty(), "the fixture prints G1 annotations");
        for c in flagged {
            assert!(
                c.kp >= 4.0,
                "a G-labelled interval carries an elevated Kp: {c:?}"
            );
            assert_eq!(c.noaa_scale.as_deref(), Some("G1"));
        }
    }

    #[test]
    fn coverage_is_bounded_and_horizons_beyond_it_are_not_covered() {
        let f = parse_three_day(THREE_DAY).unwrap();
        let to = f.valid_to().unwrap();
        assert!(f.covers(to - Duration::hours(1)));
        assert!(!f.covers(to), "the end of the period is exclusive");
        assert!(
            !f.covers(to + Duration::days(2)),
            "the product is never extended past its period"
        );
    }

    #[test]
    fn the_geomagnetic_product_parses_with_its_own_issue_time() {
        let g = parse_geomag(GEOMAG).unwrap();
        assert_eq!(g.kp.len(), 24);
        let f = parse_three_day(THREE_DAY).unwrap();
        assert_ne!(g.issued_at, f.issued_at, "two products, two issuance times");
    }

    #[test]
    fn rationale_text_is_preserved_verbatim() {
        let f = parse_three_day(THREE_DAY).unwrap();
        assert!(f.rationale.as_ref().unwrap().contains("G1"));
    }

    #[test]
    fn a_product_without_a_table_is_an_error() {
        assert!(
            parse_three_day(":Product: 3-Day Forecast\n:Issued: 2026 Sep 06 1230 UTC\n").is_err()
        );
        assert!(parse_three_day("garbage").is_err());
    }
}
