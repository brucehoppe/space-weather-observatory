/** Application state and the single shared time selection.
 *
 *  One selected time and one interval are shared by every view (spec §5).
 *  Live monitoring time is deliberately *not* part of the selection: moving
 *  the crosshair inspects history and never retargets the alert detector. */
import type { AlertView, Dashboard, Lesson, Settings, SourceEntry, SunImage } from "./types";

export type ViewName = "observatory" | "aurora" | "learn" | "sources";

export interface Selection {
  /** Epoch milliseconds. */
  time: number;
  /** True once the user clicks or uses the keyboard; false while following now. */
  pinned: boolean;
}

export type SunPassband = "aia193" | "aia304";

/** One passband's loaded sequence, for the play/pause filmstrip in Sources.
 *  Playback never starts on its own — `playing` is only ever set true by an
 *  explicit user click, so this never becomes ambient motion. */
export interface SunSequenceState {
  frames: SunImage[];
  index: number;
  playing: boolean;
  loading: boolean;
  error: string | null;
}

export interface AppState {
  view: ViewName;
  dashboard: Dashboard | null;
  alert: AlertView | null;
  settings: Settings | null;
  sources: SourceEntry[];
  lessons: Lesson[];
  sunImages: SunImage[];
  imageryError: string | null;
  sunSequences: Partial<Record<SunPassband, SunSequenceState>>;
  selection: Selection;
  /** Displayed interval, epoch milliseconds. */
  range: { start: number; end: number };
  /** Paused means "stop following now"; acquisition continues in the backend. */
  paused: boolean;
  reducedMotion: boolean;
  sideCollapsed: boolean;
  sideWidth: number;
  meaningCollapsed: boolean;
  /** Series key shown in the detail panel. */
  focusSeries: string | null;
  activeLesson: { id: string; step: number } | null;
  error: string | null;
}

export const INTERVAL_PRESETS: { label: string; ms: number }[] = [
  { label: "1 h", ms: 3600_000 },
  { label: "6 h", ms: 6 * 3600_000 },
  { label: "24 h", ms: 24 * 3600_000 },
  { label: "3 d", ms: 3 * 86_400_000 },
  { label: "7 d", ms: 7 * 86_400_000 },
];

export function initialState(): AppState {
  const now = Date.now();
  return {
    view: "observatory",
    dashboard: null,
    alert: null,
    settings: null,
    sources: [],
    lessons: [],
    sunImages: [],
    imageryError: null,
    sunSequences: {},
    selection: { time: now, pinned: false },
    range: { start: now - 6 * 3600_000, end: now },
    paused: false,
    reducedMotion: window.matchMedia("(prefers-reduced-motion: reduce)").matches,
    sideCollapsed: false,
    sideWidth: 340,
    meaningCollapsed: false,
    focusSeries: null,
    activeLesson: null,
    error: null,
  };
}

type Listener = (state: AppState) => void;

export class Store {
  private state: AppState = initialState();
  private listeners: Listener[] = [];

  get(): AppState {
    return this.state;
  }

  set(patch: Partial<AppState>): void {
    this.state = { ...this.state, ...patch };
    for (const l of this.listeners) l(this.state);
  }

  subscribe(listener: Listener): void {
    this.listeners.push(listener);
  }
}

/** The dataset's own "now": the newest *instantaneous* observation.
 *
 *  Interval-valued series such as Kp are excluded deliberately. A Kp interval
 *  is stamped with its start, and the interval containing the present moment
 *  begins in the past — but a forecast interval begins in the future, and
 *  taking a maximum across them would place "now" ahead of any real
 *  measurement. The result is also capped at the assembly time so the dataset
 *  clock can never run ahead of the snapshot it came from. */
export function datasetNow(state: AppState): number {
  const d = state.dashboard;
  if (!d) return Date.now();
  const assembled = new Date(d.assembled_at).getTime();
  let latest = 0;
  for (const series of Object.values(d.series)) {
    for (let i = series.samples.length - 1; i >= 0; i--) {
      const s = series.samples[i]!;
      if (s.time_precision !== "instant") break;
      if (s.value === null || s.quality === "missing") continue;
      latest = Math.max(latest, new Date(s.time).getTime());
      break;
    }
  }
  if (!latest) return assembled;
  return Math.min(latest, assembled);
}
