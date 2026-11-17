// Settings kept between runs (in the window's own storage), read by
// formatting code everywhere. Changing one re-renders what uses it.
import { isWindows } from "./platform";

/** How sizes are shown.
 *  "decimal": 1 GB = 1,000,000,000 bytes (macOS, drive makers).
 *  "windows": 1 GB = 1,073,741,824 bytes, labelled GB (Windows Explorer).
 *  "binary":  1 GiB = 1,073,741,824 bytes, labelled GiB. */
export type Units = "decimal" | "windows" | "binary";

const KEY = "strata.units";

function load(): Units {
  try {
    const saved = localStorage.getItem(KEY);
    if (saved === "decimal" || saved === "windows" || saved === "binary") return saved;
  } catch {
    // Storage can be unavailable; fall back to the system's convention.
  }
  return isWindows ? "windows" : "decimal";
}

export const settings = $state({ units: load() });

export function setUnits(units: Units) {
  settings.units = units;
  try {
    localStorage.setItem(KEY, units);
  } catch {
    // Not remembered, but still applied for this session.
  }
}
