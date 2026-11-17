import { test, expect, type Page } from "@playwright/test";

async function scanned(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
  await expect(page.getByRole("tablist", { name: "Views" })).toBeVisible({ timeout: 10_000 });
  await page.mouse.move(0, 0);
}

/** WCAG contrast ratio between an element's text colour and the colour behind it. */
async function contrast(page: Page, selector: string): Promise<number> {
  return page.locator(selector).first().evaluate((el) => {
    const rgb = (c: string) => (c.match(/[\d.]+/g) ?? []).slice(0, 4).map(Number);
    const lum = ([r, g, b]: number[]) => {
      const f = (v: number) => {
        v /= 255;
        return v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
      };
      return 0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b);
    };
    // The first ancestor with an opaque background.
    let bg: number[] = [255, 255, 255];
    for (let n: Element | null = el; n; n = n.parentElement) {
      const c = rgb(getComputedStyle(n).backgroundColor);
      if (c.length === 3 || (c[3] ?? 0) >= 0.99) {
        bg = c.slice(0, 3);
        break;
      }
    }
    const fg = rgb(getComputedStyle(el).color).slice(0, 3);
    const [a, b] = [lum(fg), lum(bg)].sort((x, y) => y - x);
    return (a + 0.05) / (b + 0.05);
  });
}

for (const scheme of ["light", "dark"] as const) {
  test(`text meets WCAG AA contrast in ${scheme} mode`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme });
    await scanned(page);
    expect(await contrast(page, ".listside .rows .title")).toBeGreaterThanOrEqual(7);
    expect(await contrast(page, ".listside .rows .sub")).toBeGreaterThanOrEqual(4.5);
    expect(await contrast(page, ".status")).toBeGreaterThanOrEqual(4.5);
    expect(await contrast(page, "aside .meta")).toBeGreaterThanOrEqual(4.5);
    expect(await contrast(page, '[role=tab][aria-selected="false"]')).toBeGreaterThanOrEqual(4.5);
  });

  test(`Increase contrast raises secondary text to AAA in ${scheme} mode`, async ({ page }) => {
    await page.emulateMedia({ colorScheme: scheme, contrast: "more" });
    await scanned(page);
    expect(await contrast(page, ".listside .rows .sub")).toBeGreaterThanOrEqual(7);
  });
}

test("Windows High Contrast: data colours stay and get an outline", async ({ page }) => {
  await page.emulateMedia({ forcedColors: "active" });
  await scanned(page);
  const fill = page.locator(".listside .rows button.row").first().locator(".bar > span");
  const bg = await fill.evaluate((el) => getComputedStyle(el).backgroundColor);
  expect(bg).not.toBe("rgba(0, 0, 0, 0)");
  // Users is mostly videos: its bar keeps the videos colour.
  const videos = await page.evaluate(() => {
    const probe = document.createElement("span");
    probe.style.cssText = "background: var(--kind-videos); forced-color-adjust: none";
    document.body.appendChild(probe);
    const c = getComputedStyle(probe).backgroundColor;
    probe.remove();
    return c;
  });
  expect(bg).toBe(videos);
  const outline = await page.locator(".listside .rows .bar").first().evaluate((el) => getComputedStyle(el).outlineStyle);
  expect(outline).toBe("solid");
  await expect(page.locator("canvas")).toBeVisible();
});

test("Reduce Motion: no transitions anywhere", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await scanned(page);
  const durations = await page.evaluate(() =>
    Array.from(document.querySelectorAll("*")).map((el) => getComputedStyle(el).transitionDuration).filter((d) => d.split(",").some((x) => parseFloat(x) > 0)),
  );
  expect(durations).toEqual([]);
});
