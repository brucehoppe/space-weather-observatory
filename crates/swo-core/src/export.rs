//! Scientific export: UTF-8 CSV and JSON with companion metadata (spec §12).
//!
//! Exports are taken from a **snapshot**, never from a partially refreshed live
//! view. CSV text fields are protected against spreadsheet formula execution
//! while the JSON export preserves canonical source values untouched.

use crate::model::{Aggregation, Quality, Series};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

/// Schema version of the export format itself.
pub const EXPORT_SCHEMA_VERSION: &str = "swo-export/1";

/// Companion metadata written next to every export.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportMetadata {
    pub schema_version: String,
    pub app_version: String,
    /// Product identities included, in export order.
    pub series: Vec<SeriesMetadata>,
    pub selection_start: DateTime<Utc>,
    pub selection_end: DateTime<Utc>,
    /// Time zone the app was displaying. Values themselves are always UTC.
    pub display_time_zone: String,
    /// Identity of the snapshot the values came from.
    pub snapshot_id: String,
    pub exported_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeriesMetadata {
    pub key: String,
    pub label: String,
    pub unit: String,
    pub frame: Option<String>,
    pub source_url: String,
    pub retrieved_at: DateTime<Utc>,
    pub issued_at: Option<DateTime<Utc>>,
    pub payload_sha256: Option<String>,
    /// `raw` or the documented aggregation method.
    pub data_status: String,
    pub nominal_cadence_seconds: i64,
}

impl SeriesMetadata {
    pub fn of(series: &Series) -> Self {
        let data_status = match &series.aggregation {
            Aggregation::Raw => "raw".to_string(),
            Aggregation::Downsampled {
                method,
                bucket_seconds,
            } => {
                format!("aggregated: {method} (bucket {bucket_seconds}s)")
            }
        };
        Self {
            key: series.id.key(),
            label: series.label.clone(),
            unit: series.unit.clone(),
            frame: series.frame.clone(),
            source_url: series.provenance.source_url.clone(),
            retrieved_at: series.provenance.retrieved_at,
            issued_at: series.provenance.issued_at,
            payload_sha256: series.provenance.payload_sha256.clone(),
            data_status,
            nominal_cadence_seconds: series.nominal_cadence_seconds,
        }
    }
}

/// Escape a CSV field, and neutralise spreadsheet formula execution.
///
/// A leading `=`, `+`, `-`, `@`, tab or CR makes Excel/Sheets evaluate the
/// cell. Prefixing an apostrophe is the standard mitigation; the canonical,
/// unmodified value is preserved in the JSON export.
pub fn csv_field(value: &str) -> String {
    let needs_guard = value
        .chars()
        .next()
        .is_some_and(|c| matches!(c, '=' | '+' | '-' | '@' | '\t' | '\r'));
    let guarded = if needs_guard {
        format!("'{value}")
    } else {
        value.to_string()
    };
    if guarded.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", guarded.replace('"', "\"\""))
    } else {
        guarded
    }
}

fn quality_label(q: Quality) -> &'static str {
    match q {
        Quality::Good => "good",
        Quality::Suspect => "suspect",
        Quality::Missing => "missing",
    }
}

/// One CSV per export, long format so several series with different cadences
/// coexist without inventing aligned rows.
pub fn to_csv(series: &[Series]) -> String {
    let mut out = String::from(
        "series_key,label,unit,frame,time_utc,interval_seconds,value,quality,instrument,data_status,source_url\n",
    );
    for s in series {
        let meta = SeriesMetadata::of(s);
        for o in &s.samples {
            let value = match o.value {
                // Numeric values are written unquoted and unrounded.
                Some(v) => format!("{v}"),
                None => String::new(),
            };
            out.push_str(&format!(
                "{},{},{},{},{},{},{},{},{},{},{}\n",
                csv_field(&meta.key),
                csv_field(&s.label),
                csv_field(&s.unit),
                csv_field(s.frame.as_deref().unwrap_or("")),
                csv_field(&o.time.to_rfc3339_opts(SecondsFormat::Secs, true)),
                o.interval_seconds
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
                value,
                quality_label(o.quality),
                csv_field(o.instrument.as_deref().unwrap_or("")),
                csv_field(&meta.data_status),
                csv_field(&meta.source_url),
            ));
        }
    }
    out
}

/// JSON export: canonical values, no formula guarding, full metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonExport {
    pub metadata: ExportMetadata,
    pub series: Vec<Series>,
}

pub fn to_json(series: Vec<Series>, metadata: ExportMetadata) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&JsonExport { metadata, series })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Observation, Provenance, SeriesId, TimePrecision};

    fn t(min: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(1_757_000_000 + min * 60, 0).unwrap()
    }

    fn sample_series() -> Series {
        Series {
            id: SeriesId::new("noaa-swpc", "rtsw_wind_1m", "proton_speed"),
            label: "Solar-wind speed".into(),
            unit: "km/s".into(),
            frame: None,
            nominal_cadence_seconds: 60,
            aggregation: Aggregation::Raw,
            provenance: Provenance {
                source_url: "https://services.swpc.noaa.gov/json/rtsw/rtsw_wind_1m.json".into(),
                retrieved_at: t(10),
                payload_sha256: Some("abc123".into()),
                issued_at: None,
            },
            samples: vec![
                Observation::instant(t(0), Some(412.5), Quality::Good).with_instrument("SOLAR1"),
                Observation::instant(t(1), None, Quality::Missing).with_instrument("SOLAR1"),
                Observation {
                    time: t(2),
                    interval_seconds: Some(10_800),
                    time_precision: TimePrecision::Interval,
                    value: Some(-3.5),
                    quality: Quality::Suspect,
                    instrument: None,
                },
            ],
        }
    }

    fn metadata() -> ExportMetadata {
        ExportMetadata {
            schema_version: EXPORT_SCHEMA_VERSION.into(),
            app_version: "0.1.0".into(),
            series: vec![SeriesMetadata::of(&sample_series())],
            selection_start: t(0),
            selection_end: t(2),
            display_time_zone: "UTC".into(),
            snapshot_id: "snap-1".into(),
            exported_at: t(20),
        }
    }

    #[test]
    fn csv_preserves_units_timestamps_quality_and_identity() {
        let csv = to_csv(&[sample_series()]);
        let lines: Vec<&str> = csv.lines().collect();
        assert!(lines[0].starts_with("series_key,label,unit,frame,time_utc"));
        assert!(lines[1].contains("noaa-swpc:rtsw_wind_1m:proton_speed"));
        assert!(lines[1].contains("km/s"));
        assert!(lines[1].contains("412.5"));
        assert!(lines[1].contains("good"));
        assert!(lines[1].contains("SOLAR1"));
    }

    #[test]
    fn missing_values_export_as_empty_with_an_explicit_quality_not_as_zero() {
        let csv = to_csv(&[sample_series()]);
        let row = csv.lines().nth(2).unwrap();
        assert!(row.contains(",,missing,"), "row was: {row}");
        assert!(!row.contains(",0,"));
    }

    #[test]
    fn interval_valued_rows_carry_their_interval_length() {
        let csv = to_csv(&[sample_series()]);
        assert!(csv.lines().nth(3).unwrap().contains("10800"));
    }

    #[test]
    fn formula_injection_is_neutralised_in_csv_text_fields() {
        for hostile in ["=cmd|'/c calc'!A1", "+1+1", "-2+3", "@SUM(A1)"] {
            let field = csv_field(hostile);
            assert!(
                field.starts_with('\'') || field.starts_with("\"'"),
                "unguarded: {field}"
            );
        }
    }

    #[test]
    fn negative_numeric_values_are_not_mangled_by_the_formula_guard() {
        // The guard applies to text fields; the numeric column keeps its sign.
        let csv = to_csv(&[sample_series()]);
        let row = csv.lines().nth(3).unwrap();
        assert!(
            row.contains(",-3.5,"),
            "signed values must survive export: {row}"
        );
    }

    #[test]
    fn csv_quoting_handles_commas_and_quotes() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("plain"), "plain");
    }

    #[test]
    fn json_export_keeps_canonical_values_unguarded() {
        let json = to_json(vec![sample_series()], metadata()).unwrap();
        let parsed: JsonExport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.series[0].samples[0].value, Some(412.5));
        assert_eq!(parsed.series[0].samples[2].value, Some(-3.5));
        assert_eq!(parsed.metadata.schema_version, EXPORT_SCHEMA_VERSION);
        assert_eq!(parsed.metadata.snapshot_id, "snap-1");
        assert_eq!(
            parsed.metadata.series[0].payload_sha256.as_deref(),
            Some("abc123")
        );
    }

    #[test]
    fn round_trip_matches_the_source_series_exactly() {
        let original = sample_series();
        let json = to_json(vec![original.clone()], metadata()).unwrap();
        let parsed: JsonExport = serde_json::from_str(&json).unwrap();
        assert_eq!(
            parsed.series[0], original,
            "an export must equal its snapshot"
        );
    }

    #[test]
    fn aggregated_exports_are_labelled_as_aggregated() {
        let mut s = sample_series();
        s.aggregation = Aggregation::Downsampled {
            method: "min-max".into(),
            bucket_seconds: 300,
        };
        let csv = to_csv(&[s.clone()]);
        assert!(csv.contains("aggregated: min-max"));
        assert!(SeriesMetadata::of(&s).data_status.starts_with("aggregated"));
    }
}
