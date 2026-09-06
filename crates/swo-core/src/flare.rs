//! GOES X-ray flux classification (spec §7).
//!
//! The letter class is defined on the **0.1–0.8 nm (long) passband** only, in
//! W/m^2 (see NOAA SWPC GOES X-ray flux product documentation,
//! <https://www.swpc.noaa.gov/products/goes-x-ray-flux>). Applying it to the
//! 0.05–0.4 nm short channel is a category error, so this module refuses to.
//!
//! A class computed here describes the **instantaneous flux** at one sample. It
//! is not an officially identified flare event, and it is not evidence of an
//! Earth-directed CME. Officially identified events come from SWPC event
//! products and are labelled separately in the UI.

use serde::{Deserialize, Serialize};

/// The passband a flux sample was measured in, as reported by the provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum XrayBand {
    /// 0.1–0.8 nm. The band the A/B/C/M/X classification is defined on.
    Long,
    /// 0.05–0.4 nm. Reported and plotted, but not classified.
    Short,
}

impl XrayBand {
    /// Map the provider's `energy` string to a band. Unknown strings return
    /// `None` rather than being guessed into the classified band.
    pub fn from_provider_energy(energy: &str) -> Option<Self> {
        match energy.trim() {
            "0.1-0.8nm" => Some(XrayBand::Long),
            "0.05-0.4nm" => Some(XrayBand::Short),
            _ => None,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            XrayBand::Long => "0.1–0.8 nm (long)",
            XrayBand::Short => "0.05–0.4 nm (short)",
        }
    }
}

/// An instantaneous flux class such as `C5.2`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FluxClass {
    pub letter: char,
    /// Sub-class multiplier within the decade, e.g. 5.2 in `C5.2`.
    pub sub: f64,
}

impl FluxClass {
    pub fn format(&self) -> String {
        // NOAA convention: one decimal, and the A class is often written
        // without a sub-class, but keeping one decimal everywhere is
        // unambiguous and avoids implying more precision than reported.
        format!("{}{:.1}", self.letter, self.sub)
    }
}

/// Lower bound of each class decade in W/m^2 on the 0.1–0.8 nm band.
const CLASS_FLOORS: [(char, f64); 5] = [
    ('X', 1e-4),
    ('M', 1e-5),
    ('C', 1e-6),
    ('B', 1e-7),
    ('A', 1e-8),
];

/// Classify a long-band flux value.
///
/// Returns `None` for the short band, for non-finite values, and for values at
/// or below zero — a log axis and a log classification cannot represent them.
/// The UI states that such samples are excluded rather than plotting them as
/// the axis floor (spec §7).
pub fn classify(flux_w_m2: f64, band: XrayBand) -> Option<FluxClass> {
    if band != XrayBand::Long || !flux_w_m2.is_finite() || flux_w_m2 <= 0.0 {
        return None;
    }
    for (letter, floor) in CLASS_FLOORS {
        if flux_w_m2 >= floor {
            return Some(FluxClass {
                letter,
                sub: flux_w_m2 / floor,
            });
        }
    }
    // Below A1.0: still A class, sub-class below 1.
    Some(FluxClass {
        letter: 'A',
        sub: flux_w_m2 / 1e-8,
    })
}

/// Whether a flux value can be shown on a logarithmic axis.
pub fn plottable_on_log_axis(flux_w_m2: f64) -> bool {
    flux_w_m2.is_finite() && flux_w_m2 > 0.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_boundaries_classify_exactly() {
        // Boundary values fall in the class they open, not the one below.
        assert_eq!(classify(1e-4, XrayBand::Long).unwrap().format(), "X1.0");
        assert_eq!(classify(1e-5, XrayBand::Long).unwrap().format(), "M1.0");
        assert_eq!(classify(1e-6, XrayBand::Long).unwrap().format(), "C1.0");
        assert_eq!(classify(1e-7, XrayBand::Long).unwrap().format(), "B1.0");
        assert_eq!(classify(1e-8, XrayBand::Long).unwrap().format(), "A1.0");
    }

    #[test]
    fn just_below_a_boundary_stays_in_the_lower_class() {
        assert_eq!(classify(9.99e-5, XrayBand::Long).unwrap().letter, 'M');
        assert_eq!(classify(9.99e-7, XrayBand::Long).unwrap().letter, 'B');
    }

    #[test]
    fn independent_reference_cases() {
        // Cross-checks against published examples of the NOAA notation.
        assert_eq!(classify(5.2e-6, XrayBand::Long).unwrap().format(), "C5.2");
        assert_eq!(classify(2.8e-5, XrayBand::Long).unwrap().format(), "M2.8");
        assert_eq!(classify(4.5e-4, XrayBand::Long).unwrap().format(), "X4.5");
    }

    #[test]
    fn short_band_is_never_classified() {
        assert!(classify(1e-4, XrayBand::Short).is_none());
    }

    #[test]
    fn zero_negative_and_nan_are_excluded_not_clamped() {
        for v in [0.0, -1e-7, f64::NAN, f64::INFINITY] {
            assert!(
                classify(v, XrayBand::Long).is_none(),
                "value {v} must be excluded"
            );
            if !v.is_finite() || v <= 0.0 {
                assert!(!plottable_on_log_axis(v));
            }
        }
    }

    #[test]
    fn provider_energy_strings_map_without_guessing() {
        assert_eq!(
            XrayBand::from_provider_energy("0.1-0.8nm"),
            Some(XrayBand::Long)
        );
        assert_eq!(
            XrayBand::from_provider_energy("0.05-0.4nm"),
            Some(XrayBand::Short)
        );
        assert_eq!(XrayBand::from_provider_energy("0.1-0.9nm"), None);
    }
}
