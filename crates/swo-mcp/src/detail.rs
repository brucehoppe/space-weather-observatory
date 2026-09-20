//! Per-panel detail tools: Kp history and forecast, X-ray flare activity, and
//! the interpretation rule layer in full. `get_dashboard` gives the headline
//! of each; these give the series behind it.

use crate::dashboard::{KpPoint, Source};
use crate::{rfc3339, snapshot_at, Snapshot};
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use swo_core::flare::{self, XrayBand};
use swo_core::interpret::Statement;
use swo_core::parse::{goes, kp};

fn source(product: &str, s: &Snapshot, now: DateTime<Utc>) -> Source {
    Source {
        product: product.to_string(),
        source_url: s.source_url.clone(),
        retrieved_at: rfc3339(s.retrieved_at),
        retrieved_age_minutes: (now - s.retrieved_at).num_minutes(),
    }
}

// ---------------------------------------------------------------- Kp

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct KpRequest {
    /// Hours of past intervals to return. Default 24, max 168.
    pub hours_back: Option<u32>,
    /// Hours of forecast intervals to return. Default 72, max 72.
    pub hours_ahead: Option<u32>,
    /// Replay: use the snapshot retrieved at or before this RFC 3339 time.
    pub at: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct KpReport {
    /// The evaluation time that separates past from forecast.
    pub now: String,
    /// Definitive values derived from observatories after the fact.
    pub observed: Vec<KpPoint>,
    /// NOAA's estimates for recent intervals, pending definitive values.
    pub estimated: Vec<KpPoint>,
    /// Intervals NOAA labels "estimated" that had not yet begun when the data was
    /// retrieved (NOAA labels the rest of the current UTC day this way). Treat as
    /// an outlook, not as something that was measured.
    pub estimated_not_yet_begun: Vec<KpPoint>,
    /// NOAA forecasts for future intervals. They may not occur.
    pub forecast: Vec<KpPoint>,
    /// Kp 5 = G1, 6 = G2, 7 = G3, 8 = G4, 9 = G5. Below 5 is not a storm.
    pub scale_note: String,
    pub sources: Vec<Source>,
}

pub fn kp_report(
    conn: &Connection,
    req: &KpRequest,
    now: DateTime<Utc>,
    at: Option<DateTime<Utc>>,
) -> Result<KpReport, String> {
    let back = Duration::hours(i64::from(req.hours_back.unwrap_or(24).clamp(1, 168)));
    let ahead = Duration::hours(i64::from(req.hours_ahead.unwrap_or(72).min(72)));
    let combined = snapshot_at(conn, "planetary_k_index_forecast", at)?;
    let estimates = snapshot_at(conn, "planetary_k_index", at)?;
    if combined.is_none() && estimates.is_none() {
        return Err("no Kp product in the cache for that time".into());
    }

    let mut intervals: Vec<kp::KpInterval> = Vec::new();
    let mut sources = Vec::new();
    if let Some(s) = &combined {
        intervals = kp::parse_kp_forecast(&s.payload).map_err(|e| e.to_string())?;
        sources.push(source("planetary_k_index_forecast", s, now));
    }
    if let Some(s) = &estimates {
        // The estimate-only product fills in intervals the combined product lacks.
        for i in kp::parse_kp(&s.payload).map_err(|e| e.to_string())? {
            if !intervals
                .iter()
                .any(|x| x.observation.time == i.observation.time)
            {
                intervals.push(i);
            }
        }
        sources.push(source("planetary_k_index", s, now));
    }
    intervals.sort_by_key(|i| i.observation.time);

    let retrieved = [&combined, &estimates]
        .into_iter()
        .flatten()
        .map(|s| s.retrieved_at)
        .max();
    let reference = retrieved.map_or(now, |r| r.min(now));
    let pick = |kind: kp::KpKind, begun: bool| -> Vec<KpPoint> {
        intervals
            .iter()
            .filter(|i| i.kind == kind)
            .filter(|i| {
                let t = i.observation.time;
                if kind.is_forecast() {
                    t <= now + ahead
                } else if begun {
                    t >= reference - back && t <= reference
                } else {
                    t > reference
                }
            })
            .filter_map(|i| {
                i.observation.accepted().map(|v| KpPoint {
                    interval_start: rfc3339(i.observation.time),
                    kp: v,
                    kind: i.kind.label().to_string(),
                    noaa_scale: i.noaa_scale.clone(),
                })
            })
            .collect()
    };
    Ok(KpReport {
        now: rfc3339(now),
        observed: pick(kp::KpKind::Observed, true),
        estimated: pick(kp::KpKind::Estimated, true),
        estimated_not_yet_begun: pick(kp::KpKind::Estimated, false),
        forecast: pick(kp::KpKind::Predicted, true),
        scale_note:
            "Kp 5 = G1, 6 = G2, 7 = G3, 8 = G4, 9 = G5. Below 5 is not a geomagnetic storm. \
                     noaa_scale is set only where NOAA printed it."
                .into(),
        sources,
    })
}

// ---------------------------------------------------------------- X-ray flares

#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct FlaresRequest {
    /// Smallest class letter to report: A, B, C, M or X. Default C.
    pub min_class: Option<String>,
    /// Replay: use the snapshot retrieved at or before this RFC 3339 time.
    pub at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FlareEvent {
    /// First minute at or above `min_class`.
    pub start: String,
    pub peak_time: String,
    /// Class at the peak, e.g. `M1.3`.
    pub peak_class: String,
    pub peak_flux_w_m2: f64,
    /// Last minute at or above `min_class`; absent when still above it at the end of the data.
    pub end: Option<String>,
    pub minutes_at_or_above_min_class: i64,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct FlaresReport {
    pub satellite: String,
    pub energy_band: String,
    pub window_start: Option<String>,
    pub window_end: Option<String>,
    pub latest_class: Option<String>,
    pub latest_time: Option<String>,
    /// Lowest flux in the window as a class: the quiet-Sun background level.
    pub background_class: Option<String>,
    pub peak_class: Option<String>,
    pub peak_time: Option<String>,
    pub min_class: String,
    /// Periods the flux stayed at or above `min_class`, oldest first.
    pub events: Vec<FlareEvent>,
    /// Samples excluded because the provider supplied no usable value.
    pub missing_samples: usize,
    pub method_note: String,
    pub source: Source,
}

fn class_floor(letter: &str) -> Result<(char, f64), String> {
    match letter.trim().to_ascii_uppercase().as_str() {
        "A" => Ok(('A', 1e-8)),
        "B" => Ok(('B', 1e-7)),
        "" | "C" => Ok(('C', 1e-6)),
        "M" => Ok(('M', 1e-5)),
        "X" => Ok(('X', 1e-4)),
        other => Err(format!(
            "min_class must be one of A, B, C, M, X (got {other:?})"
        )),
    }
}

pub fn flares_report(
    conn: &Connection,
    req: &FlaresRequest,
    now: DateTime<Utc>,
    at: Option<DateTime<Utc>>,
) -> Result<FlaresReport, String> {
    let (letter, floor) = class_floor(req.min_class.as_deref().unwrap_or("C"))?;
    let snap = snapshot_at(conn, "goes_xrays_1day", at)?
        .ok_or("no goes_xrays_1day snapshot in the cache for that time")?;
    let channels = goes::parse_xrays(&snap.payload).map_err(|e| e.to_string())?;
    let primary = snapshot_at(conn, "goes_instrument_sources", at)?
        .and_then(|s| goes::parse_instrument_sources(&s.payload).ok())
        .map(|a| a.primary);
    let channel = channels
        .iter()
        .filter(|c| c.band == XrayBand::Long)
        .find(|c| {
            primary
                .as_ref()
                .is_some_and(|p| c.satellite.contains(p.as_str()))
        })
        .or_else(|| goes::long_band(&channels))
        .ok_or("the product has no long-band (0.1-0.8 nm) channel")?;

    let mut vals: Vec<(DateTime<Utc>, f64)> = channel
        .flux_w_m2
        .iter()
        .filter_map(|o| o.accepted().map(|v| (o.time, v)))
        .filter(|(_, v)| flare::plottable_on_log_axis(*v))
        .collect();
    vals.sort_by_key(|v| v.0);
    let class = |v: f64| flare::classify(v, XrayBand::Long).map(|c| c.format());

    // An event is a run of samples at or above the floor. A gap of more than
    // five minutes ends a run, so missing data never stitches two flares together.
    let mut events: Vec<FlareEvent> = Vec::new();
    let mut run: Vec<(DateTime<Utc>, f64)> = Vec::new();
    let mut close = |run: &mut Vec<(DateTime<Utc>, f64)>, ended: bool| {
        if let (Some(first), Some(last)) = (run.first().copied(), run.last().copied()) {
            let peak = run
                .iter()
                .copied()
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap_or(first);
            events.push(FlareEvent {
                start: rfc3339(first.0),
                peak_time: rfc3339(peak.0),
                peak_class: class(peak.1).unwrap_or_default(),
                peak_flux_w_m2: peak.1,
                end: ended.then(|| rfc3339(last.0)),
                minutes_at_or_above_min_class: (last.0 - first.0).num_minutes() + 1,
            });
        }
        run.clear();
    };
    for &(t, v) in &vals {
        if run.last().is_some_and(|l| t - l.0 > Duration::minutes(5)) {
            close(&mut run, true);
        }
        if v >= floor {
            run.push((t, v));
        } else {
            close(&mut run, true);
        }
    }
    close(&mut run, false);

    let peak = vals.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
    let low = vals.iter().copied().min_by(|a, b| a.1.total_cmp(&b.1));
    Ok(FlaresReport {
        satellite: channel.satellite.clone(),
        energy_band: channel.energy_label.clone(),
        window_start: vals.first().map(|v| rfc3339(v.0)),
        window_end: vals.last().map(|v| rfc3339(v.0)),
        latest_class: vals.last().and_then(|v| class(v.1)),
        latest_time: vals.last().map(|v| rfc3339(v.0)),
        background_class: low.and_then(|v| class(v.1)),
        peak_class: peak.and_then(|v| class(v.1)),
        peak_time: peak.map(|v| rfc3339(v.0)),
        min_class: letter.to_string(),
        events,
        missing_samples: channel.flux_w_m2.len() - vals.len(),
        method_note: "Events are periods where the 1-minute long-band flux stayed at or above min_class, \
                      classified with the app's flux classifier. This is this application's reading of the \
                      measurements, not NOAA's official flare list, and it says nothing about eruptions (CMEs)."
            .into(),
        source: source("goes_xrays_1day", &snap, now),
    })
}

// ---------------------------------------------------------------- interpretation

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InterpretedStatement {
    pub headline: String,
    pub detail: String,
    /// `NOAA SWPC forecast`, `NOAA SWPC observation` or `Interpretation` (this app's reading).
    pub basis: String,
    /// Activity the statement is about: aurora viewing, HF radio, GNSS, satellites and power.
    pub activity: Option<String>,
    /// Geographic qualifier exactly as NOAA expresses it.
    pub region: Option<String>,
    /// Version of the reviewed wording rules that produced this statement.
    pub rule_version: String,
    /// The input the statement was generated from.
    pub source_ref: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct Interpretation {
    pub now: String,
    pub rule_version: String,
    pub statements: Vec<InterpretedStatement>,
    pub note: String,
}

pub fn interpretation(statements: Vec<Statement>, now: DateTime<Utc>) -> Interpretation {
    Interpretation {
        now: rfc3339(now),
        rule_version: swo_core::interpret::RULE_VERSION.into(),
        statements: statements
            .into_iter()
            .map(|s| InterpretedStatement {
                headline: s.headline,
                detail: s.detail,
                basis: s.basis.label().to_string(),
                activity: s.activity.map(|a| a.label().to_string()),
                region: s.region,
                rule_version: s.rule_version,
                source_ref: s.source_ref,
            })
            .collect(),
        note: "Generated by the desktop app's reviewed rule layer, not by a language model. Effects are \
               attached only to the G, R or S domain that owns them; no probabilities or personal \
               outcomes are invented. Quote or reword these; do not add effects they do not state."
            .into(),
    }
}
