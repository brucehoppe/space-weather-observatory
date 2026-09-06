//! OVATION aurora model product.
//!
//! Source: <https://services.swpc.noaa.gov/json/ovation_aurora_latest.json>,
//! described at
//! <https://www.spaceweather.gov/products/aurora-30-minute-forecast>.
//!
//! Documented geometry (verified against the captured original): a 1° x 1°
//! grid, `[longitude, latitude, aurora]`, longitude 0..359 **east**, latitude
//! -90..90. The value is the model's **probability of visible aurora at that
//! grid cell in percent** — a model quantity about the cell, not the user's
//! chance of seeing aurora from a given place, which also depends on darkness,
//! cloud and viewing conditions (spec §8).
//!
//! The product's own `Observation Time` and `Forecast Time` are preserved.
//! The lead time between them varies; the "30-minute" name is not treated as a
//! fixed arrival promise.

use super::{parse_swpc_time, ParseError};
use chrono::{DateTime, Utc};
use serde::Deserialize;

pub const URL: &str = "https://services.swpc.noaa.gov/json/ovation_aurora_latest.json";
pub const PAGE_URL: &str = "https://www.spaceweather.gov/products/aurora-30-minute-forecast";

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(rename = "Observation Time")]
    observation_time: String,
    #[serde(rename = "Forecast Time")]
    forecast_time: String,
    #[serde(rename = "Data Format")]
    data_format: String,
    coordinates: Vec<[f64; 3]>,
}

/// A parsed OVATION grid, stored densely for fast lookup and rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct AuroraGrid {
    pub observation_time: DateTime<Utc>,
    pub forecast_time: DateTime<Utc>,
    /// Provider's own description of the tuple order.
    pub data_format: String,
    pub lon_count: usize,
    pub lat_count: usize,
    /// Minimum latitude of the grid (grid is `lat_min + row` degrees).
    pub lat_min: i32,
    /// Row-major `[lat][lon]` values in percent.
    pub values: Vec<f32>,
}

impl AuroraGrid {
    /// Value at integer grid coordinates. Longitude wraps at 360; latitude is
    /// clamped to the pole rows rather than wrapping (wrapping latitude would
    /// silently mirror the hemisphere).
    pub fn value_at(&self, lon_deg: i32, lat_deg: i32) -> Option<f32> {
        let lon = lon_deg.rem_euclid(self.lon_count as i32) as usize;
        let row = lat_deg - self.lat_min;
        if row < 0 || row as usize >= self.lat_count {
            return None;
        }
        self.values
            .get(row as usize * self.lon_count + lon)
            .copied()
    }

    /// Lead time actually stated by the product for this issue.
    pub fn lead_time_minutes(&self) -> i64 {
        (self.forecast_time - self.observation_time).num_minutes()
    }
}

pub fn parse(payload: &str) -> Result<AuroraGrid, ParseError> {
    let raw: Raw = serde_json::from_str(payload)?;
    if raw.coordinates.is_empty() {
        return Err(ParseError::Empty("ovation aurora"));
    }
    if !raw.data_format.contains("Longitude") || !raw.data_format.contains("Latitude") {
        return Err(ParseError::Schema {
            product: "ovation aurora",
            detail: format!("unexpected data format {:?}", raw.data_format),
        });
    }

    let mut lon_max = i32::MIN;
    let mut lat_min = i32::MAX;
    let mut lat_max = i32::MIN;
    for c in &raw.coordinates {
        lon_max = lon_max.max(c[0] as i32);
        lat_min = lat_min.min(c[1] as i32);
        lat_max = lat_max.max(c[1] as i32);
    }
    let lon_count = (lon_max + 1) as usize;
    let lat_count = (lat_max - lat_min + 1) as usize;
    let expected = lon_count * lat_count;
    if raw.coordinates.len() != expected {
        return Err(ParseError::Schema {
            product: "ovation aurora",
            detail: format!(
                "grid is {} cells, expected {expected} for {lon_count}x{lat_count}",
                raw.coordinates.len()
            ),
        });
    }

    let mut values = vec![f32::NAN; expected];
    for c in &raw.coordinates {
        let lon = c[0] as i32;
        let lat = c[1] as i32;
        let row = (lat - lat_min) as usize;
        values[row * lon_count + lon as usize] = c[2] as f32;
    }
    if values.iter().any(|v| v.is_nan()) {
        return Err(ParseError::Schema {
            product: "ovation aurora",
            detail: "grid has unfilled cells".into(),
        });
    }

    Ok(AuroraGrid {
        observation_time: parse_swpc_time(&raw.observation_time)?,
        forecast_time: parse_swpc_time(&raw.forecast_time)?,
        data_format: raw.data_format,
        lon_count,
        lat_count,
        lat_min,
        values,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const OVATION: &str = include_str!("../../../../fixtures/captured/ovation_aurora_latest.json");

    #[test]
    fn captured_grid_has_the_documented_one_degree_geometry() {
        let g = parse(OVATION).unwrap();
        assert_eq!(g.lon_count, 360, "1-degree longitude grid, 0..359 east");
        assert_eq!(
            g.lat_count, 181,
            "1-degree latitude grid, -90..90 inclusive"
        );
        assert_eq!(g.lat_min, -90);
        assert_eq!(g.values.len(), 360 * 181);
    }

    #[test]
    fn both_poles_are_present_and_addressable() {
        let g = parse(OVATION).unwrap();
        assert!(g.value_at(0, -90).is_some(), "south pole row exists");
        assert!(g.value_at(0, 90).is_some(), "north pole row exists");
        assert!(
            g.value_at(0, 91).is_none(),
            "latitude does not wrap past the pole"
        );
    }

    #[test]
    fn longitude_wraps_at_the_date_line_without_a_seam() {
        let g = parse(OVATION).unwrap();
        // 360 must alias to 0, and -1 to 359: an off-by-one here is the classic
        // date-line seam bug.
        assert_eq!(g.value_at(360, 60), g.value_at(0, 60));
        assert_eq!(g.value_at(-1, 60), g.value_at(359, 60));
    }

    #[test]
    fn values_are_percentages_in_range() {
        let g = parse(OVATION).unwrap();
        assert!(g.values.iter().all(|v| (0.0..=100.0).contains(v)));
    }

    #[test]
    fn aurora_appears_at_auroral_latitudes_not_the_equator() {
        let g = parse(OVATION).unwrap();
        let band = |lat_lo: i32, lat_hi: i32| -> f32 {
            let mut m = 0.0f32;
            for lat in lat_lo..=lat_hi {
                for lon in 0..360 {
                    m = m.max(g.value_at(lon, lat).unwrap());
                }
            }
            m
        };
        let equator = band(-20, 20);
        let auroral = band(60, 80).max(band(-80, -60));
        // OVATION carries small non-zero values at low latitudes; the check is
        // that the auroral ovals dominate, which a flipped grid would break.
        assert!(
            auroral > equator * 3.0,
            "grid orientation check: auroral={auroral} equator={equator}"
        );
    }

    #[test]
    fn issue_and_valid_times_are_preserved_separately() {
        let g = parse(OVATION).unwrap();
        assert!(g.forecast_time > g.observation_time);
        let lead = g.lead_time_minutes();
        assert!(
            lead > 0,
            "the stated lead time is read from the product, not assumed to be 30"
        );
    }

    #[test]
    fn an_incomplete_grid_is_rejected() {
        let payload = r#"{"Observation Time":"2026-09-06T17:55:00Z","Forecast Time":"2026-09-06T18:25:00Z",
            "Data Format":"[Longitude, Latitude, Aurora]","coordinates":[[0,-90,1],[5,-90,2]]}"#;
        assert!(parse(payload).is_err());
    }

    #[test]
    fn an_unexpected_tuple_order_is_rejected_rather_than_assumed() {
        let payload = r#"{"Observation Time":"2026-09-06T17:55:00Z","Forecast Time":"2026-09-06T18:25:00Z",
            "Data Format":"[X, Y, Value]","coordinates":[[0,0,1]]}"#;
        assert!(parse(payload).is_err());
    }
}
