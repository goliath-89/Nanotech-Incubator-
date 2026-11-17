// Sizes in decimal units (1 GB = 1,000,000,000 bytes), as macOS and drive
// makers use. Windows Explorer shows binary units; a setting comes later.

const UNITS = ["bytes", "KB", "MB", "GB", "TB"];

export function bytes(value: string | number | bigint): string {
  let n = Number(value);
  if (!Number.isFinite(n) || n <= 0) return "0 bytes";
  let unit = 0;
  while (n >= 1000 && unit < UNITS.length - 1) {
    n /= 1000;
    unit++;
  }
  if (unit === 0) return `${Math.round(n)} bytes`;
  return `${n >= 100 ? n.toFixed(0) : n.toFixed(1)} ${UNITS[unit]}`;
}

const grouping = new Intl.NumberFormat();

export function count(value: string | number): string {
  return grouping.format(Number(value));
}

export function share(part: string | number, whole: string | number): number {
  const w = Number(whole);
  return w > 0 ? Number(part) / w : 0;
}

export function percent(part: string | number, whole: string | number): string {
  const p = share(part, whole) * 100;
  return p > 0 && p < 0.1 ? "<0.1%" : `${p.toFixed(1)}%`;
}

export function seconds(ms: string | number): string {
  const s = Number(ms) / 1000;
  return s < 10 ? `${s.toFixed(1)} s` : `${Math.round(s)} s`;
}
