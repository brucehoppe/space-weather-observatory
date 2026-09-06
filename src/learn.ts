/** The Learn view: three lessons over the frozen demonstration dataset, plus
 *  the labelled synthetic alert scenarios (spec §9, §13B). */
import { button, el } from "./dom";
import { fmtDuration, fmtUtc } from "./format";
import * as ipc from "./ipc";
import type { AppState, ViewName } from "./state";
import type { AlertScenario, Evaluation, Lesson } from "./types";

export interface LearnCallbacks {
  onStartLesson: (lesson: Lesson) => void;
  onLessonStep: (lesson: Lesson, step: number) => void;
  onResetLesson: () => void;
  onNavigate: (view: ViewName) => void;
}

export function renderLearnView(state: AppState, cb: LearnCallbacks): HTMLElement {
  const view = el("div", { class: "view" });
  view.append(
    el("h1", {}, "Learn"),
    el("p", {},
      "Each lesson runs against a frozen dataset of real NOAA SWPC products captured on 6 September 2026. " +
      "Nothing in it has been altered to make a point; where the honest answer is “nothing much happened”, " +
      "the lesson says so."),
  );

  if (state.dashboard && state.dashboard.mode !== "demo") {
    view.append(el("p", { class: "note warn-text" },
      "Lessons use the frozen dataset. Starting one switches the observatory into demonstration mode; " +
      "live monitoring and your alert history are unaffected."));
  }

  for (const lesson of state.lessons) {
    view.append(renderLesson(lesson, state, cb));
  }

  view.append(el("h2", {}, "Solar-wind alert: worked scenarios"));
  view.append(el("p", {},
    "The captured interval is geomagnetically quiet, so it contains no threshold crossing. These scenarios " +
    "are explicitly synthetic and are run through the same evaluator the live detector uses. They never " +
    "touch live episode history."));
  view.append(renderScenarios());
  return view;
}

function renderLesson(lesson: Lesson, state: AppState, cb: LearnCallbacks): HTMLElement {
  const active = state.activeLesson?.id === lesson.id;
  const currentStep = active ? state.activeLesson!.step : -1;

  const steps = el("ol", { class: "steps" });
  lesson.steps.forEach((step, i) => {
    const li = el("li", { class: i === currentStep ? "current" : "" }, step.prompt);
    if (active && i === currentStep) {
      li.append(el("div", { class: "note", style: "margin-top:.3rem" },
        step.focus_series ? `Series: ${step.focus_series}` : "",
        step.select_time ? ` · Time: ${fmtUtc(step.select_time)}` : "",
        ` · View: ${step.view}`));
    }
    steps.append(li);
  });

  const controls = el("div", { style: "display:flex;gap:.4rem;flex-wrap:wrap;margin-top:.4rem" });
  if (!active) {
    controls.append(button("Start lesson", () => cb.onStartLesson(lesson), "primary"));
  } else {
    controls.append(
      button("Previous", () => cb.onLessonStep(lesson, Math.max(0, currentStep - 1)), "ghost"),
      button(
        currentStep >= lesson.steps.length - 1 ? "Finish" : "Next",
        () => {
          if (currentStep >= lesson.steps.length - 1) cb.onResetLesson();
          else cb.onLessonStep(lesson, currentStep + 1);
        },
        "primary",
      ),
      button("Reset and return", cb.onResetLesson, "ghost"),
    );
  }

  const sources = el("ul", { class: "note", style: "margin:.4rem 0 0;padding-left:1.1rem" });
  for (const src of lesson.sources) {
    const link = el("a", { href: "#", tabindex: "0" }, src);
    link.addEventListener("click", (e) => {
      e.preventDefault();
      void ipc.openExternal(src);
    });
    sources.append(el("li", {}, link));
  }

  return el("article", { class: "lesson" },
    el("h2", { style: "margin-top:0" }, lesson.title),
    el("p", { style: "margin:.2rem 0" }, lesson.question),
    steps,
    active ? el("p", {}, lesson.explanation) : el("details", {}, el("summary", {}, "What this shows"), el("p", {}, lesson.explanation)),
    sources,
    el("p", { class: "note" }, lesson.attribution),
    controls,
  );
}

function renderScenarios(): HTMLElement {
  const container = el("div", {});
  void ipc.getAlertScenarios().then((scenarios) => {
    for (const s of scenarios) container.append(scenarioCard(s));
  }).catch((e: unknown) => {
    container.append(el("p", { class: "note" }, `Scenarios unavailable: ${String(e)}`));
  });
  return container;
}

function scenarioCard(scenario: AlertScenario): HTMLElement {
  const output = el("div", { class: "note", style: "margin-top:.4rem" });
  const card = el("article", { class: "lesson" },
    el("h3", { style: "margin:0" }, scenario.title),
    el("p", { class: "note", style: "margin:.2rem 0" },
      `Synthetic data · ${scenario.samples.length} samples · expected outcome: ${scenario.expectation}`),
    output,
    button("Run through the detector", async () => {
      output.replaceChildren(el("span", {}, "Running…"));
      try {
        const steps = await ipc.evaluateScenario(scenario.id);
        output.replaceChildren(scenarioResult(steps));
      } catch (e) {
        output.replaceChildren(el("span", { class: "warn-text" }, String(e)));
      }
    }, "ghost"),
  );
  return card;
}

function scenarioResult(steps: Evaluation[]): HTMLElement {
  const table = el("table", { class: "table" },
    el("tr", {}, el("th", {}, "Step"), el("th", {}, "State"), el("th", {}, "Detail")));
  steps.forEach((e, i) => {
    const s = e.state;
    let detail = "";
    switch (s.state) {
      case "pending":
        detail = `${fmtDuration(s.elapsed_seconds)} of ${fmtDuration(s.required_seconds)}`;
        break;
      case "active":
        detail = `episode ${s.episode.id}, onset ${fmtUtc(s.episode.qualified_onset)}${e.newly_active ? " (announced once)" : ""}`;
        break;
      case "data_unavailable":
        detail = `${s.reason}${s.retained_episode ? ", previous episode retained" : ""}`;
        break;
      case "cleared":
        detail = `cleared at ${fmtUtc(s.episode.cleared_at)}`;
        break;
      default:
        detail = "";
    }
    table.append(el("tr", {}, el("td", {}, String(i + 1)), el("td", {}, s.state), el("td", {}, detail)));
  });
  const banners = steps.filter((e) => e.newly_active).length;
  return el("div", {},
    table,
    el("p", { class: "note" }, `Visible alerts emitted across the whole scenario: ${banners}.`),
  );
}
