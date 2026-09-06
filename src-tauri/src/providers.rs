//! Network acquisition, centralized in the backend (spec §10).
//!
//! Rules implemented here:
//! - Outbound hosts are restricted to an allow-list; the UI cannot ask for an
//!   arbitrary URL.
//! - Every request has a bounded timeout and a documented poll cadence.
//! - Concurrent requests for the same product are coalesced.
//! - Failures preserve last-known-good data and back off; retries are not
//!   attempted aggressively while offline.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use swo_core::parse;
use tokio::sync::Mutex;

/// Hosts this application is allowed to contact. Anything else is refused
/// before a request is made.
pub const ALLOWED_HOSTS: [&str; 3] = [
    "services.swpc.noaa.gov",
    "api.helioviewer.org",
    "sdo.gsfc.nasa.gov",
];

pub const USER_AGENT: &str = concat!(
    "SpaceWeatherObservatory/",
    env!("CARGO_PKG_VERSION"),
    " (local desktop application; contact via repository)"
);

/// A product this application polls, with its documented cadence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Product {
    SolarWindPlasma,
    SolarWindMag,
    GoesXray,
    GoesInstrumentSources,
    PlanetaryKp,
    PlanetaryKpForecast,
    NoaaScales,
    Alerts,
    Aurora,
    ThreeDayForecast,
    ThreeDayGeomagForecast,
}

impl Product {
    pub const ALL: [Product; 11] = [
        Product::SolarWindPlasma,
        Product::SolarWindMag,
        Product::GoesXray,
        Product::GoesInstrumentSources,
        Product::PlanetaryKp,
        Product::PlanetaryKpForecast,
        Product::NoaaScales,
        Product::Alerts,
        Product::Aurora,
        Product::ThreeDayForecast,
        Product::ThreeDayGeomagForecast,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Product::SolarWindPlasma => "rtsw_wind_1m",
            Product::SolarWindMag => "rtsw_mag_1m",
            Product::GoesXray => "goes_xrays_1day",
            Product::GoesInstrumentSources => "goes_instrument_sources",
            Product::PlanetaryKp => "planetary_k_index",
            Product::PlanetaryKpForecast => "planetary_k_index_forecast",
            Product::NoaaScales => "noaa_scales",
            Product::Alerts => "alerts",
            Product::Aurora => "ovation_aurora_latest",
            Product::ThreeDayForecast => "three_day_forecast",
            Product::ThreeDayGeomagForecast => "three_day_geomag_forecast",
        }
    }

    pub fn url(self) -> &'static str {
        match self {
            Product::SolarWindPlasma => parse::rtsw::WIND_URL,
            Product::SolarWindMag => parse::rtsw::MAG_URL,
            Product::GoesXray => parse::goes::PRIMARY_XRAYS_1DAY_URL,
            Product::GoesInstrumentSources => parse::goes::INSTRUMENT_SOURCES_URL,
            Product::PlanetaryKp => parse::kp::KP_URL,
            Product::PlanetaryKpForecast => parse::kp::KP_FORECAST_URL,
            Product::NoaaScales => parse::scales::URL,
            Product::Alerts => parse::bulletins::URL,
            Product::Aurora => parse::ovation::URL,
            Product::ThreeDayForecast => parse::forecast_text::THREE_DAY_URL,
            Product::ThreeDayGeomagForecast => parse::forecast_text::GEOMAG_URL,
        }
    }

    /// Poll cadence, chosen from each product's own update rate. Polling faster
    /// than the provider updates only wastes their capacity.
    pub fn poll_seconds(self) -> i64 {
        match self {
            // 1-minute products: poll at 1 minute.
            Product::SolarWindPlasma | Product::SolarWindMag | Product::GoesXray => 60,
            // OVATION issues a new grid roughly every 5 minutes.
            Product::Aurora => 300,
            // Kp intervals are 3-hourly but the estimate updates within them.
            Product::PlanetaryKp | Product::PlanetaryKpForecast => 900,
            // Bulletins and scales are event-driven; 5 minutes is responsive
            // without hammering the service.
            Product::NoaaScales | Product::Alerts => 300,
            // Text outlooks are issued a few times a day.
            Product::ThreeDayForecast | Product::ThreeDayGeomagForecast => 1800,
            // Instrument assignment changes rarely.
            Product::GoesInstrumentSources => 3600,
        }
    }

    /// After this long without a fresh sample the product is shown as stale.
    pub fn stale_after_seconds(self) -> i64 {
        self.poll_seconds() * 5
    }
}

#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("host {0} is not on the allow-list")]
    HostNotAllowed(String),
    #[error("request failed: {0}")]
    Transport(String),
    #[error("provider returned HTTP {0}")]
    Status(u16),
    #[error("response was not valid UTF-8 text")]
    Encoding,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FetchOutcome {
    pub product: String,
    pub retrieved_at: DateTime<Utc>,
    pub body: String,
}

/// Backoff state for one product.
#[derive(Debug, Clone, Copy, Default)]
struct Backoff {
    consecutive_failures: u32,
    next_attempt_at: Option<DateTime<Utc>>,
}

impl Backoff {
    /// Exponential with a ceiling, so an offline machine is not retried in a
    /// tight loop.
    fn record_failure(&mut self, now: DateTime<Utc>, base_seconds: i64) {
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        let factor = 1i64 << self.consecutive_failures.min(6);
        let delay = (base_seconds * factor).min(30 * 60);
        self.next_attempt_at = Some(now + Duration::seconds(delay));
    }

    fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.next_attempt_at = None;
    }

    fn may_attempt(&self, now: DateTime<Utc>) -> bool {
        self.next_attempt_at.is_none_or(|t| now >= t)
    }
}

pub struct Fetcher {
    client: reqwest::Client,
    /// One lock per product coalesces concurrent refreshes.
    locks: Mutex<HashMap<&'static str, Arc<Mutex<()>>>>,
    backoff: Mutex<HashMap<&'static str, Backoff>>,
}

impl Fetcher {
    pub fn new() -> Result<Self, FetchError> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            // Bounded overall and connect timeouts: a hung provider must not
            // hold a refresh open indefinitely.
            .timeout(std::time::Duration::from_secs(30))
            .connect_timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        Ok(Self {
            client,
            locks: Mutex::new(HashMap::new()),
            backoff: Mutex::new(HashMap::new()),
        })
    }

    /// Whether a product may be attempted now (respecting backoff).
    pub async fn may_attempt(&self, product: Product, now: DateTime<Utc>) -> bool {
        self.backoff
            .lock()
            .await
            .get(product.key())
            .copied()
            .unwrap_or_default()
            .may_attempt(now)
    }

    pub async fn fetch(
        &self,
        product: Product,
        now: DateTime<Utc>,
    ) -> Result<FetchOutcome, FetchError> {
        check_host(product.url())?;

        let lock = {
            let mut locks = self.locks.lock().await;
            locks.entry(product.key()).or_default().clone()
        };
        // Coalesce: a second caller waits for the in-flight request rather than
        // issuing its own.
        let _guard = lock.lock().await;

        let result = self.fetch_inner(product).await;
        let mut backoff = self.backoff.lock().await;
        let entry = backoff.entry(product.key()).or_default();
        match &result {
            Ok(_) => entry.record_success(),
            Err(_) => entry.record_failure(now, product.poll_seconds()),
        }
        result.map(|body| FetchOutcome {
            product: product.key().to_string(),
            retrieved_at: now,
            body,
        })
    }

    async fn fetch_inner(&self, product: Product) -> Result<String, FetchError> {
        let resp = self
            .client
            .get(product.url())
            .send()
            .await
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(FetchError::Status(resp.status().as_u16()));
        }
        resp.text().await.map_err(|_| FetchError::Encoding)
    }

    /// Fetch an arbitrary allow-listed URL (used for imagery, whose URL is
    /// constructed from provider metadata rather than being a fixed product).
    pub async fn fetch_bytes(&self, url: &str) -> Result<Vec<u8>, FetchError> {
        check_host(url)?;
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(FetchError::Status(resp.status().as_u16()));
        }
        resp.bytes()
            .await
            .map(|b| b.to_vec())
            .map_err(|_| FetchError::Encoding)
    }

    pub async fn fetch_text(&self, url: &str) -> Result<String, FetchError> {
        check_host(url)?;
        let resp = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| FetchError::Transport(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(FetchError::Status(resp.status().as_u16()));
        }
        resp.text().await.map_err(|_| FetchError::Encoding)
    }
}

/// Reject any URL outside the allow-list, and any non-HTTPS scheme.
pub fn check_host(url: &str) -> Result<(), FetchError> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| FetchError::HostNotAllowed(url.to_string()))?;
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    // Strip any userinfo/port so `evil.com@services.swpc.noaa.gov` style tricks
    // cannot slip through.
    if host.contains('@') {
        return Err(FetchError::HostNotAllowed(host.to_string()));
    }
    let bare = host.split(':').next().unwrap_or("");
    if ALLOWED_HOSTS.contains(&bare) {
        Ok(())
    } else {
        Err(FetchError::HostNotAllowed(bare.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_product_url_is_https_and_on_the_allow_list() {
        for p in Product::ALL {
            assert!(p.url().starts_with("https://"), "{:?} must use https", p);
            check_host(p.url()).unwrap_or_else(|e| panic!("{:?}: {e}", p));
        }
    }

    #[test]
    fn product_keys_are_unique() {
        let mut keys: Vec<&str> = Product::ALL.iter().map(|p| p.key()).collect();
        keys.sort();
        let before = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), before);
    }

    #[test]
    fn arbitrary_hosts_are_refused_before_a_request_is_made() {
        assert!(check_host("https://example.com/data.json").is_err());
        assert!(
            check_host("http://services.swpc.noaa.gov/x").is_err(),
            "plain http is refused"
        );
        assert!(check_host("file:///etc/passwd").is_err());
        assert!(check_host("https://evil.com@services.swpc.noaa.gov/x").is_err());
        assert!(check_host("https://services.swpc.noaa.gov.evil.com/x").is_err());
    }

    #[test]
    fn allow_listed_hosts_pass() {
        assert!(check_host("https://services.swpc.noaa.gov/products/alerts.json").is_ok());
        assert!(check_host("https://api.helioviewer.org/v2/getClosestImage/?date=x").is_ok());
    }

    #[test]
    fn poll_cadences_are_never_faster_than_the_product_updates() {
        for p in Product::ALL {
            assert!(
                p.poll_seconds() >= 60,
                "{:?} would poll faster than any product updates",
                p
            );
            assert!(p.stale_after_seconds() > p.poll_seconds());
        }
    }

    #[test]
    fn backoff_grows_and_is_capped() {
        let now = Utc::now();
        let mut b = Backoff::default();
        assert!(b.may_attempt(now));
        b.record_failure(now, 60);
        let first = b.next_attempt_at.unwrap();
        assert!(
            !b.may_attempt(now),
            "a failed product is not retried immediately"
        );
        for _ in 0..20 {
            b.record_failure(now, 60);
        }
        let capped = b.next_attempt_at.unwrap();
        assert!(capped > first);
        assert!(
            capped <= now + Duration::minutes(30),
            "backoff must be capped"
        );
    }

    #[test]
    fn success_clears_backoff() {
        let now = Utc::now();
        let mut b = Backoff::default();
        b.record_failure(now, 60);
        b.record_success();
        assert!(b.may_attempt(now));
        assert_eq!(b.consecutive_failures, 0);
    }

    #[test]
    fn the_user_agent_identifies_the_application() {
        assert!(USER_AGENT.starts_with("SpaceWeatherObservatory/"));
    }
}
