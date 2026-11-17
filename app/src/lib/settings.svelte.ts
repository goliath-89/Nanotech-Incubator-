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

/** Light or dark: follow the system, or choose. */
export type Theme = "system" | "light" | "dark";
const THEME_KEY = "strata.theme";

function loadTheme(): Theme {
  try {
    const t = localStorage.getItem(THEME_KEY);
    if (t === "light" || t === "dark" || t === "system") return t;
  } catch {
    // Fall back to the system.
  }
  return "system";
}

export const appearance = $state({ theme: loadTheme() });

/** Apply the theme to the page; views that draw with colours redraw. */
export function applyTheme() {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (appearance.theme === "system") delete root.dataset.theme;
  else root.dataset.theme = appearance.theme;
  dispatchEvent(new Event("strata-theme"));
}

export function setTheme(theme: Theme) {
  appearance.theme = theme;
  try {
    localStorage.setItem(THEME_KEY, theme);
  } catch {
    // Applied for this session only.
  }
  applyTheme();
}

applyTheme();
