/** Formatting helpers. Every number the UI prints goes through here so units,
 *  precision and missing-value wording stay consistent. */

/** Wording for a value that is not available. Never "0", never "quiet". */
export const NO_VALUE = "—";

export function fmtNumber(value: number | null | undefined, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return NO_VALUE;
  return value.toFixed(digits);
}

/** Value with its unit, e.g. `412.5 km/s`. */
export function fmtWithUnit(value: number | null | undefined, unit: string, digits = 1): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return NO_VALUE;
  return `${value.toFixed(digits)} ${unit}`;
}

/** Scientific notation for flux, e.g. `5.1 × 10⁻⁶ W/m²`. */
export function fmtFlux(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value) || value <= 0) return NO_VALUE;
  const exp = Math.floor(Math.log10(value));
  const mantissa = value / Math.pow(10, exp);
  return `${mantissa.toFixed(1)} × 10${superscript(exp)} W/m²`;
}

function superscript(n: number): string {
  const map: Record<string, string> = {
    "0": "⁰", "1": "¹", "2": "²", "3": "³", "4": "⁴",
    "5": "⁵", "6": "⁶", "7": "⁷", "8": "⁸", "9": "⁹", "-": "⁻",
  };
  return String(n).split("").map((c) => map[c] ?? c).join("");
}

/** X-ray class from long-band flux. Mirrors `swo_core::flare::classify`;
 *  the short band is deliberately never classified. */
export function fluxClass(value: number | null | undefined): string | null {
  if (value === null || value === undefined || !Number.isFinite(value) || value <= 0) return null;
  const floors: [string, number][] = [["X", 1e-4], ["M", 1e-5], ["C", 1e-6], ["B", 1e-7], ["A", 1e-8]];
  for (const [letter, floor] of floors) {
    if (value >= floor) return `${letter}${(value / floor).toFixed(1)}`;
  }
  return `A${(value / 1e-8).toFixed(1)}`;
}

/** UTC display, always explicit about the zone. */
export function fmtUtc(iso: string | null | undefined, withSeconds = false): string {
  if (!iso) return NO_VALUE;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return NO_VALUE;
  const pad = (n: number) => String(n).padStart(2, "0");
  const time = withSeconds
    ? `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())}`
    : `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}`;
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ${time} UTC`;
}

/** Local display in an explicit named zone. Falls back to UTC when the zone is
 *  unknown, and always names whichever zone was actually used. */
export function fmtInZone(iso: string | null | undefined, zone: string, withSeconds = false): string {
  if (!iso) return NO_VALUE;
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return NO_VALUE;
  if (zone === "UTC") return fmtUtc(iso, withSeconds);
  try {
    const parts = new Intl.DateTimeFormat("en-CA", {
      timeZone: zone,
      year: "numeric", month: "2-digit", day: "2-digit",
      hour: "2-digit", minute: "2-digit",
      ...(withSeconds ? { second: "2-digit" as const } : {}),
      hour12: false,
      timeZoneName: "short",
    }).format(d);
    return parts.replace(",", "");
  } catch {
    return `${fmtUtc(iso, withSeconds)} (unknown zone "${zone}", shown in UTC)`;
  }
}

/** Human age, e.g. `4 min ago`. Ages are always shown beside a reading. */
export function fmtAge(iso: string | null | undefined, now: Date = new Date()): string {
  if (!iso) return "no data";
  const t = new Date(iso).getTime();
  if (Number.isNaN(t)) return "no data";
  const seconds = Math.round((now.getTime() - t) / 1000);
  if (seconds < 0) return "in the future";
  if (seconds < 90) return `${seconds} s ago`;
  const minutes = Math.round(seconds / 60);
  if (minutes < 90) return `${minutes} min ago`;
  const hours = Math.round(minutes / 60);
  if (hours < 48) return `${hours} h ago`;
  return `${Math.round(hours / 24)} days ago`;
}

export function fmtDuration(seconds: number): string {
  if (!Number.isFinite(seconds)) return NO_VALUE;
  const s = Math.max(0, Math.round(seconds));
  if (s < 90) return `${s} s`;
  const m = Math.round(s / 60);
  if (m < 90) return `${m} min`;
  return `${(m / 60).toFixed(1)} h`;
}

export function fmtBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(0)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** Bz orientation wording. Sign carries the meaning, so it is never dropped. */
export function bzOrientation(value: number | null | undefined): string {
  if (value === null || value === undefined || !Number.isFinite(value)) return NO_VALUE;
  if (value < 0) return "southward";
  if (value > 0) return "northward";
  return "at zero";
}

/** Neutralise spreadsheet formula execution in a CSV text field. Mirrors the
 *  backend guard; used for any CSV assembled on the frontend (chart exports). */
export function csvSafe(field: string): string {
  return /^[=+\-@\t\r]/.test(field) ? `'${field}` : field;
}

/** Escape text that came from a provider before it reaches the DOM. */
export function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}
