//! Normalized scientific data model.
//!
//! Every value the application shows can be traced back to a provider product,
//! an instrument (when the provider states one), a source timestamp and a
//! quality state. The application's own download time is kept separately from
//! provider metadata and never substituted for it (spec §5).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Stable identity of a measured or forecast series.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SeriesId {
    /// Provider, e.g. `noaa-swpc`.
    pub provider: String,
    /// Provider product identity, e.g. `rtsw_wind_1m`.
    pub product: String,
    /// Measurement name, e.g. `proton_speed`.
    pub measurement: String,
}

impl SeriesId {
    pub fn new(provider: &str, product: &str, measurement: &str) -> Self {
        Self {
            provider: provider.to_string(),
            product: product.to_string(),
            measurement: measurement.to_string(),
        }
    }
    /// Canonical flat key used by the frontend, cache tables and exports.
    pub fn key(&self) -> String {
        format!("{}:{}:{}", self.provider, self.product, self.measurement)
    }
}

/// Quality state of a single sample. Deliberately coarse and explicit: a
/// missing value is never silently promoted to "good".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    /// Provider reported the sample with no quality objection.
    Good,
    /// Provider flagged the sample as degraded/suspect but supplied a value.
    Suspect,
    /// Provider supplied the row but the value is absent (null or sentinel).
    Missing,
}

/// How precisely the timestamp is known / what it refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimePrecision {
    /// Instantaneous sample at `time`.
    Instant,
    /// Value applies to the interval `[time, time + interval_seconds)`.
    Interval,
}

/// One normalized observation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    /// Provider timestamp (UTC), never the download time.
    pub time: DateTime<Utc>,
    /// Present for interval-valued products (e.g. Kp 3-hour intervals).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interval_seconds: Option<i64>,
    pub time_precision: TimePrecision,
    /// `None` when the provider supplied no usable value.
    pub value: Option<f64>,
    pub quality: Quality,
    /// Instrument / spacecraft as stated by the provider, when stated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instrument: Option<String>,
}

impl Observation {
    pub fn instant(time: DateTime<Utc>, value: Option<f64>, quality: Quality) -> Self {
        Self {
            time,
            interval_seconds: None,
            time_precision: TimePrecision::Instant,
            value,
            quality,
            instrument: None,
        }
    }

    pub fn with_instrument(mut self, instrument: impl Into<String>) -> Self {
        self.instrument = Some(instrument.into());
        self
    }

    /// A value usable for quantitative display and alert evaluation.
    pub fn accepted(&self) -> Option<f64> {
        match self.quality {
            Quality::Good | Quality::Suspect => self.value,
            Quality::Missing => None,
        }
    }
}

/// Where a series came from and when it was retrieved.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    pub source_url: String,
    /// When *this application* fetched it. Distinct from provider timestamps.
    pub retrieved_at: DateTime<Utc>,
    /// SHA-256 of the exact bytes parsed, for reproducible exports.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_sha256: Option<String>,
    /// Provider-declared issue time, where the product has one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issued_at: Option<DateTime<Utc>>,
}

/// A normalized series: identity + units + provenance + samples.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Series {
    pub id: SeriesId,
    /// Human label, e.g. "Solar-wind speed".
    pub label: String,
    /// Unit symbol exactly as the science requires, e.g. `km/s`, `nT`, `W/m^2`.
    pub unit: String,
    /// Coordinate frame where one applies (e.g. `GSM`), else `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    /// Nominal cadence of the product in seconds, from provider documentation.
    pub nominal_cadence_seconds: i64,
    /// Whether these samples are raw provider values or app-aggregated.
    pub aggregation: Aggregation,
    pub provenance: Provenance,
    /// Sorted ascending by `time`, duplicates resolved.
    pub samples: Vec<Observation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum Aggregation {
    /// Untouched provider values.
    Raw,
    /// Reduced for display; `method` documents how peaks were preserved.
    Downsampled { method: String, bucket_seconds: i64 },
}

impl Series {
    pub fn last_accepted(&self) -> Option<&Observation> {
        self.samples.iter().rev().find(|o| o.accepted().is_some())
    }

    /// Latest provider timestamp present, accepted or not.
    pub fn last_time(&self) -> Option<DateTime<Utc>> {
        self.samples.last().map(|o| o.time)
    }

    /// Age of the newest accepted sample relative to `now`, in seconds.
    pub fn age_seconds(&self, now: DateTime<Utc>) -> Option<i64> {
        self.last_accepted().map(|o| (now - o.time).num_seconds())
    }
}

/// Status of a forecast product at a given evaluation time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForecastStatus {
    /// Issued and inside its validity window.
    Active,
    /// Validity window has passed.
    Expired,
    /// Provider issued a later product that replaces this one.
    Superseded,
    /// Provider explicitly cancelled it.
    Cancelled,
    /// A point-in-time bulletin (ALERT / SUMMARY) that states no validity
    /// window. It records that something was observed, and is never presented
    /// as a condition currently "in effect".
    Issued,
}

/// A forecast / bulletin record. Issuance and validity are separate from any
/// observation timestamps (spec §5).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ForecastRecord {
    pub provider: String,
    /// Provider product identity, e.g. SWPC `product_id` such as `WATA20`.
    pub product_id: String,
    /// Provider serial/sequence number when supplied.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    pub issued_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_to: Option<DateTime<Utc>>,
    /// Verbatim provider text, escaped at render time.
    pub text: String,
    /// Short provider-derived headline (first meaningful line of the bulletin).
    pub headline: String,
    /// NOAA scale domain and level when the product states one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scale: Option<NoaaScale>,
    pub status: ForecastStatus,
    pub source_url: String,
}

/// NOAA G/R/S scale value. The three domains are always kept separate; the
/// application never combines them into a single "storm score" (spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NoaaScale {
    pub domain: ScaleDomain,
    /// 0 means "none / below scale levels" as published by NOAA.
    pub level: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum ScaleDomain {
    /// Geomagnetic storms.
    G,
    /// Radio blackouts.
    R,
    /// Solar radiation storms.
    S,
}

impl ScaleDomain {
    pub fn label(self) -> &'static str {
        match self {
            ScaleDomain::G => "Geomagnetic storm",
            ScaleDomain::R => "Radio blackout",
            ScaleDomain::S => "Solar radiation storm",
        }
    }
}

/// Per-product freshness/health shown in the UI (spec §10).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProductStatus {
    pub product: String,
    pub state: FeedState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_success: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_sample_time: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedState {
    /// Fresh data within the product's expected cadence.
    Ok,
    /// Last-known-good retained, but older than expected.
    Stale,
    /// Last fetch failed; previous data (if any) retained.
    Error,
    /// Never successfully retrieved in this installation.
    Unavailable,
}
