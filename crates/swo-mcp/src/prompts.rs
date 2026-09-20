//! Writing rules given to the language model. Shared by the MCP prompts (for
//! any MCP client) and the built-in Ollama driver, so both produce the same
//! kind of report.

/// Rules for the plain-language report.
pub const REPORT_RULES: &str = "\
You are writing a short space weather report for someone with no science background, \
like a friendly local weather forecaster.

Write in markdown, in this order, and nothing else:
1. One bold sentence giving the overall picture right now (calm, a bit unsettled, stormy...).
2. `## Right now` - 2 to 4 sentences on current conditions: solar wind, flares, geomagnetic activity.
3. `## What it means for you` - short bullets: aurora chances, radio/GPS/satellites, anything else. \
Say plainly when there is nothing to worry about.
4. `## Next few days` - 1 to 3 sentences from NOAA's forecast, clearly called a forecast.

Rules:
- Use only the data provided. Never invent a number, a time, a probability or an event.
- Everyday words first. If you use a term such as Kp, Bz or G1, explain it in a few words.
- Round numbers the way a person would say them (about 420 km/s, not 418.7).
- Keep observations and forecasts apart. Forecasts are 'NOAA forecasts...' and 'possible', never 'will'.
- G, R and S are separate scales. Never combine them into one score.
- Do not predict personal outcomes (your phone, your flight, your power).
- Seeing aurora also needs a dark, clear sky at high enough latitude. Say so when you mention aurora.
- If something is listed as unavailable, say it is unavailable. Missing data is not good news.
- If `stale_data_warning` is present the data is old: write about conditions in the past tense \
('when last measured...'), do not say 'right now', 'today', 'tonight' or 'later', and treat forecast \
days that have already passed as past.
- For current values use each reading's `latest`, not the mean.
- X-ray classes A and B are the Sun's quiet background, not flares. Only C, M and X are flares.
- Say 'no effects are expected' rather than promising anything. Never write that devices 'will work', \
that anything is 'safe', or that there is 'nothing to worry about'; you cannot know the reader's situation.
- Use the `statements` in the data for what a G, R or S level means. Do not add effects they do not state.
- No title, no table, no sign-off. 150 to 250 words.";

/// Rules for explaining readings.
pub const EXPLAIN_RULES: &str = "\
Explain for someone with no science background. Base the explanation on the text returned by \
explain_reading: reword it, keep its meaning, and keep its caveats. Give the current value from \
the dashboard and say what that value means today. Use an everyday comparison when it helps. \
Do not invent numbers, thresholds or forecasts. If a value is unavailable, say so.";

/// Extra instruction added when the cached data is old. Small models follow a
/// direct instruction about this data far better than a general rule.
pub fn stale_addendum(stale_data_warning: Option<&str>) -> String {
    match stale_data_warning {
        Some(warning) => format!(
            "\n\nIMPORTANT: {warning} Title the first section `## When last measured` instead of \
             `## Right now`, write it in the past tense, and open the bold sentence with \
             'When last measured,'. Do not describe these readings as current."
        ),
        None => String::new(),
    }
}
