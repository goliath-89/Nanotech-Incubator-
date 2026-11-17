// The right-click menu: one for the whole window. Views call openMenu()
// with the items that make sense for what was clicked.

export interface MenuItem {
  label: string;
  run: () => void | Promise<void>;
  disabled?: boolean;
}

export const menu = $state<{ open: boolean; x: number; y: number; items: MenuItem[]; title: string; returnFocus: HTMLElement | null }>({
  open: false,
  x: 0,
  y: 0,
  items: [],
  title: "",
  returnFocus: null,
});

/** Open at the pointer, or under the focused element when opened from the keyboard. */
export function openMenu(e: MouseEvent | KeyboardEvent, title: string, items: MenuItem[]) {
  e.preventDefault();
  e.stopPropagation();
  const target = e.currentTarget as HTMLElement | null;
  let x: number;
  let y: number;
  if (e instanceof MouseEvent && e.type === "contextmenu" && (e.clientX || e.clientY)) {
    x = e.clientX;
    y = e.clientY;
  } else {
    const r = (target ?? document.body).getBoundingClientRect();
    x = r.left + 24;
    y = r.bottom - 4;
  }
  Object.assign(menu, { open: true, x, y, items, title, returnFocus: (document.activeElement as HTMLElement) ?? target });
}

export function closeMenu() {
  if (!menu.open) return;
  menu.open = false;
  menu.returnFocus?.focus?.();
}

/** Menu key or Shift+F10 on a focused item opens its menu. */
export function isMenuKey(e: KeyboardEvent): boolean {
  return e.key === "ContextMenu" || (e.key === "F10" && e.shiftKey);
}

/** Copy text to the clipboard; falls back to a hidden text field. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    return true;
  } catch {
    const area = document.createElement("textarea");
    area.value = text;
    area.style.position = "fixed";
    area.style.opacity = "0";
    document.body.appendChild(area);
    area.select();
    const ok = document.execCommand("copy");
    area.remove();
    return ok;
  }
}

/** A short confirmation at the bottom of the window ("Path copied"). */
export const toast = $state<{ text: string; id: number }>({ text: "", id: 0 });
let toastTimer: ReturnType<typeof setTimeout> | null = null;
export function showToast(text: string) {
  toast.text = text;
  toast.id++;
  if (toastTimer) clearTimeout(toastTimer);
  toastTimer = setTimeout(() => (toast.text = ""), 1800);
}

/** Copy a path and confirm it. */
export async function copyPath(path: string) {
  showToast((await copyText(path)) ? "Path copied" : "Could not copy the path");
}
