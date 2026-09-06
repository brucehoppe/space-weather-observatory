/** Shapes returned by the Rust backend. Kept in one place so a schema change
 *  surfaces as a type error rather than as a silently wrong reading. */

export type Quality = "good" | "suspect" | "missing";
export type TimePrecision = "instant" | "interval";
export type FeedState = "ok" | "stale" | "error" | "unavailable";
export type Mode = "live" | "replay" | "demo";

export interface Observation {
  time: string;
  interval_seconds?: number;
  time_precision: TimePrecision;
  value: number | null;
  quality: Quality;
  instrument?: string;
}

export interface Provenance {
  source_url: string;
  retrieved_at: string;
  payload_sha256?: string;
  issued_at?: string;
}

export type Aggregation =
  | { kind: "raw" }
  | { kind: "downsampled"; method: string; bucket_seconds: number };

export interface Series {
  id: { provider: string; product: string; measurement: string };
  label: string;
  unit: string;
  frame?: string;
  nominal_cadence_seconds: number;
  aggregation: Aggregation;
  provenance: Provenance;
  samples: Observation[];
}

export interface ProductStatus {
  product: string;
  state: FeedState;
  last_success?: string;
  last_sample_time?: string;
  message?: string;
}

export type ScaleDomain = "G" | "R" | "S";
export interface NoaaScale {
  domain: ScaleDomain;
  level: number;
}

export type ForecastStatus = "active" | "expired" | "superseded" | "cancelled" | "issued";

export interface ForecastRecord {
  provider: string;
  product_id: string;
  serial?: string;
  issued_at: string;
  valid_from?: string;
  valid_to?: string;
  text: string;
  headline: string;
  scale?: NoaaScale;
  status: ForecastStatus;
  source_url: string;
}

export type KpKind = "observed" | "estimated" | "predicted";
export interface KpInterval {
  observation: Observation;
  kind: KpKind;
  noaa_scale: string | null;
  station_count: number | null;
}

export interface DomainStatus {
  domain: ScaleDomain;
  scale: NoaaScale | null;
  text: string | null;
  probabilities: [string, number][];
}

export interface ScaleDay {
  day_offset: number;
  time: string | null;
  g: DomainStatus;
  r: DomainStatus;
  s: DomainStatus;
}

export interface KpForecastCell {
  interval_start: string;
  interval_seconds: number;
  kp: number;
  noaa_scale: string | null;
}

export interface ThreeDayForecast {
  issued_at: string;
  covered_days: string[];
  kp: KpForecastCell[];
  rationale: string | null;
  source_url: string;
}

export interface AuroraMeta {
  observation_time: string;
  forecast_time: string;
  lead_time_minutes: number;
  model: string;
  units: string;
  source_url: string;
  product_page: string;
}

export type Basis = "provider_forecast" | "provider_observation" | "interpretation";

export interface Statement {
  headline: string;
  detail: string;
  basis: Basis;
  activity?: "aurora_viewing" | "hf_radio" | "gnss" | "satellite_and_power";
  region?: string;
  rule_version: string;
  source_ref: string;
}

export interface Dashboard {
  mode: Mode;
  assembled_at: string;
  snapshot_id: string;
  app_version: string;
  schema_version: number;
  series: Record<string, Series>;
  statuses: ProductStatus[];
  wind_spacecraft: string | null;
  mag_spacecraft: string | null;
  xray_satellite: string | null;
  kp: KpInterval[];
  scales: ScaleDay[];
  bulletins: ForecastRecord[];
  three_day: ThreeDayForecast | null;
  three_day_geomag: ThreeDayForecast | null;
  aurora: AuroraMeta | null;
  statements: Statement[];
}

export interface AuroraGrid {
  observation_time: string;
  forecast_time: string;
  data_format: string;
  lon_count: number;
  lat_count: number;
  lat_min: number;
  values: number[];
}

export interface AlertSettings {
  enabled: boolean;
  entry_threshold_km_s: number;
  persistence_minutes: number;
  hysteresis_km_s: number;
  clearance_minutes: number;
  cadence_seconds: number;
  stale_after_minutes: number;
  settings_version: number;
}

export interface Settings {
  schema_version: number;
  display_time_zone: string;
  region_label: string | null;
  alert: AlertSettings;
  cache_limit_mb: number;
  snapshot_retention: number;
  force_reduced_motion: boolean;
  window_bounds: unknown;
}

export interface Coverage {
  window_start: string;
  window_end: string;
  expected_samples: number;
  accepted_samples: number;
  largest_gap_seconds: number;
  adequate: boolean;
}

export interface Episode {
  id: string;
  qualified_onset: string;
  onset_speed_km_s: number;
  last_observation_time: string;
  last_speed_km_s: number;
  peak_speed_km_s: number;
  source_product: string;
  source_spacecraft: string | null;
  settings_version: number;
  rule_version: string;
  acknowledged: boolean;
  cleared_at: string | null;
  clearance_evidence?: Coverage;
}

export type PauseReason = "stale_feed" | "no_data" | "insufficient_coverage";

export type AlertState =
  | { state: "disabled" }
  | { state: "monitoring" }
  | { state: "pending"; since: string; elapsed_seconds: number; required_seconds: number }
  | { state: "active"; episode: Episode }
  | { state: "data_unavailable"; reason: PauseReason; retained_episode: Episode | null }
  | { state: "cleared"; episode: Episode };

export interface AlertMemory {
  open_episode: Episode | null;
  history: Episode[];
  settings_version: number;
}

export interface Evaluation {
  state: AlertState;
  memory: AlertMemory;
  coverage?: Coverage;
  newly_active: boolean;
}

export interface AlertView {
  evaluation: Evaluation;
  settings: AlertSettings;
  context: Mode;
  source_product: string;
  source_spacecraft: string | null;
  bz_gsm_nt: number | null;
  bz_time: string | null;
  bz_stale: boolean;
}

export interface AlertScenario {
  id: string;
  title: string;
  expectation: string;
  synthetic: boolean;
  samples: Observation[];
}

export interface SunImage {
  passband: "aia193" | "aia304";
  label: string;
  description: string;
  acquired_at: string;
  retrieved_at: string;
  provider_image_id: string;
  credit: string;
  source_url: string;
  data_uri: string;
  false_colour: boolean;
}

export interface SourceEntry {
  product: string;
  url: string;
  page: string | null;
  cadence_seconds: number;
  description: string;
}

export interface LessonStep {
  prompt: string;
  focus_series: string | null;
  select_time: string | null;
  view: string;
}

export interface Lesson {
  id: string;
  title: string;
  question: string;
  steps: LessonStep[];
  explanation: string;
  sources: string[];
  dataset: string;
  attribution: string;
}

export interface SnapshotRef {
  id: number;
  product: string;
  retrieved_at: string;
}

export interface CacheStatus {
  size_bytes: number;
  limit_bytes: number;
  snapshot_count: number;
  data_dir: string;
  config_dir: string;
}

export interface ExportPayload {
  csv: string;
  json: string;
  suggested_basename: string;
  metadata: unknown;
}

/** Canonical series keys used across the UI. */
export const SERIES = {
  speed: "noaa-swpc:rtsw_wind_1m:proton_speed",
  density: "noaa-swpc:rtsw_wind_1m:proton_density",
  bt: "noaa-swpc:rtsw_mag_1m:bt",
  bzGsm: "noaa-swpc:rtsw_mag_1m:bz_gsm",
  bzGse: "noaa-swpc:rtsw_mag_1m:bz_gse",
  xrayLong: "noaa-swpc:goes_xrays_1day:xray_flux_long",
  xrayShort: "noaa-swpc:goes_xrays_1day:xray_flux_short",
  kp: "noaa-swpc:planetary_k_index_forecast:kp_estimated",
} as const;
