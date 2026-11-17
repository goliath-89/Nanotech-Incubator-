import { test, expect, type Page } from "@playwright/test";

async function scanned(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
  await expect(page.getByRole("tablist", { name: "Views" })).toBeVisible({ timeout: 10_000 });
  await page.mouse.move(0, 0);
}

/** Where each top-level folder's slice should be, from the list's own sizes. */
async function expectedSlices(page: Page) {
  const rows = page.locator(".listside .rows li");
  const names = await rows.locator(".title").allTextContents();
  const sizes = (await rows.locator(".size").allTextContents()).map((s) => parseFloat(s));
  const total = sizes.reduce((a, b) => a + b, 0);
  let start = 0;
  return names.map((name, i) => {
    const share = sizes[i] / total;
    const slice = { name, mid: (start + share / 2) * Math.PI * 2, share };
    start += share;
    return slice;
  });
}

test("each slice sits where its share says, in list order, clockwise from 12 o'clock", async ({ page }) => {
  await scanned(page);
  await page.waitForTimeout(600); // let the opening spring finish
  const box = (await page.locator("canvas").boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  const r = box.width * 0.24; // inside the first ring
  const centre = page.locator(".centre .what");
  for (const s of await expectedSlices(page)) {
    if (s.share < 0.01) continue; // too thin to aim at reliably
    await page.mouse.move(cx + r * Math.sin(s.mid), cy - r * Math.cos(s.mid));
    await expect(centre, s.name).toHaveText(s.name);
  }
});

test("with Reduce Motion the ring appears complete at once", async ({ page }) => {
  await page.emulateMedia({ reducedMotion: "reduce" });
  await scanned(page);
  const canvas = page.locator("canvas");
  // No spring ran, so no animation frame was recorded.
  await expect(canvas).toHaveAttribute("data-draw-ms", /\d/);
  expect(await canvas.getAttribute("data-slowest-frame-ms")).toBeNull();
});

test.describe("looks the same as approved", () => {
  // Pixel snapshots of the ring; the text in the centre is masked out, so
  // fonts that differ between machines do not matter.
  // To approve a deliberate change: npx playwright test ring --update-snapshots
  test("light", async ({ page }) => {
    await scanned(page);
    await page.waitForTimeout(800);
    await expect(page.locator("canvas")).toHaveScreenshot("ring-light.png", { maxDiffPixelRatio: 0.01, mask: [page.locator(".centre > *")] });
  });

  test("dark", async ({ page }) => {
    await page.emulateMedia({ colorScheme: "dark" });
    await scanned(page);
    await page.waitForTimeout(800);
    await expect(page.locator("canvas")).toHaveScreenshot("ring-dark.png", { maxDiffPixelRatio: 0.01, mask: [page.locator(".centre > *")] });
  });

  test("one kind picked", async ({ page }) => {
    await scanned(page);
    await page.locator("section.card").filter({ hasText: "By kind" }).getByRole("button", { name: /^Photos/ }).click();
    await page.waitForTimeout(300);
    await expect(page.locator("canvas")).toHaveScreenshot("ring-photos.png", { maxDiffPixelRatio: 0.01, mask: [page.locator(".centre > *")] });
  });
});
