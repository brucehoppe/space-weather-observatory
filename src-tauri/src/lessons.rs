//! The three lessons (spec §9).
//!
//! Every lesson runs against the frozen demonstration dataset — real NOAA SWPC
//! products captured on 6 September 2026 — and is written around what that
//! data actually shows. No storm is invented and no measurement is adjusted to
//! make a point; where the real answer is "nothing much happened", the lesson
//! says so, because that is itself the thing worth learning.

use chrono::{DateTime, TimeZone, Utc};

use crate::commands::{Lesson, LessonStep};
use crate::demo;

fn at(h: u32, m: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 9, 6, h, m, 0).unwrap()
}

const SPEED: &str = "noaa-swpc:rtsw_wind_1m:proton_speed";
const BZ: &str = "noaa-swpc:rtsw_mag_1m:bz_gsm";
const XRAY: &str = "noaa-swpc:goes_xrays_1day:xray_flux_long";
const KP: &str = "noaa-swpc:planetary_k_index_forecast:kp_estimated";

pub fn lessons() -> Vec<Lesson> {
    vec![
        Lesson {
            id: "wind-and-orientation".into(),
            title: "Speed and orientation, read together".into(),
            question: "The solar wind is always blowing. What makes some of it matter?".into(),
            steps: vec![
                LessonStep {
                    prompt: "Look at the solar-wind speed panel. Across this whole day it sits between roughly 326 and 381 km/s — ordinary, steady solar wind.".into(),
                    focus_series: Some(SPEED.into()),
                    select_time: Some(at(6, 0)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "Now look at Bz in the GSM frame. It swings either side of zero, reaching about -6 nT. Negative means southward — the orientation that can couple with Earth's field.".into(),
                    focus_series: Some(BZ.into()),
                    select_time: Some(at(6, 0)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "Pin a time where Bz is most negative and read the speed at the same instant. Both values come from the same upstream spacecraft, at the same minute.".into(),
                    focus_series: Some(BZ.into()),
                    select_time: Some(at(12, 0)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "Finally, check the Kp panel for the hours that follow. It stays at or below about 3.67 — below NOAA storm levels.".into(),
                    focus_series: Some(KP.into()),
                    select_time: Some(at(15, 0)),
                    view: "observatory".into(),
                },
            ],
            explanation: "Speed alone is not an impact score, and a southward Bz alone is not a storm. \
Geomagnetic activity depends on speed and orientation *and* on how long that orientation persists. \
In this real interval the wind was slow, Bz wandered south only briefly, and no storm followed. \
That is the ordinary case, and recognising it is what makes the unusual case legible."
                .into(),
            sources: vec![
                "https://www.swpc.noaa.gov/products/real-time-solar-wind".into(),
                "https://www.swpc.noaa.gov/products/planetary-k-index".into(),
                "https://www.spaceweather.gov/phenomena/geomagnetic-storms".into(),
            ],
            dataset: "frozen-2026-09-06".into(),
            attribution: demo::ATTRIBUTION.into(),
        },
        Lesson {
            id: "flare-wind-response".into(),
            title: "A flare, the wind, and what came after".into(),
            question: "A flare happened. Does a geomagnetic storm follow?".into(),
            steps: vec![
                LessonStep {
                    prompt: "Open the GOES X-ray panel. The largest long-band peak of this day is at 10:42 UTC, about 5.1 x 10^-6 W/m^2 — a C5.1 flux level on the 0.1-0.8 nm passband.".into(),
                    focus_series: Some(XRAY.into()),
                    select_time: Some(at(10, 42)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "X-rays travel at the speed of light: this is the flare being seen, about eight minutes after it happened. It is not a measurement of anything arriving at Earth later.".into(),
                    focus_series: Some(XRAY.into()),
                    select_time: Some(at(10, 42)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "Now look at the solar wind in the hours after 10:42. Speed stays near 350 km/s. Nothing in the upstream measurements changes in step with the flare.".into(),
                    focus_series: Some(SPEED.into()),
                    select_time: Some(at(13, 0)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "And the geomagnetic response: Kp for the intervals after the flare stays below storm levels. Check the official bulletins panel too — the G1 watch in effect is for 08 September, from CMEs on 05 September, not from this flare.".into(),
                    focus_series: Some(KP.into()),
                    select_time: Some(at(15, 0)),
                    view: "observatory".into(),
                },
            ],
            explanation: "Three different quantities, three different clocks. The X-ray flux is light from the flare, \
arriving in about eight minutes. The upstream solar wind is measured by a spacecraft roughly a million miles \
ahead of Earth, so it reports material that will reach Earth some tens of minutes later. The geomagnetic \
response is measured at the ground, later still. Aligning them in time is useful; concluding that one caused \
another because they are near each other on a chart is not. In this real interval a C-class flare occurred and \
no geomagnetic storm followed — an X-ray spike is not evidence of an Earth-directed CME."
                .into(),
            sources: vec![
                "https://www.swpc.noaa.gov/products/goes-x-ray-flux".into(),
                "https://www.swpc.noaa.gov/products/real-time-solar-wind".into(),
                "https://www.spaceweather.gov/products/alerts-watches-and-warnings".into(),
            ],
            dataset: "frozen-2026-09-06".into(),
            attribution: demo::ATTRIBUTION.into(),
        },
        Lesson {
            id: "observation-versus-forecast".into(),
            title: "What the aurora forecast actually says".into(),
            question: "The app shows an aurora map. Is that where the aurora is?".into(),
            steps: vec![
                LessonStep {
                    prompt: "Open the Aurora view. Read the two times in the header: the observation time the model was run from, and the forecast time it applies to. They are different, and the gap is whatever this issue states — not a fixed 30 minutes.".into(),
                    focus_series: None,
                    select_time: None,
                    view: "aurora".into(),
                },
                LessonStep {
                    prompt: "The colour scale is the OVATION model's probability of visible aurora in each 1-degree grid cell. It is a property of the cell, not of you: it does not know whether it is dark, or cloudy, where you are.".into(),
                    focus_series: None,
                    select_time: None,
                    view: "aurora".into(),
                },
                LessonStep {
                    prompt: "Compare with the observations panel: Kp is an index computed from ground magnetometers over three-hour intervals. It is a different quantity, on a different clock, from a different instrument network.".into(),
                    focus_series: Some(KP.into()),
                    select_time: Some(at(15, 0)),
                    view: "observatory".into(),
                },
                LessonStep {
                    prompt: "Finally open Sources. Every number you just read has a product page, a machine-readable endpoint and a verification date behind it.".into(),
                    focus_series: None,
                    select_time: None,
                    view: "sources".into(),
                },
            ],
            explanation: "A forecast is a statement about the future made at a stated time, and it carries its own \
validity. An observation is a measurement of something that already happened. The aurora map is a model \
forecast; Kp is an index derived from observations; neither is a personal viewing probability. Whether you \
would actually see aurora also depends on darkness, cloud and how far you are from the model's oval — inputs \
this application does not have and therefore does not pretend to."
                .into(),
            sources: vec![
                "https://www.spaceweather.gov/products/aurora-30-minute-forecast".into(),
                "https://www.swpc.noaa.gov/products/planetary-k-index".into(),
            ],
            dataset: "frozen-2026-09-06".into(),
            attribution: demo::ATTRIBUTION.into(),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::{assemble, Mode};

    #[test]
    fn there_are_exactly_three_complete_lessons() {
        let l = lessons();
        assert_eq!(l.len(), 3);
        for lesson in &l {
            assert!(!lesson.question.is_empty());
            assert!(lesson.steps.len() >= 3, "{} has too few steps", lesson.id);
            assert!(
                lesson.explanation.len() > 200,
                "{} needs a real explanation",
                lesson.id
            );
            assert!(!lesson.sources.is_empty());
            assert!(lesson.sources.iter().all(|s| s.starts_with("https://")));
            assert!(!lesson.attribution.is_empty());
        }
    }

    #[test]
    fn every_lesson_step_refers_to_a_series_that_actually_exists() {
        let d = assemble(
            Mode::Demo,
            &demo::payloads(),
            demo::captured_at(),
            "demo".into(),
        );
        for lesson in lessons() {
            for step in lesson.steps {
                if let Some(key) = step.focus_series {
                    assert!(
                        d.series.contains_key(&key),
                        "lesson {} references missing series {key}",
                        lesson.id
                    );
                }
                assert!(
                    ["observatory", "aurora", "sources"].contains(&step.view.as_str()),
                    "unknown view {}",
                    step.view
                );
            }
        }
    }

    #[test]
    fn every_selected_time_lies_inside_the_frozen_dataset() {
        let d = assemble(
            Mode::Demo,
            &demo::payloads(),
            demo::captured_at(),
            "demo".into(),
        );
        let speed = &d.series[SPEED];
        let first = speed.samples.first().unwrap().time;
        let last = speed.samples.last().unwrap().time;
        for lesson in lessons() {
            for step in lesson.steps {
                if let Some(t) = step.select_time {
                    assert!(
                        t >= first && t <= last,
                        "lesson {} selects {t}, outside {first}..{last}",
                        lesson.id
                    );
                }
            }
        }
    }

    #[test]
    fn the_flare_lesson_matches_the_real_peak_in_the_data() {
        let d = assemble(
            Mode::Demo,
            &demo::payloads(),
            demo::captured_at(),
            "demo".into(),
        );
        let long = &d.series[XRAY];
        let peak = long
            .samples
            .iter()
            .filter(|o| o.value.is_some())
            .max_by(|a, b| a.value.partial_cmp(&b.value).unwrap())
            .unwrap();
        assert_eq!(
            peak.time,
            at(10, 42),
            "the lesson quotes the real peak time"
        );
        let class = swo_core::flare::classify(peak.value.unwrap(), swo_core::flare::XrayBand::Long)
            .unwrap();
        assert_eq!(
            class.format(),
            "C5.1",
            "the lesson quotes the real flux class"
        );
    }

    #[test]
    fn the_first_lesson_matches_the_real_speed_range_in_the_data() {
        let d = assemble(
            Mode::Demo,
            &demo::payloads(),
            demo::captured_at(),
            "demo".into(),
        );
        let speed = &d.series[SPEED];
        let values: Vec<f64> = speed.samples.iter().filter_map(|o| o.value).collect();
        let min = values.iter().cloned().fold(f64::MAX, f64::min);
        let max = values.iter().cloned().fold(f64::MIN, f64::max);
        assert_eq!(min.round(), 326.0);
        assert_eq!(max.round(), 381.0);
    }

    #[test]
    fn the_second_lesson_claim_that_no_storm_followed_is_true_in_the_data() {
        let d = assemble(
            Mode::Demo,
            &demo::payloads(),
            demo::captured_at(),
            "demo".into(),
        );
        let kp = &d.series[KP];
        let max = kp
            .samples
            .iter()
            .filter_map(|o| o.value)
            .fold(f64::MIN, f64::max);
        assert!(
            max < 5.0,
            "Kp reached {max}; the lesson text would need revising"
        );
    }
}
