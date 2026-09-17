/** Sources & References, privacy, cache and attribution (spec §11, §12). */
import { button, el } from "./dom";
import { fmtBytes, fmtDuration, fmtNumber, fmtUtc } from "./format";
import * as ipc from "./ipc";
import { MAP_CREDIT } from "./aurora";
import type { AppState, SunPassband } from "./state";

export interface SourcesCallbacks {
  onLoadSequence: (passband: SunPassband) => void;
  onTogglePlayback: (passband: SunPassband) => void;
  onStepFrame: (passband: SunPassband, delta: number) => void;
}

export function renderSourcesView(state: AppState, cb: SourcesCallbacks): HTMLElement {
  const view = el("div", { class: "view" });
  view.append(
    el("h1", {}, "Sources & references"),
    el("p", {},
      "Every reading in this application comes from one of the products below. Each row shows the exact " +
      "machine-readable endpoint the application requests, the official product page it was reached from, " +
      "and how often this application polls it."),
  );

  const table = el("table", { class: "table" },
    el("tr", {},
      el("th", {}, "Product"),
      el("th", {}, "Endpoint"),
      el("th", {}, "Poll cadence"),
      el("th", {}, "Description")));
  for (const s of state.sources) {
    const pageLink = s.page ? linkTo(s.page, "product page") : null;
    table.append(el("tr", {},
      el("td", {}, s.product, pageLink ? el("div", {}, pageLink) : null),
      el("td", {}, el("code", {}, s.url)),
      el("td", {}, fmtDuration(s.cadence_seconds)),
      el("td", {}, s.description)));
  }
  view.append(table);

  view.append(el("h2", {}, "Solar imagery"));
  if (state.sunImages.length) {
    const wrap = el("div", { style: "display:flex;gap:.75rem;flex-wrap:wrap" });
    for (const img of state.sunImages) {
      const seq = state.sunSequences[img.passband];
      const shownFrame = seq && seq.frames.length ? seq.frames[seq.index] : undefined;
      const shown = shownFrame ?? img;
      wrap.append(el("figure", { style: "margin:0;max-width:320px" },
        el("img", {
          src: shown.data_uri,
          alt: `${shown.label} solar image`,
          style: "width:100%;border-radius:6px;transition:opacity .25s ease",
          "data-sun-frame-img": img.passband,
        }),
        el("figcaption", { class: "note" },
          `${shown.label} · acquired `,
          el("span", { "data-sun-frame-time": img.passband }, fmtUtc(shown.acquired_at)),
          el("div", {}, shown.false_colour ? "False colour." : "", ` ${shown.description}`),
          el("div", {}, shown.credit)),
        renderSequenceControls(state, cb, img.passband),
      ));
    }
    view.append(wrap);
  } else {
    view.append(el("p", { class: "note" }, state.imageryError ?? "Solar imagery has not been retrieved yet."));
  }

  view.append(renderSolarCycle(state));

  view.append(el("h2", {}, "Scientific methods"));
  view.append(el("ul", {},
    li("Flux classification uses the 0.1–0.8 nm long passband only, with the published A/B/C/M/X decade boundaries. The 0.05–0.4 nm short band is plotted but never classified."),
    li("Bz is shown in the GSM frame with a zero reference, and the GSE value is retained separately. The two frames are never treated as interchangeable."),
    li("Kp is an interval-valued index over 3-hour UT intervals. NOAA estimates, observations and forecasts are kept visually and structurally distinct."),
    li("Cross-series lookup uses an explicit tolerance of one nominal cadence. Where no sample falls inside it, the application says “no nearby sample” rather than showing an unrelated measurement."),
    li("Chart lines break across data gaps and missing samples. Aggregation for display uses min–max decimation, which preserves peaks; aggregated series are labelled as such and raw values remain available for export."),
    li("Plain-language statements come from a reviewed, versioned rule layer. No language model runs at any point."),
  ));

  view.append(el("h2", {}, "Map and data credits"));
  view.append(el("ul", {},
    li("NOAA Space Weather Prediction Center: all space-weather measurements, indices, scales, bulletins and forecasts. U.S. Government work, not subject to domestic copyright."),
    li("NASA/SDO and the AIA, EVE and HMI science teams, served through the Helioviewer Project: solar imagery."),
    li(MAP_CREDIT),
  ));

  view.append(el("h2", {}, "Privacy"));
  view.append(el("ul", {},
    li("The application contacts three hosts only: services.swpc.noaa.gov, api.helioviewer.org and sdo.gsfc.nasa.gov. Any other host is refused before a request is made."),
    li("No account, no sign-in, no analytics, no telemetry, no crash reporting, and no language-model calls at runtime."),
    li("Requests carry only a user-agent identifying the application and its version. Nothing about you is sent."),
    li("All data stays on this computer, in your per-user application directories, and is bounded by the cache limit below."),
  ));

  view.append(renderCacheSection());

  view.append(el("h2", {}, "Version"));
  view.append(el("p", { class: "note" },
    `Space Weather Observatory ${state.dashboard?.app_version ?? "—"} · data schema ${state.dashboard?.schema_version ?? "—"}`));
  return view;
}

function renderSolarCycle(state: AppState): HTMLElement {
  const section = el("section", {});
  section.append(el("h2", {}, "Solar Cycle 25 progression"));
  const d = state.dashboard;
  const observed = d?.solar_cycle_observed ?? [];
  const predicted = d?.solar_cycle_predicted ?? [];
  if (!observed.length && !predicted.length) {
    section.append(el("p", { class: "note" },
      "Solar cycle progression unavailable. That is a gap in information, not evidence the cycle has ended."));
    return section;
  }

  section.append(el("p", {},
    "The sunspot number and F10.7 radio flux over the current 11-year solar cycle, monthly, against the " +
    "official consensus prediction panel's stated expected range. This is context for the activity level " +
    "behind the numbers above, not a forecast this application makes itself."));

  const latest = observed.length ? observed[observed.length - 1] : undefined;
  if (latest) {
    const matchingPrediction = predicted.find((p) => p.month === latest.month);
    section.append(el("p", { class: "note" },
      `Latest observed month (${latest.month}): sunspot number ${fmtNumber(latest.ssn, 1)}` +
      (latest.observed_swpc_ssn !== null ? ` (NOAA's own provisional figure: ${fmtNumber(latest.observed_swpc_ssn, 1)})` : "") +
      (latest.f10_7 !== null ? `, F10.7 radio flux ${fmtNumber(latest.f10_7, 1)} sfu.` : "."),
      matchingPrediction
        ? ` The panel's prediction for the same month was ${fmtNumber(matchingPrediction.predicted_ssn, 1)}, ` +
          `expected range ${fmtNumber(matchingPrediction.low_ssn, 1)}–${fmtNumber(matchingPrediction.high_ssn, 1)}.`
        : ""));
  }

  if (observed.length) {
    section.append(el("h3", { style: "font-size:12.5px;margin:.5rem 0 .2rem" }, "Observed (most recent months)"));
    const obsTable = el("table", { class: "table" },
      el("tr", {}, el("th", {}, "Month"), el("th", {}, "SSN"), el("th", {}, "NOAA provisional SSN"), el("th", {}, "F10.7")));
    for (const m of observed.slice(-6)) {
      obsTable.append(el("tr", {},
        el("td", {}, m.month),
        el("td", {}, fmtNumber(m.ssn, 1)),
        el("td", {}, fmtNumber(m.observed_swpc_ssn, 1)),
        el("td", {}, fmtNumber(m.f10_7, 1))));
    }
    section.append(obsTable);
  }

  if (predicted.length) {
    section.append(el("h3", { style: "font-size:12.5px;margin:.5rem 0 .2rem" }, "Predicted (consensus panel, next months)"));
    const predTable = el("table", { class: "table" },
      el("tr", {}, el("th", {}, "Month"), el("th", {}, "Predicted SSN"), el("th", {}, "Expected range")));
    for (const m of predicted.slice(0, 6)) {
      predTable.append(el("tr", {},
        el("td", {}, m.month),
        el("td", {}, fmtNumber(m.predicted_ssn, 1)),
        el("td", {}, `${fmtNumber(m.low_ssn, 1)}–${fmtNumber(m.high_ssn, 1)}`)));
    }
    section.append(predTable);
  }

  section.append(el("p", { class: "note" },
    "See ", el("a", { href: "https://www.spaceweather.gov/products/solar-cycle-progression", target: "_blank", rel: "noreferrer" }, "NOAA's solar cycle progression product"), "."));
  return section;
}

function renderSequenceControls(state: AppState, cb: SourcesCallbacks, passband: SunPassband): HTMLElement {
  const seq = state.sunSequences[passband];
  const wrap = el("div", { class: "sequence-controls" });

  if (!seq) {
    wrap.append(button("Play sequence (last ~22 h)", () => cb.onLoadSequence(passband), "ghost"));
    return wrap;
  }
  if (seq.loading) {
    wrap.append(el("span", { class: "note" }, "Loading sequence…"));
    return wrap;
  }
  if (seq.error) {
    wrap.append(
      el("span", { class: "note warn-text" }, `Sequence unavailable: ${seq.error}`),
      button("Retry", () => cb.onLoadSequence(passband), "ghost"),
    );
    return wrap;
  }
  if (seq.frames.length < 2) {
    wrap.append(el("span", { class: "note" }, "Not enough distinct frames were returned to animate."));
    return wrap;
  }

  wrap.append(
    el("div", { style: "display:flex;gap:.3rem;align-items:center" },
      button(seq.playing ? "Pause" : "Play", () => cb.onTogglePlayback(passband), "ghost"),
      button("◀", () => cb.onStepFrame(passband, -1), "ghost"),
      el("span", { class: "note", "data-sun-frame-counter": passband }, `Frame ${seq.index + 1} / ${seq.frames.length}`),
      button("▶", () => cb.onStepFrame(passband, 1), "ghost"),
    ),
  );
  return wrap;
}

function li(text: string): HTMLElement {
  return el("li", {}, text);
}

function linkTo(url: string, label: string): HTMLElement {
  const a = el("a", { href: "#" }, label);
  a.addEventListener("click", (e) => {
    e.preventDefault();
    void ipc.openExternal(url);
  });
  return a;
}

function renderCacheSection(): HTMLElement {
  const section = el("section", {});
  section.append(el("h2", {}, "Local storage"));
  const body = el("div", { class: "note" }, "Reading cache status…");
  section.append(body);

  const refresh = () => {
    void ipc.cacheStatus().then((status) => {
      body.replaceChildren(
        el("p", {}, `${status.snapshot_count} stored source snapshots, ${fmtBytes(status.size_bytes)} of a ${fmtBytes(status.limit_bytes)} limit.`),
        el("p", {}, el("code", {}, status.data_dir)),
        el("p", {}, el("code", {}, status.config_dir)),
        el("div", { style: "display:flex;gap:.4rem" },
          button("Trim to newest snapshot per product", () => {
            void ipc.clearCache().then(refresh);
          }, "ghost")),
      );
    }).catch((e: unknown) => {
      body.replaceChildren(el("p", { class: "warn-text" }, String(e)));
    });
  };
  refresh();
  return section;
}
