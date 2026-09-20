//! Reviewed plain-language explanations of every dashboard reading.
//!
//! A small local model should not explain space weather from memory. It is
//! handed these entries instead and asked to reword them for the reader, the
//! same way the desktop app keeps its interpretation wording in a versioned,
//! reviewed rule layer rather than generating it.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Version of the wording below, recorded on every explanation.
pub const GLOSSARY_VERSION: &str = "glossary/1";

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Entry {
    /// Stable id, e.g. `bz`. Pass it to `explain_reading`.
    pub id: String,
    pub name: String,
    /// Field of `get_dashboard` holding the current value.
    pub dashboard_field: String,
    pub units: String,
    /// What the reading is, in everyday words.
    pub what_it_is: String,
    /// How to read the number: typical values and what higher or lower means.
    pub how_to_read_it: String,
    /// Why a non-specialist might care.
    pub why_it_matters: String,
    /// What the reading cannot tell you. Keep these limits when explaining it.
    pub caveats: String,
}

struct Raw {
    id: &'static str,
    aliases: &'static [&'static str],
    name: &'static str,
    field: &'static str,
    units: &'static str,
    what: &'static str,
    how: &'static str,
    why: &'static str,
    caveats: &'static str,
}

const ENTRIES: &[Raw] = &[
    Raw {
        id: "solar_wind_speed",
        aliases: &["speed", "wind speed", "solar wind"],
        name: "Solar-wind speed",
        field: "solar_wind.speed_km_s",
        units: "km/s",
        what: "The Sun constantly blows a thin stream of charged gas into space, called the solar wind. This is how fast that stream is moving as it passes a monitoring spacecraft about 1.5 million km from Earth, on the Sun-facing side.",
        how: "Roughly 300-500 km/s is ordinary. 500-700 km/s is elevated, usually a fast stream from a coronal hole. Above 700 km/s is high and often follows an eruption from the Sun. This app labels 500 km/s and up as elevated; that is the app's own rule, not a NOAA threshold.",
        why: "Faster wind hits Earth's magnetic field harder, which can help stir up geomagnetic activity and aurora. Wind measured at the spacecraft reaches Earth roughly 30-60 minutes later.",
        caveats: "Speed alone does not cause a storm. The direction of the magnetic field carried by the wind (Bz) matters as much. A high speed is not a forecast.",
    },
    Raw {
        id: "solar_wind_density",
        aliases: &["density", "proton density"],
        name: "Solar-wind density",
        field: "solar_wind.density_per_cm3",
        units: "particles per cubic centimetre",
        what: "How many solar-wind particles are in each cubic centimetre of space at the monitoring spacecraft. Even a 'dense' solar wind is a far better vacuum than any laboratory can make.",
        how: "Around 1-10 per cm3 is typical. Values above about 20 are dense and often mark the front edge of a disturbance or the boundary between slow and fast wind.",
        why: "Dense wind pushes harder on Earth's magnetic field. A sudden jump in density together with a jump in speed can mark a shock wave arriving from the Sun.",
        caveats: "Density is noisy and the instrument sometimes reports gaps. On its own it says little about effects at the ground.",
    },
    Raw {
        id: "bt",
        aliases: &["total field", "imf", "magnetic field strength"],
        name: "Bt - total interplanetary magnetic field",
        field: "solar_wind.bt_nt",
        units: "nanotesla (nT)",
        what: "The solar wind carries a magnetic field with it. Bt is the overall strength of that field at the monitoring spacecraft, whatever direction it points.",
        how: "Around 2-8 nT is ordinary. Above about 10 nT is strong, and above 20 nT is very strong, typical of an eruption from the Sun passing by.",
        why: "Bt sets the limit on how strongly the solar wind can connect to Earth's field. A large Bt means a large Bz is possible.",
        caveats: "A strong Bt only matters for Earth when the field also points south (negative Bz).",
    },
    Raw {
        id: "bz",
        aliases: &["bz gsm", "north-south field", "southward"],
        name: "Bz - north-south magnetic field (GSM)",
        field: "solar_wind.bz_gsm_nt",
        units: "nanotesla (nT); negative means southward",
        what: "The north-south part of the magnetic field carried by the solar wind, measured in a frame lined up with Earth's own magnetic field (GSM).",
        how: "Positive is northward: Earth's field mostly deflects the wind. Negative is southward: the two fields link up and energy flows in. Around -5 nT held for an hour or more is noteworthy; -10 nT or lower, sustained, commonly accompanies geomagnetic storms. The dashboard also counts the minutes Bz spent southward in the last two hours.",
        why: "Think of southward Bz as an open door. Sustained southward Bz with fast wind is the usual recipe for geomagnetic storms and bright aurora.",
        caveats: "Bz flips often and cannot be predicted far ahead. A brief southward dip means little; persistence is what matters. It is a measurement, not a forecast.",
    },
    Raw {
        id: "xray_flux",
        aliases: &["x-ray", "xray", "flare", "flare class", "goes", "solar flare"],
        name: "Solar X-ray flux and flare class",
        field: "xray_flux",
        units: "watts per square metre (W/m^2), shown as a class letter and number such as C2.4",
        what: "How bright the Sun is in X-rays, measured by a GOES weather satellite. Solar flares are sudden bursts of X-rays, so this is the flare meter.",
        how: "Classes run A, B, C, M, X. Each letter is ten times stronger than the one before, and the number is the multiplier inside the letter, so M2 is twice M1 and X1 is ten times M1. A and B are background. C flares are small and common. M flares are medium. X flares are major.",
        why: "X-rays reach Earth in 8 minutes and disturb the upper atmosphere on the daylit side. M and X flares can fade or black out shortwave (HF) radio there for minutes to hours, which NOAA rates on the R scale.",
        caveats: "A flare's X-rays only affect the sunlit side of Earth. A flare is not the same as an eruption of material (CME); only some flares launch one, and that would arrive one to three days later.",
    },
    Raw {
        id: "kp_index",
        aliases: &["kp", "planetary k", "k-index", "k index"],
        name: "Kp index",
        field: "kp_index",
        units: "0 to 9, one value per 3-hour interval",
        what: "A worldwide score of how disturbed Earth's magnetic field is, combined from magnetometer stations around the globe.",
        how: "0-2 is quiet, 3 unsettled, 4 active, and 5 or more is a geomagnetic storm. NOAA's G scale starts there: Kp 5 = G1, 6 = G2, 7 = G3, 8 = G4, 9 = G5. Recent values are NOAA estimates, earlier ones are observed, and future ones are forecasts. The dashboard labels each kind and never mixes them.",
        why: "Higher Kp pushes the aurora toward the equator. Around Kp 5 aurora may reach places like the northern United States or Scotland, and higher Kp reaches lower latitudes.",
        caveats: "Kp is a 3-hour planet-wide average, so it lags what is happening right now and says nothing about one town. Seeing aurora also needs a dark, clear sky.",
    },
    Raw {
        id: "noaa_scales",
        aliases: &["g scale", "r scale", "s scale", "g/r/s", "scales", "storm level", "geomagnetic storm", "radio blackout", "radiation storm"],
        name: "NOAA space weather scales (G, R, S)",
        field: "noaa_scales",
        units: "level 0 (none) to 5 (extreme), separately for G, R and S",
        what: "NOAA's official severity ratings, like hurricane categories. G rates geomagnetic storms (disturbance of Earth's magnetic field). R rates radio blackouts caused by solar flares. S rates solar radiation storms (fast particles from the Sun).",
        how: "Level 1 is minor, 2 moderate, 3 strong, 4 severe, 5 extreme; 0 or 'none' means no level is published. day_offset 0 is the current status and 1-3 are NOAA's forecast days. Probabilities appear only where NOAA publishes them.",
        why: "These are the numbers official alerts and news reports quote. G relates to aurora and power grids, R to shortwave radio and aviation communication, S to satellites, polar flights and astronauts.",
        caveats: "The three scales measure different things and are never added into one score. For most people, levels 1-2 pass unnoticed. Forecast levels may not occur.",
    },
    Raw {
        id: "bulletins",
        aliases: &["alerts", "watches", "warnings", "alert", "watch", "warning", "summary"],
        name: "NOAA alerts, watches and warnings",
        field: "bulletins",
        units: "text bulletins with issue and validity times",
        what: "Official messages from NOAA's Space Weather Prediction Center. A WATCH means conditions are possible in the coming days. A WARNING means conditions are expected soon or under way. An ALERT records that a threshold was just reached. A SUMMARY reports an event after it ended.",
        how: "'active' bulletins are inside their validity window now. 'issued' bulletins are point-in-time records of something observed, not conditions in effect. Cancelled, expired and superseded bulletins are left out.",
        why: "This is the forecaster's own judgement, the most authoritative short-term guidance available.",
        caveats: "An empty list means no bulletin is cached for now. It does not prove conditions are quiet.",
    },
    Raw {
        id: "three_day_forecast",
        aliases: &["3-day forecast", "three day", "forecast", "outlook"],
        name: "NOAA 3-day forecast",
        field: "three_day_forecast",
        units: "highest forecast Kp per UTC day, with NOAA's rationale text",
        what: "NOAA's official outlook for the next three days: the expected Kp for each 3-hour period, and a short paragraph explaining the forecaster's reasoning.",
        how: "The dashboard shows the highest forecast Kp for each day and any G level NOAA printed beside it. Read it with the Kp scale: 5 or more means a geomagnetic storm is forecast.",
        why: "It is the best available answer to 'might there be aurora or disruption in the next few days?'.",
        caveats: "It is a forecast and may not happen. Timing of arrivals from the Sun is often uncertain by half a day. Days outside the product are not covered and are never extrapolated.",
    },
    Raw {
        id: "aurora",
        aliases: &["ovation", "aurora forecast", "northern lights", "southern lights", "aurora probability"],
        name: "Aurora forecast (OVATION model)",
        field: "aurora",
        units: "percent probability of visible aurora overhead, on a 1-degree map",
        what: "A NOAA computer model that turns the solar wind just measured upstream into a map of where aurora is likely in the next 30-90 minutes.",
        how: "max_probability_percent is the highest value anywhere in that hemisphere. equatorward_latitude_10_percent is how close to the equator the 10 % zone reaches: about 65 degrees is normal, and lower numbers mean the aurora oval has expanded toward more populated latitudes.",
        why: "It is the most direct 'should I look outside tonight?' product.",
        caveats: "It is a model, not a sighting. Aurora can be seen low on the horizon from well equatorward of the map. You still need darkness and clear sky, and the forecast only reaches about an hour ahead.",
    },
    Raw {
        id: "solar_cycle",
        aliases: &["sunspot number", "sunspots", "ssn", "f10.7", "solar cycle 25", "cycle"],
        name: "Solar cycle progression (sunspot number, F10.7)",
        field: "solar_cycle",
        units: "monthly sunspot number; F10.7 radio flux in solar flux units",
        what: "The Sun's activity rises and falls over about 11 years. The monthly sunspot number counts dark spots on the Sun, and F10.7 is the Sun's radio brightness, which follows the same rhythm. We are in Solar Cycle 25.",
        how: "Near minimum the sunspot number is under about 20; near maximum it is often 100-200. The smoothed number is a 13-month average, so it trails the newest month by about half a year. The predicted value and its low-high range come from the official forecast panel.",
        why: "More sunspots means more flares and eruptions, so storms and aurora are more frequent in the years around maximum. It is the climate behind the daily weather.",
        caveats: "This describes long-term climate only. It cannot tell you whether today or this week will be active.",
    },
    Raw {
        id: "solar_wind_alert",
        aliases: &["alert banner", "custom alert", "threshold alert"],
        name: "Custom solar-wind alert",
        field: "(desktop app only)",
        units: "km/s threshold and minutes of persistence, chosen by the user",
        what: "A condition detector in the desktop app. It reports that solar-wind speed stayed at or above a threshold you chose for as long as you chose, with enough valid data to be sure.",
        how: "Defaults are 500 km/s held for 10 minutes. It clears once the speed stays below the threshold minus a small margin for 5 minutes.",
        why: "It gives an early heads-up that faster wind has arrived, without watching the chart.",
        caveats: "It is not a NOAA product, not a G/R/S level and not a prediction of any outage or personal risk. It is evaluated only while the desktop app is open, so this server does not report it.",
    },
    Raw {
        id: "sun_imagery",
        aliases: &["sun image", "sdo", "helioviewer", "sun-image sequence"],
        name: "Sun imagery (NASA SDO)",
        field: "(desktop app only)",
        units: "images",
        what: "Pictures of the Sun from NASA's Solar Dynamics Observatory, fetched through Helioviewer. The desktop app plays the last day or so as a short sequence.",
        how: "Bright loops and patches are active regions where flares come from. Large dark areas are coronal holes, sources of fast solar wind.",
        why: "It shows where on the Sun today's activity is coming from.",
        caveats: "Images are not stored in the cache this server reads, so it cannot describe today's pictures.",
    },
    Raw {
        id: "data_quality",
        aliases: &["quality", "provenance", "stale", "staleness", "missing", "age", "retrieved_at", "source"],
        name: "Data quality, provenance and staleness",
        field: "*.source / *.sources, oldest_retrieval_age_minutes, unavailable",
        units: "timestamps (UTC), minutes of age, quality flags good / suspect / missing",
        what: "Every number carries where it came from (source_url), when the desktop app retrieved it (retrieved_at) and the provider's own sample time. Samples the provider marked bad are flagged missing and are never treated as zero.",
        how: "Solar-wind and X-ray data normally update every minute, Kp every 3 hours, forecasts a few times a day and the solar cycle monthly. A retrieval age far beyond that means the desktop app has not refreshed recently.",
        why: "Space weather changes within minutes. A report written from old data should say so plainly.",
        caveats: "This server never downloads anything itself. To refresh the data, open the desktop app. A panel listed under 'unavailable' is a gap in information, not evidence of quiet conditions.",
    },
];

fn entry(r: &Raw) -> Entry {
    Entry {
        id: r.id.into(),
        name: r.name.into(),
        dashboard_field: r.field.into(),
        units: r.units.into(),
        what_it_is: r.what.into(),
        how_to_read_it: r.how.into(),
        why_it_matters: r.why.into(),
        caveats: r.caveats.into(),
    }
}

pub fn all() -> Vec<Entry> {
    ENTRIES.iter().map(entry).collect()
}

pub fn ids() -> Vec<&'static str> {
    ENTRIES.iter().map(|r| r.id).collect()
}

/// Find an entry by id, alias or a loose match on the name. Small models pass
/// things like "Bz (GSM)" or "the Kp number", so matching is forgiving.
pub fn find(query: &str) -> Option<Entry> {
    let q = query.trim().to_ascii_lowercase().replace(['-', '_'], " ");
    if q.is_empty() {
        return None;
    }
    let norm = |s: &str| s.to_ascii_lowercase().replace(['-', '_'], " ");
    let names = |r: &Raw| -> Vec<String> {
        std::iter::once(norm(r.id))
            .chain(r.aliases.iter().map(|a| norm(a)))
            .collect()
    };
    ENTRIES
        .iter()
        .find(|r| names(r).contains(&q))
        .or_else(|| {
            // The longest id/alias mentioned in the query wins, so "solar wind
            // density" picks density rather than the shorter "solar wind".
            ENTRIES
                .iter()
                .filter_map(|r| {
                    let best = names(r)
                        .into_iter()
                        .filter(|a| q.contains(a.as_str()))
                        .map(|a| a.len())
                        .max();
                    best.map(|len| (len, r))
                })
                .max_by_key(|(len, _)| *len)
                .map(|(_, r)| r)
        })
        .or_else(|| {
            ENTRIES
                .iter()
                .find(|r| q.len() >= 2 && names(r).iter().any(|a| a.contains(&q)))
        })
        .map(entry)
}
