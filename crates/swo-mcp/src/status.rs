//! `swo-mcp status`: a one-screen plain-text rollup of the latest space
//! weather in the cache. Every line is computed from the dashboard; no model
//! is involved, so it is instant and works offline.

use crate::dashboard::{BulletinReading, Dashboard};
use crate::reports::human_minutes;
use chrono::{DateTime, Utc};

/// Render the rollup. Pure: same dashboard, same text.
pub fn render(d: &Dashboard) -> String {
    let mut out = String::new();

    // Header: clock and data age.
    let clock = DateTime::parse_from_rfc3339(&d.now)
        .map(|t| {
            t.with_timezone(&Utc)
                .format("%Y-%m-%d %H:%M UTC")
                .to_string()
        })
        .unwrap_or_else(|_| d.now.clone());
    let age = d
        .realtime_retrieval_age_minutes
        .or(d.oldest_retrieval_age_minutes)
        .map_or("data age unknown".into(), |m| {
            format!("data {} old", human_minutes(m))
        });
    out.push_str(&format!(
        "Space Weather Observatory — status at {clock} ({age})\n"
    ));
    if let Some(w) = &d.stale_data_warning {
        out.push_str(&format!("\n*** STALE DATA: {w} ***\n"));
    }
    out.push('\n');

    // NOW: the headline numbers on one line.
    let mut now: Vec<String> = Vec::new();
    if let Some(today) = d
        .noaa_scales
        .as_ref()
        .and_then(|s| s.days.iter().find(|x| x.day_offset == 0))
    {
        let levels: Vec<String> = today
            .entries
            .iter()
            .map(|e| {
                format!(
                    "{}{}",
                    e.domain,
                    e.level.map_or("?".into(), |l| l.to_string())
                )
            })
            .collect();
        now.push(levels.join(" / "));
    }
    if let Some(k) = &d.kp_index {
        match &k.latest {
            Some(p) => now.push(format!(
                "Kp {:.2} ({}, {})",
                p.kp,
                p.kind,
                k.latest_level.as_deref().unwrap_or("level unavailable")
            )),
            None => now.push("Kp unavailable".into()),
        }
    }
    if let Some(w) = &d.solar_wind {
        now.push(format!("Wind {} km/s", num(w.speed_km_s.latest, 0)));
        now.push(format!("Bz {} nT", num(w.bz_gsm_nt.latest, 1)));
    }
    if let Some(x) = &d.xray_flux {
        now.push(format!(
            "X-ray {} (24 h peak {})",
            x.latest_class.as_deref().unwrap_or("unavailable"),
            x.peak_24h_class.as_deref().unwrap_or("unavailable")
        ));
    }
    if !now.is_empty() {
        out.push_str(&format!("NOW      {}\n", now.join("   ")));
    }

    // OUTLOOK: NOAA's three-day forecast, highest Kp per day.
    if let Some(f) = &d.three_day_forecast {
        let days: Vec<String> = f
            .days
            .iter()
            .map(|x| match &x.noaa_scale {
                Some(s) => format!("{} Kp {:.2} ({s})", x.date, x.max_kp),
                None => format!("{} Kp {:.2}", x.date, x.max_kp),
            })
            .collect();
        out.push_str(&format!("OUTLOOK  {}\n", days.join(" · ")));
    }

    // AURORA: the model's reach in each hemisphere.
    if let Some(a) = &d.aurora {
        let edge = |e: Option<i32>| {
            e.map_or("nowhere reaches 10 %".into(), |l| {
                format!(
                    "10 % zone to {} deg {}",
                    l.abs(),
                    if l < 0 { "S" } else { "N" }
                )
            })
        };
        out.push_str(&format!(
            "AURORA   north max {:.0} % ({}) · south max {:.0} % ({})\n",
            a.north.max_probability_percent,
            edge(a.north.equatorward_latitude_10_percent),
            a.south.max_probability_percent,
            edge(a.south.equatorward_latitude_10_percent),
        ));
    }

    // The app's reviewed plain-language statements.
    if !d.statements.is_empty() {
        out.push_str("\nWHAT IT MEANS\n");
        for s in &d.statements {
            out.push_str(&format!("  - {}  [{}]\n", s.headline, s.basis));
        }
    }

    // Bulletins: active watches/warnings first, then what was issued today.
    if let Some(b) = &d.bulletins {
        out.push_str(&format!(
            "\nBULLETINS  {} active, {} issued in the last 24 h\n",
            b.active.len(),
            b.issued_last_24h.len()
        ));
        for x in b.active.iter().chain(b.issued_last_24h.iter()) {
            out.push_str(&bulletin_line(x));
        }
    }

    // Gaps are information, not quiet.
    out.push('\n');
    if !d.unavailable.is_empty() {
        out.push_str("UNAVAILABLE  (a gap in information, not evidence of quiet)\n");
        for u in &d.unavailable {
            out.push_str(&format!("  - {u}\n"));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "Data: NOAA SWPC via the app's cache, fingerprint {}. Not an official forecast: https://www.spaceweather.gov\n",
        d.data_fingerprint
    ));
    out
}

fn bulletin_line(b: &BulletinReading) -> String {
    let when = DateTime::parse_from_rfc3339(&b.issued_at)
        .map(|t| t.with_timezone(&Utc).format("%H:%M").to_string())
        .unwrap_or_else(|_| "??:??".into());
    format!(
        "  {when}  {:<6}  {:<2}  {}\n",
        b.status,
        b.scale.as_deref().unwrap_or("--"),
        b.headline
    )
}

fn num(v: Option<f64>, decimals: usize) -> String {
    v.map_or("n/a".into(), |x| format!("{x:.decimals$}"))
}
