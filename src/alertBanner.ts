/** The custom solar-wind alert banner and its settings dialog (spec §13B).
 *
 *  The banner is clearly separated from NOAA products: it never shows G/R/S
 *  labels, never invents a forecast, and its identity line always says whose
 *  alert it is. Dismissal is a presentation attribute; it cannot clear a
 *  condition or disable the detector. */
import { announce, button, el } from "./dom";
import { fmtAge, fmtDuration, fmtUtc, fmtWithUnit, NO_VALUE } from "./format";
import * as ipc from "./ipc";
import type { AlertSettings, AlertView, Dashboard, Episode, Settings } from "./types";

export interface BannerCallbacks {
  onDismiss: (episodeId: string) => void;
  onConfigure: () => void;
  onExplain: () => void;
  onShowDismissed: () => void;
}

/** Episodes the user dismissed stay discoverable rather than disappearing. */
let showDismissed = false;

export function setShowDismissed(value: boolean): void {
  showDismissed = value;
}

let lastAnnouncedEpisode: string | null = null;

export function renderAlertBanner(
  view: AlertView | null,
  dashboard: Dashboard | null,
  callbacks: BannerCallbacks,
  /** The dataset's own clock, so replay and demo ages are not measured against today. */
  now: Date = new Date(),
): HTMLElement {
  const container = el("section", {
    class: "alert-banner is-neutral",
    "aria-label": "Custom solar-wind alert",
  });

  if (!view) {
    container.append(
      el("div", {},
        el("div", { class: "alert-identity" }, "Custom solar-wind alert"),
        el("p", { class: "alert-headline" }, "Evaluating…"),
      ),
    );
    return container;
  }

  const { evaluation, settings, context } = view;
  const demo = context !== "live";
  const identity = demo ? "Demo: solar-wind alert" : "Custom solar-wind alert";
  const state = evaluation.state;

  let headline = "";
  let tone: "neutral" | "active" | "paused" = "neutral";
  let episode: Episode | null = null;

  switch (state.state) {
    case "disabled":
      headline = "Custom alert is turned off";
      break;
    case "monitoring":
      headline = `Monitoring: solar-wind speed is below your ${settings.entry_threshold_km_s} km/s threshold`;
      break;
    case "pending": {
      const remaining = state.required_seconds - state.elapsed_seconds;
      headline = `Above your threshold for ${fmtDuration(state.elapsed_seconds)} — ${fmtDuration(remaining)} more needed to qualify`;
      break;
    }
    case "active":
      headline = "Solar wind above your threshold";
      tone = "active";
      episode = state.episode;
      break;
    case "data_unavailable": {
      const reason =
        state.reason === "stale_feed"
          ? "the solar-wind feed is stale"
          : state.reason === "no_data"
            ? "no solar-wind data is available"
            : "there is insufficient recent data";
      headline = `Evaluation paused: ${reason}`;
      tone = "paused";
      episode = state.retained_episode;
      break;
    }
    case "cleared":
      headline = "Condition cleared";
      episode = state.episode;
      break;
  }

  container.className = `alert-banner ${tone === "active" ? "is-active" : tone === "paused" ? "is-paused" : "is-neutral"}`;

  // Announce a newly active episode once — never on each numerical refresh.
  if (evaluation.newly_active && episode && episode.id !== lastAnnouncedEpisode) {
    lastAnnouncedEpisode = episode.id;
    announce(`${identity}: ${headline}.`);
  }
  if (state.state === "monitoring") lastAnnouncedEpisode = null;

  const dismissed = episode?.acknowledged === true;
  const body = el("div", {});
  body.append(el("div", { class: "alert-identity" }, identity));

  if (dismissed && !showDismissed) {
    body.append(
      el("p", { class: "alert-headline" }, "A dismissed alert condition is still active"),
      el("p", { class: "alert-meaning" },
        "You dismissed the banner for this episode. The detector is still running and the condition has not cleared."),
    );
    const actions = el("div", { class: "alert-actions" },
      button("Show details", () => { showDismissed = true; callbacks.onShowDismissed(); }, "ghost"),
    );
    container.append(body, actions);
    return container;
  }

  body.append(el("p", { class: "alert-headline" }, headline));
  body.append(renderEvidence(view, episode, dashboard, now));

  if (state.state === "active") {
    body.append(
      el("p", { class: "alert-meaning" },
        "Faster solar wind can contribute to geomagnetic activity, but magnetic orientation and persistence also matter. " +
        "This is a measurement condition you configured — it is not a NOAA warning and does not predict a storm. " +
        "Check the official outlook for expected conditions."),
    );
  }
  if (demo) {
    body.append(
      el("p", { class: "alert-meaning warn-text" },
        context === "demo"
          ? "Demonstration dataset. These readings are frozen historical values, not current conditions."
          : "Replay. These readings are historical; live monitoring and today's forecast are unaffected."),
    );
  }

  const actions = el("div", { class: "alert-actions" });
  actions.append(button("What does this mean?", callbacks.onExplain, "ghost"));
  actions.append(button("Configure", callbacks.onConfigure, "ghost"));
  if (episode && !dismissed && state.state === "active") {
    actions.append(button("Dismiss", () => callbacks.onDismiss(episode!.id), "ghost"));
  }
  if (dismissed) {
    actions.append(button("Hide details", () => { showDismissed = false; callbacks.onShowDismissed(); }, "ghost"));
  }

  container.append(body, actions);
  return container;
}

function renderEvidence(view: AlertView, episode: Episode | null, dashboard: Dashboard | null, now: Date): HTMLElement {
  const { evaluation, settings } = view;
  const list = el("div", { class: "alert-evidence" });

  const speedSeries = dashboard?.series["noaa-swpc:rtsw_wind_1m:proton_speed"];
  const lastSpeed = [...(speedSeries?.samples ?? [])].reverse().find((s) => s.value !== null && s.quality !== "missing");

  const item = (label: string, value: string, extra?: string) =>
    el("span", {}, `${label} `, el("b", {}, value), extra ? ` ${extra}` : "");

  list.append(item("Latest accepted speed:", lastSpeed?.value != null ? fmtWithUnit(lastSpeed.value, "km/s") : NO_VALUE));
  list.append(item("Your threshold:", fmtWithUnit(settings.entry_threshold_km_s, "km/s")));
  list.append(item("Persistence required:", fmtDuration(settings.persistence_minutes * 60)));
  list.append(item("Observation time:", fmtUtc(lastSpeed?.time)));
  list.append(item("Feed age:", fmtAge(lastSpeed?.time, now)));

  // Bz carries its own time and frame, or is explicitly marked unavailable.
  if (view.bz_gsm_nt !== null && view.bz_time) {
    list.append(item("Bz (GSM):", fmtWithUnit(view.bz_gsm_nt, "nT"), `at ${fmtUtc(view.bz_time)}`));
  } else {
    list.append(item("Bz (GSM):", view.bz_stale ? "stale" : "unavailable"));
  }

  list.append(item("Source:", `${view.source_product}${view.source_spacecraft ? ` · ${view.source_spacecraft}` : ""}`));

  if (episode) {
    list.append(item("Episode began:", fmtUtc(episode.qualified_onset)));
    list.append(item("Peak in episode:", fmtWithUnit(episode.peak_speed_km_s, "km/s")));
    if (episode.cleared_at) list.append(item("Cleared:", fmtUtc(episode.cleared_at)));
  }

  const coverage = evaluation.coverage;
  if (coverage && !coverage.adequate) {
    list.append(item("Coverage:",
      `${coverage.accepted_samples}/${coverage.expected_samples} samples`,
      `largest gap ${fmtDuration(coverage.largest_gap_seconds)}`));
  }

  return list;
}

/** Configuration dialog. Values are validated by the backend; invalid input is
 *  reported rather than silently coerced. */
export function openAlertSettings(settings: Settings, onSaved: (s: Settings) => void): void {
  const dialog = el("dialog", { "aria-label": "Custom solar-wind alert settings" });
  const error = el("p", { class: "form-error" });

  const enabled = el("input", { type: "checkbox", id: "alert-enabled" }) as HTMLInputElement;
  enabled.checked = settings.alert.enabled;

  const num = (id: string, value: number, step: string) => {
    const input = el("input", { type: "number", id, step, value: String(value) }) as HTMLInputElement;
    return input;
  };
  const threshold = num("alert-threshold", settings.alert.entry_threshold_km_s, "1");
  const persistence = num("alert-persistence", settings.alert.persistence_minutes, "1");
  const hysteresis = num("alert-hysteresis", settings.alert.hysteresis_km_s, "1");
  const clearance = num("alert-clearance", settings.alert.clearance_minutes, "1");

  const field = (labelText: string, control: HTMLElement, hint: string) =>
    el("div", { class: "field" },
      el("label", { for: control.id }, labelText),
      control,
      el("span", { class: "hint" }, hint));

  const body = el("div", { class: "dialog-body" },
    el("h2", {}, "Custom solar-wind alert"),
    el("p", { class: "note" },
      "These are application settings for your own measurement alert. They are not NOAA alert criteria " +
      "and not physical impact thresholds. Evaluation runs only while this application is open."),
    field("Enabled", enabled, "Turning this off stops evaluation. Past episodes are kept."),
    field("Entry threshold (km/s)", threshold, "The alert qualifies at or above this speed. Default 500."),
    field("Persistence (minutes)", persistence, "How long the speed must stay at or above the threshold. Default 10."),
    field("Hysteresis (km/s)", hysteresis, "Clearing needs speed below threshold minus this margin. Default 25."),
    field("Clearance (minutes)", clearance, "How long below the clearing bound before the episode clears. Default 5."),
    error,
  );

  const actions = el("div", { class: "actions" });
  const close = () => { dialog.close(); dialog.remove(); };

  actions.append(
    button("Restore defaults", async () => {
      try {
        const saved = await ipc.resetAlertSettings();
        onSaved(saved);
        close();
      } catch (e) {
        error.textContent = String(e);
      }
    }, "ghost"),
    button("Cancel", close, "ghost"),
    button("Save", async () => {
      const next: Settings = {
        ...settings,
        alert: {
          ...settings.alert,
          enabled: enabled.checked,
          entry_threshold_km_s: Number(threshold.value),
          persistence_minutes: Number(persistence.value),
          hysteresis_km_s: Number(hysteresis.value),
          clearance_minutes: Number(clearance.value),
        } satisfies AlertSettings,
      };
      try {
        const saved = await ipc.saveSettings(next);
        onSaved(saved);
        close();
      } catch (e) {
        error.textContent = String(e);
      }
    }, "primary"),
  );

  dialog.append(body, actions);
  document.body.append(dialog);
  dialog.showModal();
  threshold.focus();
}

/** "What does this mean?" — sourced explanation, no invented forecast. */
export function openAlertExplanation(settings: AlertSettings): void {
  const dialog = el("dialog", { "aria-label": "What the solar-wind alert means" });
  const close = () => { dialog.close(); dialog.remove(); };
  dialog.append(
    el("div", { class: "dialog-body" },
      el("h2", {}, "What this alert means"),
      el("p", {},
        `This alert says one thing: solar-wind speed measured upstream of Earth stayed at or above ` +
        `${settings.entry_threshold_km_s} km/s — a value you chose — for at least ` +
        `${settings.persistence_minutes} minutes, with enough valid data to be sure.`),
      el("p", {},
        "Faster solar wind can contribute to geomagnetic activity. Whether it does also depends on the " +
        "orientation of the interplanetary magnetic field (particularly a sustained southward Bz) and on " +
        "how long those conditions persist. Speed alone is not an impact score."),
      el("p", {},
        "This is not a NOAA warning. It produces no G, R or S level, no outage prediction, no personal risk " +
        "score and no confidence percentage. For expected conditions, read the official NOAA outlook shown " +
        "in the side panel, which carries its own issuance time and validity period."),
      el("p", { class: "note" },
        "Aurora prospects at your location depend on more than this reading: darkness, cloud cover, your " +
        "latitude relative to the auroral oval, and local viewing conditions. This application does not have " +
        "those inputs and does not estimate them."),
    ),
    el("div", { class: "actions" }, button("Close", close, "primary")),
  );
  document.body.append(dialog);
  dialog.showModal();
}
