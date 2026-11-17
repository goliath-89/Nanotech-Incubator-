// Words that differ between Windows and macOS.
const windows = typeof navigator !== "undefined" && navigator.userAgent.includes("Windows");

export const isWindows = windows;
export const trashName = windows ? "Recycle Bin" : "Trash";
export const fileManager = windows ? "Explorer" : "Finder";
