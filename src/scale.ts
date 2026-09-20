/** UI scale for large displays.
 *
 *  Layout, text and canvas drawing are all specified for a ~1600x900 window.
 *  On a bigger window everything grows by the same factor, so a 27-inch
 *  monitor shows a larger, sharper interface rather than the same tiny one
 *  with more empty space. Below the reference size nothing shrinks: the
 *  narrow-window layouts handle that.
 *
 *  CSS reads the factor as `--ui-scale` (through the root font size, so sizes
 *  are written in `rem`); canvas code calls `uiScale()` and draws in
 *  unscaled design units under a scaled transform. */

const REFERENCE_WIDTH = 1600;
const REFERENCE_HEIGHT = 900;
const MAX_SCALE = 2;

let current = 1;

/** Factor for a window of this size. Whichever dimension is tighter decides,
 *  so a wide-but-short window does not overflow vertically. */
export function scaleFor(width: number, height: number): number {
  const raw = Math.min(width / REFERENCE_WIDTH, height / REFERENCE_HEIGHT);
  if (!Number.isFinite(raw)) return 1;
  // Steps of 1/20 avoid re-laying-out on every pixel of a window drag.
  return Math.round(Math.min(MAX_SCALE, Math.max(1, raw)) * 20) / 20;
}

export function uiScale(): number {
  return current;
}

/** Apply the scale to the page now and whenever the window changes size. */
export function initUiScale(onChange: () => void = () => {}): void {
  const apply = (): void => {
    const next = scaleFor(window.innerWidth, window.innerHeight);
    if (next === current && document.documentElement.style.getPropertyValue("--ui-scale")) return;
    current = next;
    document.documentElement.style.setProperty("--ui-scale", String(next));
    onChange();
  };
  apply();
  window.addEventListener("resize", apply);
}
