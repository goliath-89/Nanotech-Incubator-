// Sizes, counts and percentages for display, in the system's language
// (decimal separator, digit grouping) and the units chosen in Settings.
import { settings } from "./settings.svelte";

const locale = typeof navigator !== "undefined" ? navigator.language : undefined;
const formats = new Map<string, Intl.NumberFormat>();
function fmt(digits: number): Intl.NumberFormat {
  const key = String(digits);
  let f = formats.get(key);
  if (!f) formats.set(key, (f = new Intl.NumberFormat(locale, { minimumFractionDigits: digits, maximumFractionDigits: digits })));
  return f;
}

const LABELS = {
  decimal: ["bytes", "KB", "MB", "GB", "TB", "PB"],
  windows: ["bytes", "KB", "MB", "GB", "TB", "PB"],
  binary: ["bytes", "KiB", "MiB", "GiB", "TiB", "PiB"],
} as const;

export function bytes(value: string | number | bigint): string {
  let n = Number(value);
  if (!Number.isFinite(n) || n <= 0) return "0 bytes";
  const step = settings.units === "decimal" ? 1000 : 1024;
  const labels = LABELS[settings.units];
  let unit = 0;
  // Switch unit just before the number would need four digits.
  while (n >= 1000 && unit < labels.length - 1) {
    n /= step;
    unit++;
  }
  if (unit === 0) return `${fmt(0).format(Math.round(n))} bytes`;
  return `${fmt(n >= 100 ? 0 : 1).format(n)} ${labels[unit]}`;
}

export function count(value: string | number): string {
  return fmt(0).format(Number(value));
}

export function share(part: string | number, whole: string | number): number {
  const w = Number(whole);
  return w > 0 ? Number(part) / w : 0;
}

const percentFormat = new Intl.NumberFormat(locale, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 });
const tiny = `<${percentFormat.format(0.001)}`;

export function percent(part: string | number, whole: string | number): string {
  const p = share(part, whole);
  return p > 0 && p < 0.001 ? tiny : percentFormat.format(p);
}

export function seconds(ms: string | number): string {
  const s = Number(ms) / 1000;
  return `${fmt(s < 10 ? 1 : 0).format(s)} s`;
}

const dateFormat = new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric" });
const relative = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });

/** "3 Mar 2026", from unix seconds; "unknown" when missing. */
export function day(unix: number | null | undefined): string {
  return unix ? dateFormat.format(new Date(unix * 1000)) : "unknown";
}

/** "today", "3 days ago", "5 months ago", "2 years ago". */
export function ago(unix: number | null | undefined, now = Date.now()): string {
  if (!unix) return "";
  const days = Math.floor((now / 1000 - unix) / 86_400);
  if (days < 1) return relative.format(0, "day");
  if (days < 31) return relative.format(-days, "day");
  if (days < 365) return relative.format(-Math.floor(days / 30.4), "month");
  return relative.format(-Math.floor(days / 365.25), "year");
}

/** "Modified 3 Mar 2026 (2 years ago) · Created 1 Jan 2020", or "" when unknown. */
export function fileDates(created: number | null | undefined, modified: number | null | undefined): string {
  const parts: string[] = [];
  if (modified) parts.push(`Modified ${day(modified)} (${ago(modified)})`);
  if (created) parts.push(`Created ${day(created)}`);
  return parts.join(" · ");
}
