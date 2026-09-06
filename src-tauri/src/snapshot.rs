//! Assembly of a coherent dashboard snapshot from stored source payloads.
//!
//! A snapshot is the unit the UI reads and the unit an export is taken from,
//! so a partially refreshed view can never be exported as if it were coherent
//! (spec §12). One failing product degrades its own panel only (spec §10).

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use swo_core::interpret::{self, Basis, Statement};
use swo_core::model::*;
use swo_core::parse::{self, bulletins, forecast_text, goes, kp, ovation, rtsw, scales};

use crate::providers::Product;
use crate::store::Snapshot as StoredSnapshot;

/// What the UI is currently showing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    /// Live provider data.
    Live,
    /// A stored snapshot being replayed. Never mixed with live state.
    Replay,
    /// The attributed frozen demonstration dataset.
    Demo,
}

/// Everything the dashboard renders, with provenance attached throughout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Dashboard {
    pub mode: Mode,
    pub assembled_at: DateTime<Utc>,
    pub snapshot_id: String,
    pub app_version: String,
    pub schema_version: u32,
    /// Normalized series keyed by `SeriesId::key()`.
    pub series: BTreeMap<String, Series>,
    pub statuses: Vec<ProductStatus>,
    /// Spacecraft currently supplying each upstream stream, as the provider
    /// states it. `None` when the provider marks none active.
    pub wind_spacecraft: Option<String>,
    pub mag_spacecraft: Option<String>,
    pub xray_satellite: Option<String>,
    pub kp: Vec<kp::KpInterval>,
    pub scales: Vec<scales::ScaleDay>,
    pub bulletins: Vec<ForecastRecord>,
    pub three_day: Option<forecast_text::ThreeDayForecast>,
    pub three_day_geomag: Option<forecast_text::ThreeDayForecast>,
    /// Aurora product metadata; the grid itself is fetched separately because
    /// of its size.
    pub aurora: Option<AuroraMeta>,
    /// Plain-language statements, each traceable to its rule version + source.
    pub statements: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AuroraMeta {
    pub observation_time: DateTime<Utc>,
    pub forecast_time: DateTime<Utc>,
    pub lead_time_minutes: i64,
    pub model: String,
    pub units: String,
    pub source_url: String,
    pub product_page: String,
}

/// Raw payloads keyed by product key, as stored.
pub type Payloads = BTreeMap<String, StoredSnapshot>;

#[allow(clippy::too_many_arguments)] // one call site per product; a struct here
                                     // would only move the same fields around
fn series_of(
    product: &str,
    measurement: &str,
    label: &str,
    unit: &str,
    frame: Option<&str>,
    cadence: i64,
    samples: Vec<Observation>,
    stored: &StoredSnapshot,
    issued_at: Option<DateTime<Utc>>,
) -> Series {
    Series {
        id: SeriesId::new("noaa-swpc", product, measurement),
        label: label.to_string(),
        unit: unit.to_string(),
        frame: frame.map(str::to_string),
        nominal_cadence_seconds: cadence,
        aggregation: Aggregation::Raw,
        provenance: Provenance {
            source_url: stored.source_url.clone(),
            retrieved_at: stored.retrieved_at,
            payload_sha256: Some(stored.sha256.clone()),
            issued_at,
        },
        samples: swo_core::timeline::normalize_samples(samples),
    }
}

fn status(
    product: Product,
    stored: Option<&StoredSnapshot>,
    last_sample: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
    error: Option<String>,
) -> ProductStatus {
    let state = match (stored, &error, last_sample) {
        (None, Some(_), _) => FeedState::Unavailable,
        (None, None, _) => FeedState::Unavailable,
        (Some(_), Some(_), _) => FeedState::Error,
        (Some(_), None, Some(t)) => {
            if (now - t) > Duration::seconds(product.stale_after_seconds()) {
                FeedState::Stale
            } else {
                FeedState::Ok
            }
        }
        (Some(s), None, None) => {
            if (now - s.retrieved_at) > Duration::seconds(product.stale_after_seconds()) {
                FeedState::Stale
            } else {
                FeedState::Ok
            }
        }
    };
    ProductStatus {
        product: product.key().to_string(),
        state,
        last_success: stored.map(|s| s.retrieved_at),
        last_sample_time: last_sample,
        message: error,
    }
}

/// Build the dashboard. Parse failures are recorded per product; they never
/// abort assembly and never produce an empty-but-successful dataset.
pub fn assemble(
    mode: Mode,
    payloads: &Payloads,
    now: DateTime<Utc>,
    snapshot_id: String,
) -> Dashboard {
    let mut series: BTreeMap<String, Series> = BTreeMap::new();
    let mut statuses: Vec<ProductStatus> = Vec::new();
    let mut statements: Vec<Statement> = Vec::new();

    let mut wind_spacecraft = None;
    let mut mag_spacecraft = None;
    let mut xray_satellite = None;

    // --- Upstream solar wind (plasma) -------------------------------------
    let mut wind_last = None;
    let mut wind_err = None;
    if let Some(stored) = payloads.get(Product::SolarWindPlasma.key()) {
        match rtsw::parse_wind(&stored.payload) {
            Ok(streams) => match rtsw::active_wind(&streams) {
                Some(active) => {
                    wind_spacecraft = Some(active.spacecraft.clone());
                    wind_last = active.speed_km_s.last().map(|o| o.time);
                    for (measurement, label, unit, samples) in [
                        (
                            "proton_speed",
                            "Solar-wind speed",
                            "km/s",
                            active.speed_km_s.clone(),
                        ),
                        (
                            "proton_density",
                            "Solar-wind proton density",
                            "particles/cm^3",
                            active.density_per_cm3.clone(),
                        ),
                    ] {
                        let s = series_of(
                            Product::SolarWindPlasma.key(),
                            measurement,
                            label,
                            unit,
                            None,
                            rtsw::CADENCE_SECONDS,
                            samples,
                            stored,
                            None,
                        );
                        series.insert(s.id.key(), s);
                    }
                }
                None => wind_err = Some("provider marks no solar-wind stream active".into()),
            },
            Err(e) => wind_err = Some(e.to_string()),
        }
    }
    statuses.push(status(
        Product::SolarWindPlasma,
        payloads.get(Product::SolarWindPlasma.key()),
        wind_last,
        now,
        wind_err,
    ));

    // --- Upstream solar wind (magnetic field) ------------------------------
    let mut mag_last = None;
    let mut mag_err = None;
    if let Some(stored) = payloads.get(Product::SolarWindMag.key()) {
        match rtsw::parse_mag(&stored.payload) {
            Ok(streams) => match rtsw::active_mag(&streams) {
                Some(active) => {
                    mag_spacecraft = Some(active.spacecraft.clone());
                    mag_last = active.bt_nt.last().map(|o| o.time);
                    for (measurement, label, frame, samples) in [
                        ("bt", "Total magnetic field |B|", None, active.bt_nt.clone()),
                        ("bz_gsm", "Bz", Some("GSM"), active.bz_gsm_nt.clone()),
                        ("bz_gse", "Bz", Some("GSE"), active.bz_gse_nt.clone()),
                    ] {
                        let s = series_of(
                            Product::SolarWindMag.key(),
                            measurement,
                            label,
                            "nT",
                            frame,
                            rtsw::CADENCE_SECONDS,
                            samples,
                            stored,
                            None,
                        );
                        series.insert(s.id.key(), s);
                    }
                }
                None => mag_err = Some("provider marks no magnetometer stream active".into()),
            },
            Err(e) => mag_err = Some(e.to_string()),
        }
    }
    statuses.push(status(
        Product::SolarWindMag,
        payloads.get(Product::SolarWindMag.key()),
        mag_last,
        now,
        mag_err,
    ));

    // --- GOES X-ray flux ---------------------------------------------------
    let mut xray_last = None;
    let mut xray_err = None;
    if let Some(stored) = payloads.get(Product::GoesXray.key()) {
        match goes::parse_xrays(&stored.payload) {
            Ok(channels) => {
                for c in &channels {
                    xray_satellite = Some(c.satellite.clone());
                    xray_last = c.flux_w_m2.last().map(|o| o.time).max(xray_last);
                    let measurement = match c.band {
                        swo_core::flare::XrayBand::Long => "xray_flux_long",
                        swo_core::flare::XrayBand::Short => "xray_flux_short",
                    };
                    let s = series_of(
                        Product::GoesXray.key(),
                        measurement,
                        &format!("GOES X-ray flux {}", c.band.label()),
                        "W/m^2",
                        None,
                        goes::CADENCE_SECONDS,
                        c.flux_w_m2.clone(),
                        stored,
                        None,
                    );
                    series.insert(s.id.key(), s);
                }
            }
            Err(e) => xray_err = Some(e.to_string()),
        }
    }
    statuses.push(status(
        Product::GoesXray,
        payloads.get(Product::GoesXray.key()),
        xray_last,
        now,
        xray_err,
    ));

    // --- Planetary Kp ------------------------------------------------------
    let mut kp_values: Vec<kp::KpInterval> = Vec::new();
    let mut kp_err = None;
    if let Some(stored) = payloads.get(Product::PlanetaryKpForecast.key()) {
        match kp::parse_kp_forecast(&stored.payload) {
            Ok(v) => kp_values = v,
            Err(e) => kp_err = Some(e.to_string()),
        }
    }
    if kp_values.is_empty() {
        if let Some(stored) = payloads.get(Product::PlanetaryKp.key()) {
            match kp::parse_kp(&stored.payload) {
                Ok(v) => kp_values = v,
                Err(e) => kp_err = Some(e.to_string()),
            }
        }
    }
    // Estimated/observed intervals become a chartable series; forecast
    // intervals stay in `kp` so the UI can render them distinctly.
    if let Some(stored) = payloads
        .get(Product::PlanetaryKpForecast.key())
        .or_else(|| payloads.get(Product::PlanetaryKp.key()))
    {
        let observed: Vec<Observation> = kp_values
            .iter()
            .filter(|i| !i.kind.is_forecast())
            .map(|i| i.observation.clone())
            .collect();
        if !observed.is_empty() {
            let s = series_of(
                Product::PlanetaryKpForecast.key(),
                "kp_estimated",
                "Planetary Kp (estimated / observed)",
                "Kp",
                None,
                kp::INTERVAL_SECONDS,
                observed,
                stored,
                None,
            );
            series.insert(s.id.key(), s);
        }
    }
    let kp_last = kp_values
        .iter()
        .filter(|i| !i.kind.is_forecast())
        .map(|i| i.observation.time)
        .max();
    statuses.push(status(
        Product::PlanetaryKpForecast,
        payloads.get(Product::PlanetaryKpForecast.key()),
        kp_last,
        now,
        kp_err,
    ));

    // --- NOAA scales -------------------------------------------------------
    let mut scale_days: Vec<scales::ScaleDay> = Vec::new();
    let mut scales_err = None;
    if let Some(stored) = payloads.get(Product::NoaaScales.key()) {
        match scales::parse(&stored.payload) {
            Ok(d) => scale_days = d,
            Err(e) => scales_err = Some(e.to_string()),
        }
    }
    statuses.push(status(
        Product::NoaaScales,
        payloads.get(Product::NoaaScales.key()),
        scale_days.iter().filter_map(|d| d.time).max(),
        now,
        scales_err,
    ));

    // Statements for the current published status, per domain, kept separate.
    if let Some(today) = scale_days.iter().find(|d| d.day_offset == 0) {
        if let Some(g) = today.g.scale {
            statements.extend(interpret::geomagnetic_effects(
                g,
                "noaa-scales:day0",
                Basis::ProviderObservation,
            ));
        }
        if let Some(r) = today.r.scale {
            statements.extend(interpret::radio_effects(
                r,
                "noaa-scales:day0",
                Basis::ProviderObservation,
            ));
        }
        if let Some(s) = today.s.scale {
            statements.extend(interpret::radiation_effects(
                s,
                "noaa-scales:day0",
                Basis::ProviderObservation,
            ));
        }
    } else {
        statements.push(interpret::unavailable("NOAA scale status"));
    }

    // --- Bulletins ---------------------------------------------------------
    let mut records: Vec<ForecastRecord> = Vec::new();
    let mut alerts_err = None;
    if let Some(stored) = payloads.get(Product::Alerts.key()) {
        match bulletins::parse(&stored.payload) {
            Ok(b) => records = bulletins::resolve_statuses(&b, now),
            Err(e) => alerts_err = Some(e.to_string()),
        }
    } else {
        statements.push(interpret::unavailable("Alerts, watches and warnings"));
    }
    records.sort_by_key(|r| std::cmp::Reverse(r.issued_at));
    statuses.push(status(
        Product::Alerts,
        payloads.get(Product::Alerts.key()),
        records.first().map(|r| r.issued_at),
        now,
        alerts_err,
    ));

    // --- Text outlooks -----------------------------------------------------
    let mut three_day = None;
    let mut three_day_err = None;
    if let Some(stored) = payloads.get(Product::ThreeDayForecast.key()) {
        match forecast_text::parse_three_day(&stored.payload) {
            Ok(f) => three_day = Some(f),
            Err(e) => three_day_err = Some(e.to_string()),
        }
    } else {
        statements.push(interpret::unavailable("NOAA 3-day forecast"));
    }
    statuses.push(status(
        Product::ThreeDayForecast,
        payloads.get(Product::ThreeDayForecast.key()),
        three_day.as_ref().map(|f| f.issued_at),
        now,
        three_day_err,
    ));

    let mut three_day_geomag = None;
    let mut geomag_err = None;
    if let Some(stored) = payloads.get(Product::ThreeDayGeomagForecast.key()) {
        match forecast_text::parse_geomag(&stored.payload) {
            Ok(f) => three_day_geomag = Some(f),
            Err(e) => geomag_err = Some(e.to_string()),
        }
    }
    statuses.push(status(
        Product::ThreeDayGeomagForecast,
        payloads.get(Product::ThreeDayGeomagForecast.key()),
        three_day_geomag.as_ref().map(|f| f.issued_at),
        now,
        geomag_err,
    ));

    // --- Aurora metadata ---------------------------------------------------
    let mut aurora = None;
    let mut aurora_err = None;
    if let Some(stored) = payloads.get(Product::Aurora.key()) {
        match ovation::parse(&stored.payload) {
            Ok(grid) => {
                aurora = Some(AuroraMeta {
                    observation_time: grid.observation_time,
                    forecast_time: grid.forecast_time,
                    lead_time_minutes: grid.lead_time_minutes(),
                    model: "NOAA OVATION aurora model".into(),
                    units: "probability of visible aurora in the model grid cell (%)".into(),
                    source_url: ovation::URL.into(),
                    product_page: ovation::PAGE_URL.into(),
                })
            }
            Err(e) => aurora_err = Some(e.to_string()),
        }
    } else {
        statements.push(interpret::unavailable("Aurora forecast"));
    }
    statuses.push(status(
        Product::Aurora,
        payloads.get(Product::Aurora.key()),
        aurora.as_ref().map(|a| a.observation_time),
        now,
        aurora_err,
    ));

    // --- Observation-based commentary --------------------------------------
    let speed_key =
        SeriesId::new("noaa-swpc", Product::SolarWindPlasma.key(), "proton_speed").key();
    let bz_key = SeriesId::new("noaa-swpc", Product::SolarWindMag.key(), "bz_gsm").key();
    if let Some(speed) = series.get(&speed_key).and_then(|s| s.last_accepted()) {
        // Bz is looked up with an explicit tolerance; a stale Bz is reported as
        // unavailable rather than paired with a fresh speed.
        let bz = series.get(&bz_key).and_then(|s| {
            s.last_accepted()
                .filter(|o| {
                    (speed.time - o.time).abs() <= Duration::seconds(rtsw::CADENCE_SECONDS * 3)
                })
                .and_then(|o| o.value)
        });
        statements.push(interpret::solar_wind_speed_note(
            speed.value.unwrap_or_default(),
            bz,
            &speed_key,
        ));
    }

    Dashboard {
        mode,
        assembled_at: now,
        snapshot_id,
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        schema_version: swo_core::SCHEMA_VERSION,
        series,
        statuses,
        wind_spacecraft,
        mag_spacecraft,
        xray_satellite,
        kp: kp_values,
        scales: scale_days,
        bulletins: records,
        three_day,
        three_day_geomag,
        aurora,
        statements,
    }
}

/// Parse the aurora grid from a stored payload, for the aurora view.
pub fn aurora_grid(payloads: &Payloads) -> Option<ovation::AuroraGrid> {
    payloads
        .get(Product::Aurora.key())
        .and_then(|s| parse::ovation::parse(&s.payload).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn stored(
        product: &str,
        url: &str,
        payload: &str,
        retrieved_at: DateTime<Utc>,
    ) -> StoredSnapshot {
        StoredSnapshot {
            id: 1,
            product: product.into(),
            source_url: url.into(),
            retrieved_at,
            sha256: crate::store::sha256_hex(payload),
            payload: payload.into(),
        }
    }

    fn fixture_payloads(now: DateTime<Utc>) -> Payloads {
        let mut p = Payloads::new();
        let files: [(Product, &str, &str); 8] = [
            (
                Product::SolarWindPlasma,
                "rtsw_wind_1m.json",
                rtsw::WIND_URL,
            ),
            (Product::SolarWindMag, "rtsw_mag_1m.json", rtsw::MAG_URL),
            (
                Product::GoesXray,
                "goes_primary_xrays_1day.json",
                goes::PRIMARY_XRAYS_1DAY_URL,
            ),
            (
                Product::PlanetaryKpForecast,
                "planetary_k_index_forecast.json",
                kp::KP_FORECAST_URL,
            ),
            (Product::NoaaScales, "noaa_scales.json", scales::URL),
            (Product::Alerts, "alerts.json", bulletins::URL),
            (Product::Aurora, "ovation_aurora_latest.json", ovation::URL),
            (
                Product::ThreeDayForecast,
                "3-day-forecast.txt",
                forecast_text::THREE_DAY_URL,
            ),
        ];
        for (product, file, url) in files {
            let path = format!("{}/../fixtures/captured/{file}", env!("CARGO_MANIFEST_DIR"));
            let body = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
            p.insert(
                product.key().to_string(),
                stored(product.key(), url, &body, now),
            );
        }
        p
    }

    /// Slightly after the fixture capture time, so nothing reads as stale.
    fn capture_time() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 9, 6, 18, 5, 0).unwrap()
    }

    #[test]
    fn a_full_fixture_set_assembles_every_required_series() {
        let d = assemble(
            Mode::Live,
            &fixture_payloads(capture_time()),
            capture_time(),
            "test".into(),
        );
        for key in [
            "noaa-swpc:rtsw_wind_1m:proton_speed",
            "noaa-swpc:rtsw_wind_1m:proton_density",
            "noaa-swpc:rtsw_mag_1m:bt",
            "noaa-swpc:rtsw_mag_1m:bz_gsm",
            "noaa-swpc:goes_xrays_1day:xray_flux_long",
            "noaa-swpc:planetary_k_index_forecast:kp_estimated",
        ] {
            assert!(d.series.contains_key(key), "missing series {key}");
        }
        assert!(d.wind_spacecraft.is_some());
        assert!(d.xray_satellite.is_some());
        assert!(d.aurora.is_some());
        assert!(d.three_day.is_some());
        assert!(!d.bulletins.is_empty());
    }

    #[test]
    fn units_and_frames_are_attached_to_every_series() {
        let d = assemble(
            Mode::Live,
            &fixture_payloads(capture_time()),
            capture_time(),
            "t".into(),
        );
        assert_eq!(d.series["noaa-swpc:rtsw_wind_1m:proton_speed"].unit, "km/s");
        assert_eq!(
            d.series["noaa-swpc:goes_xrays_1day:xray_flux_long"].unit,
            "W/m^2"
        );
        assert_eq!(
            d.series["noaa-swpc:rtsw_mag_1m:bz_gsm"].frame.as_deref(),
            Some("GSM")
        );
        assert_eq!(
            d.series["noaa-swpc:rtsw_mag_1m:bz_gse"].frame.as_deref(),
            Some("GSE")
        );
    }

    #[test]
    fn forecast_kp_stays_out_of_the_observed_series() {
        let d = assemble(
            Mode::Live,
            &fixture_payloads(capture_time()),
            capture_time(),
            "t".into(),
        );
        let observed = &d.series["noaa-swpc:planetary_k_index_forecast:kp_estimated"];
        let latest_observed = observed.samples.iter().map(|o| o.time).max().unwrap();
        let first_forecast =
            d.kp.iter()
                .filter(|i| i.kind.is_forecast())
                .map(|i| i.observation.time)
                .min()
                .unwrap();
        assert!(
            first_forecast > latest_observed,
            "forecast values must never enter the observed series"
        );
        assert!(
            d.kp.iter().any(|i| i.kind.is_forecast()),
            "forecasts are still available separately"
        );
    }

    #[test]
    fn one_failing_product_does_not_disable_the_rest() {
        let mut payloads = fixture_payloads(capture_time());
        payloads.insert(
            Product::SolarWindPlasma.key().into(),
            stored(
                Product::SolarWindPlasma.key(),
                rtsw::WIND_URL,
                "{ broken",
                capture_time(),
            ),
        );
        let d = assemble(Mode::Live, &payloads, capture_time(), "t".into());
        assert!(!d.series.contains_key("noaa-swpc:rtsw_wind_1m:proton_speed"));
        assert!(
            d.series
                .contains_key("noaa-swpc:goes_xrays_1day:xray_flux_long"),
            "other panels still work"
        );
        let s = d
            .statuses
            .iter()
            .find(|s| s.product == "rtsw_wind_1m")
            .unwrap();
        assert_eq!(s.state, FeedState::Error);
        assert!(s.message.is_some(), "the failure is reported, not hidden");
    }

    #[test]
    fn a_missing_product_is_unavailable_and_never_reads_as_quiet() {
        let mut payloads = fixture_payloads(capture_time());
        payloads.remove(Product::Alerts.key());
        let d = assemble(Mode::Live, &payloads, capture_time(), "t".into());
        let s = d.statuses.iter().find(|s| s.product == "alerts").unwrap();
        assert_eq!(s.state, FeedState::Unavailable);
        assert!(d
            .statements
            .iter()
            .any(|st| st.headline.contains("unavailable")));
        assert!(!d
            .statements
            .iter()
            .any(|st| st.headline.to_lowercase().contains("all quiet")));
    }

    #[test]
    fn old_data_is_reported_stale_rather_than_current() {
        let much_later = capture_time() + Duration::hours(6);
        let d = assemble(
            Mode::Live,
            &fixture_payloads(capture_time()),
            much_later,
            "t".into(),
        );
        let s = d
            .statuses
            .iter()
            .find(|s| s.product == "rtsw_wind_1m")
            .unwrap();
        assert_eq!(s.state, FeedState::Stale);
        assert!(
            s.last_sample_time.is_some(),
            "the last real sample time is still shown"
        );
    }

    #[test]
    fn every_series_carries_its_source_url_and_payload_hash() {
        let d = assemble(
            Mode::Live,
            &fixture_payloads(capture_time()),
            capture_time(),
            "t".into(),
        );
        for (key, s) in &d.series {
            assert!(s.provenance.source_url.starts_with("https://"), "{key}");
            assert_eq!(
                s.provenance.payload_sha256.as_ref().unwrap().len(),
                64,
                "{key}"
            );
        }
    }

    #[test]
    fn statements_are_generated_only_from_published_levels() {
        let d = assemble(
            Mode::Live,
            &fixture_payloads(capture_time()),
            capture_time(),
            "t".into(),
        );
        assert!(!d.statements.is_empty());
        for st in &d.statements {
            assert_eq!(st.rule_version, interpret::RULE_VERSION);
        }
        assert!(
            d.statements
                .iter()
                .any(|s| s.basis == Basis::Interpretation),
            "observation commentary is present and labelled as interpretation"
        );
    }

    #[test]
    fn the_aurora_grid_is_available_from_the_same_payloads() {
        let g = aurora_grid(&fixture_payloads(capture_time())).unwrap();
        assert_eq!(g.lon_count, 360);
    }

    #[test]
    fn assembly_is_deterministic_for_the_same_inputs() {
        let p = fixture_payloads(capture_time());
        let a = assemble(Mode::Live, &p, capture_time(), "t".into());
        let b = assemble(Mode::Live, &p, capture_time(), "t".into());
        assert_eq!(a, b);
    }
}
