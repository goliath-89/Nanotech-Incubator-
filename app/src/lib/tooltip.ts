// One shared tooltip for the whole window. Use as `use:tip={"text"}`.
// Shows after a short pause on hover, at once on keyboard focus; hides on
// leave, blur, Escape and scroll. While shown it describes its element for
// screen readers (aria-describedby).

let el: HTMLDivElement | null = null;
let owner: HTMLElement | null = null;
let timer: ReturnType<typeof setTimeout> | null = null;

function bubble(): HTMLDivElement {
  if (!el) {
    el = document.createElement("div");
    el.id = "strata-tooltip";
    el.className = "tooltip";
    el.setAttribute("role", "tooltip");
    document.body.appendChild(el);
    addEventListener("scroll", hide, true);
    addEventListener("keydown", (e) => e.key === "Escape" && hide());
  }
  return el;
}

function show(node: HTMLElement, text: string) {
  if (!text) return;
  const b = bubble();
  b.textContent = text;
  b.classList.add("on");
  owner = node;
  node.setAttribute("aria-describedby", b.id);
  // Above the element, centred, kept inside the window.
  const r = node.getBoundingClientRect();
  const w = b.offsetWidth;
  const h = b.offsetHeight;
  const left = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), innerWidth - w - 8);
  const above = r.top - h - 8;
  b.style.left = `${left}px`;
  b.style.top = `${above >= 8 ? above : r.bottom + 8}px`;
}

function hide() {
  if (timer) clearTimeout(timer);
  timer = null;
  if (owner) owner.removeAttribute("aria-describedby");
  owner = null;
  el?.classList.remove("on");
}

export function tip(node: HTMLElement, text: string) {
  let current = text;
  const enter = () => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => show(node, current), 350);
  };
  const focus = () => {
    if (node.matches(":focus-visible")) show(node, current);
  };
  node.addEventListener("pointerenter", enter);
  node.addEventListener("pointerleave", hide);
  node.addEventListener("focus", focus);
  node.addEventListener("blur", hide);
  return {
    update(next: string) {
      current = next;
      if (owner === node && el) el.textContent = next;
    },
    destroy() {
      if (owner === node) hide();
      node.removeEventListener("pointerenter", enter);
      node.removeEventListener("pointerleave", hide);
      node.removeEventListener("focus", focus);
      node.removeEventListener("blur", hide);
    },
  };
}
