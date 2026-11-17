// Arrow-key movement inside a list, attached to each item:
// Up/Down to the previous/next item, Home/End to the first/last. Items are
// the elements matching `selector` inside the nearest list (<ul>).
export function listKeys(e: KeyboardEvent, selector: string) {
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(e.key)) return;
  const list = (e.currentTarget as HTMLElement).closest("ul");
  if (!list) return;
  const items = Array.from(list.querySelectorAll<HTMLElement>(selector));
  if (items.length === 0) return;
  const at = items.indexOf(document.activeElement as HTMLElement);
  const next =
    e.key === "Home" ? 0 : e.key === "End" ? items.length - 1 : e.key === "ArrowDown" ? Math.min(items.length - 1, at + 1) : Math.max(0, at - 1);
  e.preventDefault();
  items[next].focus();
}
