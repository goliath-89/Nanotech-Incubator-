import { test, expect, type Page } from "@playwright/test";

// Day 21 target: the ring stays at 60 fps (under 16.7 ms a frame) with a
// tree of 100,000 folders, while hovering and while it springs open.
const FRAME_MS = 16.7;

async function bigScan(page: Page, slices: number) {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await page.goto(`/?demo-ring=${slices}`);
  await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
  await expect(page.getByRole("tablist", { name: "Views" })).toBeVisible({ timeout: 10_000 });
  await page.mouse.move(0, 0);
  const canvas = page.locator("canvas");
  await expect(canvas).toHaveAttribute("data-slowest-frame-ms", /\d/, { timeout: 5_000 });
  return { canvas, errors };
}

test("springs open within a frame budget with 100,000 folders", async ({ page }) => {
  const { canvas, errors } = await bigScan(page, 100_000);
  const slowest = Number(await canvas.getAttribute("data-slowest-frame-ms"));
  const build = Number(await canvas.getAttribute("data-build-ms"));
  console.log(`opening: slowest frame ${slowest} ms; layout and image built once in ${build} ms`);
  // A one-off cost when the ring changes, hidden under the opening spring.
  expect(build).toBeLessThan(150);
  expect(slowest).toBeLessThan(FRAME_MS);
  expect(errors).toEqual([]);
});

test("hovering stays within a frame budget with 100,000 folders", async ({ page }) => {
  const { canvas } = await bigScan(page, 100_000);
  const box = (await canvas.boundingBox())!;
  const cx = box.x + box.width / 2;
  const cy = box.y + box.height / 2;
  const times: number[] = [];
  // Sweep round the outer two rings, where the slices are thinnest.
  for (let i = 0; i < 80; i++) {
    const a = (i / 80) * Math.PI * 2;
    const r = box.width * (i % 2 ? 0.42 : 0.34);
    await page.mouse.move(cx + r * Math.sin(a), cy - r * Math.cos(a));
    times.push(Number(await canvas.getAttribute("data-draw-ms")));
  }
  times.sort((a, b) => a - b);
  const worst = times[times.length - 1];
  console.log(`hover: median ${times[40]} ms, worst ${worst} ms`);
  expect(worst).toBeLessThan(FRAME_MS);
});

test("drilling in springs open from the slice and keeps the budget", async ({ page }) => {
  const { canvas } = await bigScan(page, 100_000);
  const box = (await canvas.boundingBox())!;
  await canvas.evaluate((c) => delete (c as HTMLCanvasElement).dataset.slowestFrameMs);
  // The first, largest folder starts at 12 o'clock on the inner ring.
  await page.mouse.click(box.x + box.width / 2 + box.width * 0.1, box.y + box.height * 0.22);
  await expect(page.getByRole("navigation", { name: "Location" }).locator(".here")).toHaveText(/Folder/);
  await expect(canvas).toHaveAttribute("data-slowest-frame-ms", /\d/, { timeout: 5_000 });
  const slowest = Number(await canvas.getAttribute("data-slowest-frame-ms"));
  console.log(`drill: slowest frame ${slowest} ms`);
  expect(slowest).toBeLessThan(FRAME_MS);
});

test("the animation keeps up with the screen (frames per second)", async ({ page }) => {
  const { canvas } = await bigScan(page, 100_000);
  const box = (await canvas.boundingBox())!;
  const fps = await page.evaluate(
    ([x, y]) =>
      new Promise<number>((resolve) => {
        let frames = 0;
        const start = performance.now();
        const count = () => {
          frames++;
          if (performance.now() - start < 1000) requestAnimationFrame(count);
          else resolve(frames);
        };
        requestAnimationFrame(count);
        // Keep the ring busy: hover events every few milliseconds.
        let i = 0;
        const timer = setInterval(() => {
          const a = (i++ / 30) * Math.PI * 2;
          const el = document.querySelector("canvas")!;
          el.dispatchEvent(new MouseEvent("mousemove", { clientX: x + 130 * Math.sin(a), clientY: y - 130 * Math.cos(a), bubbles: true }));
          if (performance.now() - start > 1000) clearInterval(timer);
        }, 4);
      }),
    [box.x + box.width / 2, box.y + box.height / 2],
  );
  console.log(`fps while hovering: ${fps}`);
  expect(fps).toBeGreaterThanOrEqual(50);
});
