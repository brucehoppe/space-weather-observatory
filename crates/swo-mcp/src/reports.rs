//! Saved plain-language reports, and the check that keeps them up to date.
//!
//! Division of labour: the language model writes the *words*; this module
//! writes the *numbers*. Every saved report is framed with a title, a readings
//! table and a provenance footer generated here from the dashboard, so a
//! model that garbles a figure in its prose cannot corrupt the record.
//!
//! Reports live beside the app's cache, never inside it. The cache stays
//! read-only and the desktop app stays its only writer.

use crate::dashboard::{Dashboard, Measure};
use crate::rfc3339;
use chrono::{DateTime, Duration, Utc};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// A report older than this is out of date even if the data has not changed,
/// because its "today"/"tonight" wording and bulletin validity have moved on.
pub const MAX_REPORT_AGE_HOURS: i64 = 6;

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StoredReport {
    pub generated_at: String,
    /// `Dashboard::data_fingerprint` the report was written from.
    pub data_fingerprint: String,
    /// Model that wrote the prose, e.g. `qwen3:8b`.
    pub model: String,
    pub headline: String,
    /// The full framed report.
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Freshness {
    /// True when the report matches the current data and is recent enough.
    pub is_current: bool,
    /// Why it is out of date; empty when current.
    pub reasons: Vec<String>,
    pub report_age_minutes: Option<i64>,
}

/// Compare the saved report against the dashboard as it is now.
pub fn freshness(
    report: Option<&StoredReport>,
    dashboard: &Dashboard,
    now: DateTime<Utc>,
) -> Freshness {
    let Some(report) = report else {
        return Freshness {
            is_current: false,
            reasons: vec!["no report has been written yet".into()],
            report_age_minutes: None,
        };
    };
    let mut reasons = Vec::new();
    if report.data_fingerprint != dashboard.data_fingerprint {
        reasons.push("the cached data has changed since the report was written".into());
    }
    let age = DateTime::parse_from_rfc3339(&report.generated_at)
        .ok()
        .map(|t| now - t.with_timezone(&Utc));
    match age {
        Some(a) if a > Duration::hours(MAX_REPORT_AGE_HOURS) => reasons.push(format!(
            "the report is more than {MAX_REPORT_AGE_HOURS} hours old"
        )),
        None => reasons.push("the report has no readable timestamp".into()),
        _ => {}
    }
    Freshness {
        is_current: reasons.is_empty(),
        reasons,
        report_age_minutes: age.map(|a| a.num_minutes()),
    }
}

// ---------------------------------------------------------------- framing

fn num(v: Option<f64>, decimals: usize) -> String {
    v.map_or("unavailable".into(), |x| format!("{x:.decimals$}"))
}

fn range(m: &Measure, decimals: usize) -> String {
    match (m.window_min, m.window_max) {
        (Some(lo), Some(hi)) => format!("{lo:.decimals$} to {hi:.decimals$}"),
        _ => "unavailable".into(),
    }
}

/// The readings table. Generated from the dashboard, never by a model.
pub fn readings_table(d: &Dashboard) -> String {
    let mut rows: Vec<(String, String, String)> = Vec::new();
    if let Some(w) = &d.solar_wind {
        let win = format!("last {} min: ", w.window_minutes);
        rows.push((
            "Solar-wind speed".into(),
            format!("{} km/s", num(w.speed_km_s.latest, 0)),
            format!(
                "{win}{} km/s; {}",
                range(&w.speed_km_s, 0),
                w.speed_level.as_deref().unwrap_or("level unavailable")
            ),
        ));
        rows.push((
            "Solar-wind density".into(),
            format!("{} /cm3", num(w.density_per_cm3.latest, 1)),
            format!("{win}{}", range(&w.density_per_cm3, 1)),
        ));
        rows.push((
            "Magnetic field Bt".into(),
            format!("{} nT", num(w.bt_nt.latest, 1)),
            format!("{win}{}", range(&w.bt_nt, 1)),
        ));
        rows.push((
            "Magnetic field Bz (GSM)".into(),
            format!("{} nT", num(w.bz_gsm_nt.latest, 1)),
            format!(
                "{win}{}; southward for {} min",
                range(&w.bz_gsm_nt, 1),
                w.bz_southward_minutes
            ),
        ));
    }
    if let Some(x) = &d.xray_flux {
        rows.push((
            "Solar X-ray flux".into(),
            x.latest_class
                .clone()
                .unwrap_or_else(|| "unavailable".into()),
            format!(
                "24 h peak {}",
                x.peak_24h_class.as_deref().unwrap_or("unavailable")
            ),
        ));
    }
    if let Some(k) = &d.kp_index {
        let latest = k.latest.as_ref();
        rows.push((
            "Kp index".into(),
            latest.map_or("unavailable".into(), |p| {
                format!("{:.2} ({})", p.kp, p.kind)
            }),
            format!(
                "{}; 24 h max {}; NOAA forecast max next 24 h {}",
                k.latest_level.as_deref().unwrap_or("level unavailable"),
                num(k.max_last_24h.as_ref().map(|p| p.kp), 2),
                num(k.max_forecast_next_24h.as_ref().map(|p| p.kp), 2),
            ),
        ));
    }
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
        rows.push((
            "NOAA scales now".into(),
            levels.join(" / "),
            "G geomagnetic, R radio blackout, S radiation; 0 = none".into(),
        ));
    }
    if let Some(b) = &d.bulletins {
        rows.push((
            "NOAA bulletins".into(),
            format!("{} active", b.active.len()),
            format!("{} issued in the last 24 h", b.issued_last_24h.len()),
        ));
    }
    if let Some(f) = &d.three_day_forecast {
        let days: Vec<String> = f
            .days
            .iter()
            .map(|x| match &x.noaa_scale {
                Some(s) => format!("{} Kp {:.2} ({s})", x.date, x.max_kp),
                None => format!("{} Kp {:.2}", x.date, x.max_kp),
            })
            .collect();
        rows.push((
            "NOAA 3-day forecast".into(),
            "highest Kp per day".into(),
            days.join("; "),
        ));
    }
    if let Some(a) = &d.aurora {
        let edge = |e: Option<i32>| {
            e.map_or("nowhere reaches 10 %".into(), |l| {
                format!(
                    "10 % zone reaches {} deg {}",
                    l.abs(),
                    if l < 0 { "S" } else { "N" }
                )
            })
        };
        rows.push((
            "Aurora model (north)".into(),
            format!("max {:.0} %", a.north.max_probability_percent),
            edge(a.north.equatorward_latitude_10_percent),
        ));
        rows.push((
            "Aurora model (south)".into(),
            format!("max {:.0} %", a.south.max_probability_percent),
            edge(a.south.equatorward_latitude_10_percent),
        ));
    }
    if let Some(c) = &d.solar_cycle {
        rows.push((
            "Sunspot number".into(),
            format!("{:.0} ({})", c.sunspot_number, c.latest_month),
            match (
                c.predicted_sunspot_number,
                c.predicted_low,
                c.predicted_high,
            ) {
                (Some(p), Some(lo), Some(hi)) => {
                    format!("official prediction {p:.0} (range {lo:.0} to {hi:.0})")
                }
                _ => "no official prediction cached for this month".into(),
            },
        ));
    }

    let mut out = String::from("| Reading | Latest | Context |\n|---|---|---|\n");
    for (a, b, c) in rows {
        out.push_str(&format!("| {a} | {b} | {c} |\n"));
    }
    out
}

/// First non-empty line of the model's prose, without markdown markers, as a headline.
pub fn headline_of(body: &str) -> String {
    body.lines()
        .map(|l| l.replace("**", "").replace("__", ""))
        .map(|l| {
            l.trim()
                .trim_start_matches(['#', '>', '-', ' '])
                .trim()
                .to_string()
        })
        .find(|l| !l.is_empty())
        .unwrap_or_else(|| "Space weather report".into())
        .chars()
        .take(160)
        .collect()
}

/// Wrap a model's prose in the deterministic title, readings table and footer.
pub fn compose(d: &Dashboard, body: &str, model: &str, now: DateTime<Utc>) -> StoredReport {
    let mut md = format!(
        "# Space weather report\n\n_{}_\n\n",
        now.format("%A %-d %B %Y, %H:%M UTC")
    );
    // Stated by the program, ahead of the prose, so it cannot be left out.
    if let Some(warning) = &d.stale_data_warning {
        md.push_str(&format!("> **Old data.** {warning}\n\n"));
    }
    md.push_str(&format!("{}\n\n", body.trim()));
    md.push_str("## Readings at a glance\n\n");
    md.push_str(&readings_table(d));
    if !d.unavailable.is_empty() {
        md.push_str("\nNot available (a gap in information, not a sign of quiet conditions):\n");
        for u in &d.unavailable {
            md.push_str(&format!("- {u}\n"));
        }
    }
    md.push_str("\n---\n");
    md.push_str("Data: NOAA SWPC products cached by Space Weather Observatory");
    match (
        d.realtime_retrieval_age_minutes,
        d.oldest_retrieval_age_minutes,
    ) {
        (Some(live), Some(oldest)) => {
            md.push_str(&format!(
                "; real-time readings retrieved {} before this report",
                human_minutes(live)
            ));
            // Slow products (the monthly solar cycle) are refreshed rarely; say so
            // rather than letting their age read as the age of everything.
            if oldest > live + 180 {
                md.push_str(&format!(
                    " (slower-changing products up to {} before)",
                    human_minutes(oldest)
                ));
            }
        }
        (None, Some(oldest)) => md.push_str(&format!(
            "; retrieved up to {} before this report",
            human_minutes(oldest)
        )),
        _ => {}
    }
    md.push_str(". ");
    md.push_str(&format!(
        "Wording by the local language model `{model}`; the table above is computed directly from the data. \
         This is not an official forecast. Official products: <https://www.spaceweather.gov>. \
         Data fingerprint `{}`.\n",
        d.data_fingerprint
    ));
    StoredReport {
        generated_at: rfc3339(now),
        data_fingerprint: d.data_fingerprint.clone(),
        model: model.to_string(),
        headline: headline_of(body),
        markdown: md,
    }
}

pub fn human_minutes(m: i64) -> String {
    let plural = |n: i64, unit: &str| format!("{n} {unit}{}", if n == 1 { "" } else { "s" });
    match m {
        m if m < 90 => plural(m, "minute"),
        m if m < 48 * 60 => plural(m / 60, "hour"),
        m => plural(m / (24 * 60), "day"),
    }
}

// ---------------------------------------------------------------- storage

/// `latest.md` and `latest.json` hold the current report; every report is also
/// kept under `history/`. Nothing here is ever deleted by this program.
#[derive(Debug, Clone)]
pub struct ReportStore {
    dir: PathBuf,
}

impl ReportStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn latest(&self) -> Result<Option<StoredReport>, String> {
        let path = self.dir.join("latest.json");
        match std::fs::read_to_string(&path) {
            Ok(raw) => serde_json::from_str(&raw)
                .map(Some)
                .map_err(|e| format!("{} is not a saved report: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(format!("cannot read {}: {e}", path.display())),
        }
    }

    /// Save as the latest report and add it to the history. Returns the markdown path.
    pub fn save(&self, report: &StoredReport) -> Result<PathBuf, String> {
        let history = self.dir.join("history");
        std::fs::create_dir_all(&history)
            .map_err(|e| format!("cannot create {}: {e}", history.display()))?;
        let write = |path: &Path, content: &str| {
            std::fs::write(path, content)
                .map_err(|e| format!("cannot write {}: {e}", path.display()))
        };
        let stamp = report.generated_at.replace([':', '-'], "");
        write(
            &history.join(format!("report-{stamp}.md")),
            &report.markdown,
        )?;
        let json = serde_json::to_string_pretty(report).map_err(|e| e.to_string())?;
        write(&self.dir.join("latest.json"), &json)?;
        let md = self.dir.join("latest.md");
        write(&md, &report.markdown)?;
        Ok(md)
    }
}
