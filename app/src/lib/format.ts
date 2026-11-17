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
