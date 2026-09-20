//! One condensed reading per dashboard panel, assembled from the newest cached
//! snapshots. This is what a language model is given instead of raw payloads:
//! small enough for a local model's context, and every number is parsed by
//! `swo-core` and carries its own source and timestamp.
//!
//! Rules kept from the desktop app: observations and forecasts stay separate,
//! the G/R/S domains are never merged into one score, a missing product is
//! reported as missing (never as "quiet"), and nothing is extrapolated.

use crate::{rfc3339, snapshot_at, Snapshot};
use chrono::{DateTime, Duration, Utc};
use rusqlite::Connection;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use swo_core::flare::{self, XrayBand};
use swo_core::interpret::{self, Basis, Statement};
use swo_core::model::{ForecastStatus, Observation};
use swo_core::parse::{bulletins, forecast_text, goes, kp, ovation, rtsw, scales, solar_cycle};

/// Where a reading came from and how old the retrieval is.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Source {
    pub product: String,
    pub source_url: String,
    /// When the desktop app retrieved this product.
    pub retrieved_at: String,
    /// Minutes between `retrieved_at` and now. Large values mean stale data.
    pub retrieved_age_minutes: i64,
}

/// Latest value plus the range over a recent window.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Measure {
    pub latest: Option<f64>,
    /// Provider timestamp of `latest` (UTC).
    pub latest_time: Option<String>,
    pub window_min: Option<f64>,
    pub window_max: Option<f64>,
    pub window_mean: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SolarWindReading {
    pub spacecraft: String,
    /// Length of the window the min/max/mean cover.
    pub window_minutes: i64,
    pub speed_km_s: Measure,
    pub density_per_cm3: Measure,
    pub bt_nt: Measure,
    /// Bz in the GSM frame. Negative means southward.
    pub bz_gsm_nt: Measure,
    /// Minutes in the window with southward (negative) Bz.
    pub bz_southward_minutes: usize,
    /// `ordinary` below 500 km/s, `elevated` from 500, `high` from 700 (rule of this app, not a NOAA product).
    pub speed_level: Option<String>,
    pub sources: Vec<Source>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct XrayReading {
    pub satellite: String,
    /// Long band (0.1-0.8 nm) flux in W/m^2, the band flare classes are defined on.
    pub latest_flux_w_m2: Option<f64>,
    pub latest_time: Option<String>,
    /// Flux class of the latest sample, e.g. `C2.4`. Letters A<B<C<M<X, each 10x stronger.
    pub latest_class: Option<String>,
    /// Highest class in the product's 24-hour span, with its time.
    pub peak_24h_class: Option<String>,
    pub peak_24h_time: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KpPoint {
    /// Start of the 3-hour interval (UTC).
    pub interval_start: String,
    pub kp: f64,
    /// `observed`, `NOAA estimate` or `forecast`. Never merged.
    pub kind: String,
    /// G-scale label only when NOAA printed one next to the value.
    pub noaa_scale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct KpReading {
    /// Most recent non-forecast interval that had begun when the data was retrieved.
    pub latest: Option<KpPoint>,
    /// Highest non-forecast Kp in the 24 hours before the retrieval.
    pub max_last_24h: Option<KpPoint>,
    /// Highest forecast Kp in the next 24 hours (NOAA forecast, may not occur).
    pub max_forecast_next_24h: Option<KpPoint>,
    /// `quiet` (<3), `unsettled` (3), `active` (4), `storm` (>=5; NOAA G1 starts at Kp 5).
    pub latest_level: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScaleEntry {
    /// `G` geomagnetic storm, `R` radio blackout, `S` solar radiation storm.
    pub domain: String,
    /// 0-5 as published; absent when NOAA states no level.
    pub level: Option<u8>,
    /// NOAA's own word for the level, e.g. `none`, `minor`.
    pub text: Option<String>,
    /// NOAA-supplied probabilities, with NOAA's labels. Never invented.
    pub probabilities: Vec<Probability>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Probability {
    pub label: String,
    pub percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScaleDayReading {
    /// 0 = current status, 1..3 = NOAA forecast days, -1 = previous day.
    pub day_offset: i8,
    pub time: Option<String>,
    pub entries: Vec<ScaleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ScalesReading {
    pub days: Vec<ScaleDayReading>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BulletinReading {
    pub product_id: String,
    pub headline: String,
    /// `active` (inside its validity window) or `issued` (a point-in-time record, not "in effect").
    pub status: String,
    pub issued_at: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    /// e.g. `G1`, when the bulletin states a scale.
    pub scale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BulletinsReading {
    /// Watches/warnings currently inside their validity window. Open-ended
    /// watches issued more than four days ago are omitted.
    pub active: Vec<BulletinReading>,
    /// Alerts/summaries issued in the last 24 hours (records of what was observed).
    pub issued_last_24h: Vec<BulletinReading>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ForecastDay {
    pub date: String,
    pub max_kp: f64,
    pub noaa_scale: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ThreeDayReading {
    pub issued_at: String,
    pub days: Vec<ForecastDay>,
    /// NOAA's own rationale paragraph, verbatim.
    pub rationale: Option<String>,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct HemisphereAurora {
    /// Highest modelled aurora probability anywhere in the hemisphere, percent.
    pub max_probability_percent: f32,
    /// Latitude closest to the equator where the model reaches at least 10 %
    /// (geographic degrees; absent when nowhere reaches 10 %).
    pub equatorward_latitude_10_percent: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AuroraReading {
    pub observation_time: String,
    pub forecast_time: String,
    pub lead_time_minutes: i64,
    pub north: HemisphereAurora,
    pub south: HemisphereAurora,
    pub source: Source,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SolarCycleReading {
    /// Newest observed month (YYYY-MM).
    pub latest_month: String,
    pub sunspot_number: f64,
    pub smoothed_sunspot_number: Option<f64>,
    pub f10_7: Option<f64>,
    /// Official consensus prediction for that same month, with the panel's stated range.
    pub predicted_sunspot_number: Option<f64>,
    pub predicted_low: Option<f64>,
    pub predicted_high: Option<f64>,
    pub sources: Vec<Source>,
}

/// Plain-language statement from the app's versioned rule layer (`swo_core::interpret`).
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct StatementReading {
    pub headline: String,
    pub detail: String,
    /// `NOAA SWPC forecast`, `NOAA SWPC observation` or `Interpretation`.
    pub basis: String,
    pub region: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Dashboard {
    pub now: String,
    /// Changes whenever any underlying product changes. Reports record the
    /// fingerprint they were written from, so staleness is detectable.
    pub data_fingerprint: String,
    /// Age in minutes of the oldest product retrieval feeding this dashboard.
    pub oldest_retrieval_age_minutes: Option<i64>,
    /// Age in minutes of the real-time (1-minute solar wind) retrieval: how old
    /// "now" is. Monthly products such as the solar cycle are legitimately older.
    pub realtime_retrieval_age_minutes: Option<i64>,
    /// Present when the data is more than 3 hours old. Any report or answer
    /// must pass this warning on to the reader.
    pub stale_data_warning: Option<String>,
    pub solar_wind: Option<SolarWindReading>,
    pub xray_flux: Option<XrayReading>,
    pub kp_index: Option<KpReading>,
    pub noaa_scales: Option<ScalesReading>,
    pub bulletins: Option<BulletinsReading>,
    pub three_day_forecast: Option<ThreeDayReading>,
    pub aurora: Option<AuroraReading>,
    pub solar_cycle: Option<SolarCycleReading>,
    /// The app's own reviewed plain-language statements for the current readings.
    pub statements: Vec<StatementReading>,
    /// Panels with no usable data, and why. A gap in information, not evidence of quiet.
    pub unavailable: Vec<String>,
}

const WIND_WINDOW_MINUTES: i64 = 120;

fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

fn measure(obs: &[Observation], start: DateTime<Utc>) -> Measure {
    let mut vals: Vec<(DateTime<Utc>, f64)> = obs
        .iter()
        .filter(|o| o.time >= start)
        .filter_map(|o| o.accepted().map(|v| (o.time, v)))
        .collect();
    vals.sort_by_key(|v| v.0);
    Measure {
        latest: vals.last().map(|v| v.1),
        latest_time: vals.last().map(|v| rfc3339(v.0)),
        window_min: vals.iter().map(|v| v.1).reduce(f64::min),
        window_max: vals.iter().map(|v| v.1).reduce(f64::max),
        window_mean: (!vals.is_empty())
            .then(|| round2(vals.iter().map(|v| v.1).sum::<f64>() / vals.len() as f64)),
    }
}

fn newest(obs: &[Observation]) -> Option<DateTime<Utc>> {
    obs.iter()
        .filter(|o| o.accepted().is_some())
        .map(|o| o.time)
        .max()
}

struct Builder<'a> {
    conn: &'a Connection,
    now: DateTime<Utc>,
    hasher: Sha256,
    oldest: Option<i64>,
    unavailable: Vec<String>,
}

impl Builder<'_> {
    /// Newest snapshot of `product`; records it in the fingerprint and freshness.
    fn load(&mut self, product: &str) -> Option<Snapshot> {
        match snapshot_at(self.conn, product, None) {
            Ok(Some(s)) => {
                self.hasher.update(product.as_bytes());
                self.hasher.update(s.sha256.as_bytes());
                let age = (self.now - s.retrieved_at).num_minutes();
                self.oldest = Some(self.oldest.map_or(age, |o| o.max(age)));
                Some(s)
            }
            Ok(None) => {
                self.unavailable
                    .push(format!("{product}: not in the cache yet"));
                None
            }
            Err(e) => {
                self.unavailable.push(format!("{product}: {e}"));
                None
            }
        }
    }

    fn source(&self, product: &str, s: &Snapshot) -> Source {
        Source {
            product: product.to_string(),
            source_url: s.source_url.clone(),
            retrieved_at: rfc3339(s.retrieved_at),
            retrieved_age_minutes: (self.now - s.retrieved_at).num_minutes(),
        }
    }

    /// Unwrap a parse result, recording a failure as an unavailable panel.
    fn parsed<T, E: std::fmt::Display>(&mut self, product: &str, r: Result<T, E>) -> Option<T> {
        r.map_err(|e| {
            self.unavailable
                .push(format!("{product}: could not be parsed ({e})"))
        })
        .ok()
    }

    fn solar_wind(&mut self) -> Option<SolarWindReading> {
        let wind_snap = self.load("rtsw_wind_1m");
        let mag_snap = self.load("rtsw_mag_1m");
        let (wind_snap, mag_snap) = (wind_snap?, mag_snap?);
        let winds = self.parsed("rtsw_wind_1m", rtsw::parse_wind(&wind_snap.payload))?;
        let mags = self.parsed("rtsw_mag_1m", rtsw::parse_mag(&mag_snap.payload))?;
        let (Some(wind), Some(mag)) = (rtsw::active_wind(&winds), rtsw::active_mag(&mags)) else {
            self.unavailable
                .push("solar wind: the provider marks no spacecraft stream active".into());
            return None;
        };
        let end = [newest(&wind.speed_km_s), newest(&mag.bz_gsm_nt)]
            .into_iter()
            .flatten()
            .max()?;
        let start = end - Duration::minutes(WIND_WINDOW_MINUTES);
        let speed = measure(&wind.speed_km_s, start);
        let speed_level = speed.latest.map(|v| {
            if v >= 700.0 {
                "high"
            } else if v >= 500.0 {
                "elevated"
            } else {
                "ordinary"
            }
            .to_string()
        });
        Some(SolarWindReading {
            spacecraft: wind.spacecraft.clone(),
            window_minutes: WIND_WINDOW_MINUTES,
            speed_km_s: speed,
            density_per_cm3: measure(&wind.density_per_cm3, start),
            bt_nt: measure(&mag.bt_nt, start),
            bz_gsm_nt: measure(&mag.bz_gsm_nt, start),
            bz_southward_minutes: mag
                .bz_gsm_nt
                .iter()
                .filter(|o| o.time >= start && o.accepted().is_some_and(|v| v < 0.0))
                .count(),
            speed_level,
            sources: vec![
                self.source("rtsw_wind_1m", &wind_snap),
                self.source("rtsw_mag_1m", &mag_snap),
            ],
        })
    }

    fn xray(&mut self) -> Option<XrayReading> {
        let snap = self.load("goes_xrays_1day")?;
        let channels = self.parsed("goes_xrays_1day", goes::parse_xrays(&snap.payload))?;
        // Prefer the satellite NOAA designates primary, when that product is cached.
        let primary = self
            .load("goes_instrument_sources")
            .and_then(|s| goes::parse_instrument_sources(&s.payload).ok())
            .map(|a| a.primary);
        let long: Vec<&goes::XrayChannel> = channels
            .iter()
            .filter(|c| c.band == XrayBand::Long)
            .collect();
        let channel = primary
            .as_ref()
            .and_then(|p| long.iter().find(|c| c.satellite.contains(p.as_str())))
            .or_else(|| long.first())
            .copied()
            .or_else(|| goes::long_band(&channels));
        let Some(channel) = channel else {
            self.unavailable
                .push("goes_xrays_1day: no long-band channel in the product".into());
            return None;
        };
        let vals: Vec<(DateTime<Utc>, f64)> = channel
            .flux_w_m2
            .iter()
            .filter_map(|o| o.accepted().map(|v| (o.time, v)))
            .filter(|(_, v)| flare::plottable_on_log_axis(*v))
            .collect();
        let latest = vals.iter().max_by_key(|v| v.0).copied();
        let peak = vals.iter().copied().max_by(|a, b| a.1.total_cmp(&b.1));
        let class = |v: f64| flare::classify(v, XrayBand::Long).map(|c| c.format());
        Some(XrayReading {
            satellite: channel.satellite.clone(),
            latest_flux_w_m2: latest.map(|v| v.1),
            latest_time: latest.map(|v| rfc3339(v.0)),
            latest_class: latest.and_then(|v| class(v.1)),
            peak_24h_class: peak.and_then(|v| class(v.1)),
            peak_24h_time: peak.map(|v| rfc3339(v.0)),
            source: self.source("goes_xrays_1day", &snap),
        })
    }

    fn kp(&mut self) -> Option<KpReading> {
        // The combined product carries observed, estimated and forecast intervals;
        // fall back to the estimate-only product when it is absent.
        let (product, snap, intervals) = match self.load("planetary_k_index_forecast") {
            Some(s) => {
                let parsed = self.parsed(
                    "planetary_k_index_forecast",
                    kp::parse_kp_forecast(&s.payload),
                );
                ("planetary_k_index_forecast", s, parsed?)
            }
            None => {
                let s = self.load("planetary_k_index")?;
                let parsed = self.parsed("planetary_k_index", kp::parse_kp(&s.payload));
                ("planetary_k_index", s, parsed?)
            }
        };
        let point = |i: &kp::KpInterval| {
            i.observation.accepted().map(|v| KpPoint {
                interval_start: rfc3339(i.observation.time),
                kp: v,
                kind: i.kind.label().to_string(),
                noaa_scale: i.noaa_scale.clone(),
            })
        };
        let now = self.now;
        // NOAA labels the remainder of the current UTC day "estimated" too, so an
        // interval only counts as having happened if it began before the retrieval.
        let reference = now.min(snap.retrieved_at);
        let past = || {
            intervals
                .iter()
                .filter(move |i| !i.kind.is_forecast() && i.observation.time <= reference)
                .filter(|i| i.observation.accepted().is_some())
        };
        let by_kp = |a: &&kp::KpInterval, b: &&kp::KpInterval| {
            a.observation
                .accepted()
                .unwrap_or(0.0)
                .total_cmp(&b.observation.accepted().unwrap_or(0.0))
        };
        let latest = past().max_by_key(|i| i.observation.time).and_then(point);
        let max_last_24h = past()
            .filter(|i| i.observation.time >= reference - Duration::hours(24))
            .max_by(by_kp)
            .and_then(point);
        let max_forecast_next_24h = intervals
            .iter()
            .filter(|i| i.observation.time > reference && i.observation.accepted().is_some())
            .filter(|i| {
                i.observation.time > now - Duration::hours(3)
                    && i.observation.time <= now + Duration::hours(24)
            })
            .max_by(by_kp)
            .and_then(point);
        let latest_level = latest.as_ref().map(|p| {
            if p.kp >= 5.0 {
                "storm"
            } else if p.kp >= 4.0 {
                "active"
            } else if p.kp >= 3.0 {
                "unsettled"
            } else {
                "quiet"
            }
            .to_string()
        });
        Some(KpReading {
            latest,
            max_last_24h,
            max_forecast_next_24h,
            latest_level,
            source: self.source(product, &snap),
        })
    }

    fn scales(&mut self) -> Option<(ScalesReading, Vec<scales::ScaleDay>)> {
        let snap = self.load("noaa_scales")?;
        let days = self.parsed("noaa_scales", scales::parse(&snap.payload))?;
        let entry = |d: &scales::DomainStatus, letter: &str| ScaleEntry {
            domain: letter.to_string(),
            level: d.scale.map(|s| s.level),
            text: d.text.clone(),
            probabilities: d
                .probabilities
                .iter()
                .map(|(label, percent)| Probability {
                    label: label.clone(),
                    percent: *percent,
                })
                .collect(),
        };
        let mut out: Vec<ScaleDayReading> = days
            .iter()
            .map(|d| ScaleDayReading {
                day_offset: d.day_offset,
                time: d.time.map(rfc3339),
                entries: vec![entry(&d.g, "G"), entry(&d.r, "R"), entry(&d.s, "S")],
            })
            .collect();
        out.sort_by_key(|d| d.day_offset);
        Some((
            ScalesReading {
                days: out,
                source: self.source("noaa_scales", &snap),
            },
            days,
        ))
    }

    fn bulletins(&mut self) -> Option<BulletinsReading> {
        let snap = self.load("alerts")?;
        let parsed = self.parsed("alerts", bulletins::parse(&snap.payload))?;
        let reading = |r: &swo_core::model::ForecastRecord, status: &str| BulletinReading {
            product_id: r.product_id.clone(),
            headline: r.headline.clone(),
            status: status.to_string(),
            issued_at: rfc3339(r.issued_at),
            valid_from: r.valid_from.map(rfc3339),
            valid_to: r.valid_to.map(rfc3339),
            scale: r.scale.map(|s| format!("{:?}{}", s.domain, s.level)),
        };
        let mut records = bulletins::resolve_statuses(&parsed, self.now);
        records.sort_by_key(|r| std::cmp::Reverse(r.issued_at));
        // A watch that states no end time stays "active" in the core model. SWPC
        // watches look at most three days ahead, so an open-ended one issued more
        // than four days ago is left out rather than reported as in effect.
        let open_ended_cutoff = self.now - Duration::days(4);
        let active = records
            .iter()
            .filter(|r| r.status == ForecastStatus::Active)
            .filter(|r| r.valid_to.is_some() || r.issued_at >= open_ended_cutoff)
            .map(|r| reading(r, "active"))
            .collect();
        let issued_last_24h = records
            .iter()
            .filter(|r| r.status == ForecastStatus::Issued)
            .filter(|r| r.issued_at >= self.now - Duration::hours(24) && r.issued_at <= self.now)
            .map(|r| reading(r, "issued"))
            .collect();
        Some(BulletinsReading {
            active,
            issued_last_24h,
            source: self.source("alerts", &snap),
        })
    }

    fn three_day(&mut self) -> Option<ThreeDayReading> {
        let snap = self.load("three_day_forecast")?;
        let f = self.parsed(
            "three_day_forecast",
            forecast_text::parse_three_day(&snap.payload),
        )?;
        let days = f
            .covered_days
            .iter()
            .filter_map(|day| {
                f.kp.iter()
                    .filter(|c| c.interval_start.date_naive() == *day)
                    .max_by(|a, b| a.kp.total_cmp(&b.kp))
                    .map(|c| ForecastDay {
                        date: day.to_string(),
                        max_kp: c.kp,
                        noaa_scale: c.noaa_scale.clone(),
                    })
            })
            .collect();
        Some(ThreeDayReading {
            issued_at: rfc3339(f.issued_at),
            days,
            rationale: f.rationale.clone(),
            source: self.source("three_day_forecast", &snap),
        })
    }

    fn aurora(&mut self) -> Option<AuroraReading> {
        let snap = self.load("ovation_aurora_latest")?;
        let grid = self.parsed("ovation_aurora_latest", ovation::parse(&snap.payload))?;
        let lat_max = grid.lat_min + grid.lat_count as i32 - 1;
        let hemisphere = |north: bool| {
            let mut max = 0f32;
            let mut edge: Option<i32> = None;
            for lat in grid.lat_min..=lat_max {
                if (north && lat <= 0) || (!north && lat >= 0) {
                    continue;
                }
                let row_max = (0..grid.lon_count as i32)
                    .filter_map(|lon| grid.value_at(lon, lat))
                    .fold(0f32, f32::max);
                max = max.max(row_max);
                if row_max >= 10.0 && edge.is_none_or(|e| lat.abs() < e.abs()) {
                    edge = Some(lat);
                }
            }
            HemisphereAurora {
                max_probability_percent: max,
                equatorward_latitude_10_percent: edge,
            }
        };
        Some(AuroraReading {
            observation_time: rfc3339(grid.observation_time),
            forecast_time: rfc3339(grid.forecast_time),
            lead_time_minutes: grid.lead_time_minutes(),
            north: hemisphere(true),
            south: hemisphere(false),
            source: self.source("ovation_aurora_latest", &snap),
        })
    }

    fn solar_cycle(&mut self) -> Option<SolarCycleReading> {
        let snap = self.load("solar_cycle_observed")?;
        let observed = self.parsed(
            "solar_cycle_observed",
            solar_cycle::parse_observed(&snap.payload),
        )?;
        let latest = observed.iter().max_by_key(|m| m.month)?;
        // The smoothed value needs months on both sides, so it lags the newest month.
        let smoothed = observed
            .iter()
            .filter(|m| m.smoothed_ssn.is_some())
            .max_by_key(|m| m.month)
            .and_then(|m| m.smoothed_ssn);
        let mut sources = vec![self.source("solar_cycle_observed", &snap)];
        let predicted = self.load("solar_cycle_predicted").and_then(|s| {
            let months = self.parsed(
                "solar_cycle_predicted",
                solar_cycle::parse_predicted(&s.payload),
            )?;
            sources.push(self.source("solar_cycle_predicted", &s));
            months.into_iter().find(|m| m.month == latest.month)
        });
        Some(SolarCycleReading {
            latest_month: latest.month.format("%Y-%m").to_string(),
            sunspot_number: latest.ssn,
            smoothed_sunspot_number: smoothed,
            f10_7: latest.f10_7,
            predicted_sunspot_number: predicted.as_ref().map(|p| p.predicted_ssn),
            predicted_low: predicted.as_ref().map(|p| p.low_ssn),
            predicted_high: predicted.as_ref().map(|p| p.high_ssn),
            sources,
        })
    }
}

/// Assemble every dashboard reading from the newest cached snapshots.
pub fn build(conn: &Connection, now: DateTime<Utc>) -> Dashboard {
    build_with_statements(conn, now).0
}

/// The dashboard plus the rule layer's statements in full, with their audit
/// fields (rule version, source ref), for `get_interpretation`.
pub fn build_with_statements(conn: &Connection, now: DateTime<Utc>) -> (Dashboard, Vec<Statement>) {
    let mut b = Builder {
        conn,
        now,
        hasher: Sha256::new(),
        oldest: None,
        unavailable: Vec::new(),
    };

    let solar_wind = b.solar_wind();
    let xray_flux = b.xray();
    let kp_index = b.kp();
    let scales = b.scales();
    let bulletins = b.bulletins();
    let three_day_forecast = b.three_day();
    let aurora = b.aurora();
    let solar_cycle = b.solar_cycle();

    // Same rule layer, inputs and source refs the desktop app uses.
    let mut statements = Vec::new();
    if let Some(today) = scales
        .as_ref()
        .and_then(|(_, d)| d.iter().find(|d| d.day_offset == 0))
    {
        let basis = Basis::ProviderObservation;
        if let Some(g) = today.g.scale {
            statements.extend(interpret::geomagnetic_effects(g, "noaa-scales:day0", basis));
        }
        if let Some(r) = today.r.scale {
            statements.extend(interpret::radio_effects(r, "noaa-scales:day0", basis));
        }
        if let Some(s) = today.s.scale {
            statements.extend(interpret::radiation_effects(s, "noaa-scales:day0", basis));
        }
    } else {
        statements.push(interpret::unavailable("NOAA scale status"));
    }
    // NOAA's own forecast days, only where it publishes a level above none.
    for day in scales
        .iter()
        .flat_map(|(_, d)| d.iter())
        .filter(|d| d.day_offset > 0)
    {
        let period = match day.time {
            Some(t) => format!("{} (forecast day {})", t.format("%-d %B"), day.day_offset),
            None => format!("forecast day {}", day.day_offset),
        };
        for scale in [day.g.scale, day.r.scale, day.s.scale]
            .into_iter()
            .flatten()
            .filter(|s| s.level > 0)
        {
            let source_ref = format!("noaa-scales:day{}", day.day_offset);
            statements.push(interpret::forecast_summary(
                scale,
                &period,
                "in the current NOAA scales product",
                &source_ref,
            ));
        }
    }
    if bulletins.is_none() {
        statements.push(interpret::unavailable("Alerts, watches and warnings"));
    }
    if three_day_forecast.is_none() {
        statements.push(interpret::unavailable("NOAA 3-day forecast"));
    }
    if aurora.is_none() {
        statements.push(interpret::unavailable("Aurora forecast"));
    }
    if let Some(w) = &solar_wind {
        if let Some(speed) = w.speed_km_s.latest {
            // As in the app: a Bz sample is paired with the speed only when the two
            // are within three cadences of each other; otherwise Bz is "unavailable".
            let time = |m: &Measure| {
                m.latest_time
                    .as_deref()
                    .and_then(|t| DateTime::parse_from_rfc3339(t).ok())
            };
            let paired = match (time(&w.speed_km_s), time(&w.bz_gsm_nt)) {
                (Some(a), Some(b)) => (a - b).abs() <= Duration::seconds(rtsw::CADENCE_SECONDS * 3),
                _ => false,
            };
            let bz = w.bz_gsm_nt.latest.filter(|_| paired);
            statements.push(interpret::solar_wind_speed_note(speed, bz, "rtsw_wind_1m"));
        }
    }

    // The 1-minute solar wind is the fastest-moving product, so its retrieval
    // age is the one that decides whether "right now" wording is honest.
    let age = solar_wind
        .as_ref()
        .and_then(|w| w.sources.iter().map(|s| s.retrieved_age_minutes).max())
        .or(b.oldest);
    let stale_data_warning = age.filter(|a| *a > 180).map(|a| {
        format!(
            "These readings were retrieved {} ago, so they describe conditions then, not now. \
             Open the Space Weather Observatory app to refresh them.",
            crate::reports::human_minutes(a)
        )
    });

    let dashboard = Dashboard {
        now: rfc3339(now),
        data_fingerprint: format!("{:x}", b.hasher.finalize())[..16].to_string(),
        oldest_retrieval_age_minutes: b.oldest,
        realtime_retrieval_age_minutes: age,
        stale_data_warning,
        solar_wind,
        xray_flux,
        kp_index,
        noaa_scales: scales.map(|(r, _)| r),
        bulletins,
        three_day_forecast,
        aurora,
        solar_cycle,
        statements: statements
            .iter()
            .map(|s| StatementReading {
                headline: s.headline.clone(),
                detail: s.detail.clone(),
                basis: s.basis.label().to_string(),
                region: s.region.clone(),
            })
            .collect(),
        unavailable: b.unavailable,
    };
    (dashboard, statements)
}
