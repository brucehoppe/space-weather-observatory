/** Sources & References, privacy, cache and attribution (spec §11, §12). */
import { button, el } from "./dom";
import { fmtBytes, fmtDuration, fmtUtc } from "./format";
import * as ipc from "./ipc";
import { MAP_CREDIT } from "./aurora";
import type { AppState } from "./state";

export function renderSourcesView(state: AppState): HTMLElement {
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
      wrap.append(el("figure", { style: "margin:0;max-width:320px" },
        el("img", { src: img.data_uri, alt: `${img.label} solar image`, style: "width:100%;border-radius:6px" }),
        el("figcaption", { class: "note" },
          `${img.label} · acquired ${fmtUtc(img.acquired_at)}`,
          el("div", {}, img.false_colour ? "False colour." : "", ` ${img.description}`),
          el("div", {}, img.credit)),
      ));
    }
    view.append(wrap);
  } else {
    view.append(el("p", { class: "note" }, state.imageryError ?? "Solar imagery has not been retrieved yet."));
  }

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
