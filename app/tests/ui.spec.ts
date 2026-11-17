import { test, expect, type Page } from "@playwright/test";

// Fails a test on any uncaught error or console error.
function watchErrors(page: Page) {
  const errors: string[] = [];
  page.on("pageerror", (e) => errors.push(e.message));
  page.on("console", (m) => m.type() === "error" && errors.push(m.text()));
  return errors;
}

async function scanned(page: Page, query = "") {
  await page.goto("/" + query);
  await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
  await expect(page.getByRole("tablist", { name: "Views" })).toBeVisible({ timeout: 10_000 });
  // Move the pointer off the ring, which now sits where the button was.
  await page.mouse.move(0, 0);
}

function rows(page: Page) {
  return page.locator(".listside .rows button.row");
}

test.describe("before a scan", () => {
  test("lists drives by group and offers a scan", async ({ page }) => {
    const errors = watchErrors(page);
    await page.goto("/");
    const drives = page.getByRole("navigation", { name: "Drives" });
    await expect(drives.getByText("This computer")).toBeVisible();
    await expect(drives.getByText("Other drives")).toBeVisible();
    await expect(drives.getByRole("button", { name: /Macintosh HD/ })).toHaveAttribute("aria-current", "true");
    await expect(page.getByRole("heading", { name: "See what is using Macintosh HD" })).toBeVisible();
    await expect(page.getByText("742 GB used of 1.0 TB · 258 GB available")).toBeVisible();
    await drives.getByRole("button", { name: /Archive SSD/ }).click();
    await expect(page.getByRole("button", { name: "Scan Archive SSD" })).toBeVisible();
    expect(errors).toEqual([]);
  });
});

test.describe("scanning", () => {
  test("shows live progress, then the result", async ({ page }) => {
    const errors = watchErrors(page);
    await page.goto("/");
    await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
    await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
    await expect(page.getByText(/Counting Macintosh HD/)).toBeVisible();
    await expect(page.getByText(/files in .* folders/)).toBeVisible();
    await expect(page.getByText("Scanned in 2.1 s")).toBeVisible({ timeout: 10_000 });
    await expect(page.getByRole("button", { name: "Scan Again" })).toBeVisible();
    expect(errors).toEqual([]);
  });

  test("the ring fills in while the scan runs", async ({ page }) => {
    const errors = watchErrors(page);
    await page.goto("/");
    await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
    const live = page.getByRole("img", { name: /Ring chart of Macintosh HD: Users/ });
    await expect(live).toBeVisible();
    await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
    // Then the finished, clickable ring of the whole drive.
    await expect(page.getByRole("img", { name: /Ring chart of \/: Users 493 GB/ })).toBeVisible({ timeout: 10_000 });
    expect(errors).toEqual([]);
  });

  test("Stop keeps partial figures and says so", async ({ page }) => {
    await page.goto("/");
    await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
    await page.getByRole("button", { name: "Stop" }).click();
    await expect(page.getByText(/Stopped early, figures are partial/)).toBeVisible();
  });

  test("explains the administrator prompt while it is open", async ({ page }) => {
    await page.goto("/?demo-stage=permission");
    await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
    await expect(page.getByText(/allow administrator access for the fast scan/)).toBeVisible();
  });

  test("says when it reads the file table", async ({ page }) => {
    await page.goto("/?demo-stage=fileTable");
    await page.getByRole("button", { name: "Scan Macintosh HD" }).click();
    await expect(page.getByText("Reading Macintosh HD's file table. This takes a few seconds.")).toBeVisible();
  });

  test("a second scan replaces the first", async ({ page }) => {
    await scanned(page);
    await page.getByRole("button", { name: "Scan Again" }).click();
    await expect(page.getByRole("button", { name: "Stop" })).toBeVisible();
    await expect(page.getByText("Scanned in 2.1 s")).toBeVisible({ timeout: 10_000 });
  });
});

test.describe("folders", () => {
  test("largest first, with sizes and shares", async ({ page }) => {
    const errors = watchErrors(page);
    await scanned(page);
    await expect(page.getByRole("tab", { name: "Folders" })).toHaveAttribute("aria-selected", "true");
    const titles = await rows(page).locator(".title").allTextContents();
    expect(titles.slice(0, 3)).toEqual(["Users", "Applications", "System"]);
    const sizes = await rows(page).locator(".size").allTextContents();
    const gb = sizes.map((s) => parseFloat(s));
    expect([...gb].sort((a, b) => b - a)).toEqual(gb);
    await expect(rows(page).first()).toContainText("493 GB");
    await expect(rows(page).first()).toContainText("67.2%");
    expect(errors).toEqual([]);
  });

  test("the Trash folder is shown by name, as its own kind", async ({ page }) => {
    await scanned(page);
    const trash = rows(page).filter({ hasText: "Trash" });
    await expect(trash).toHaveCount(1);
    await expect(trash).toContainText("mostly trash");
    await expect(page.getByText(".Trashes")).toHaveCount(0);
    await expect(page.locator("section.card").filter({ hasText: "By kind" })).toContainText("Trash");
  });

  test("click into a folder, breadcrumb and keyboard back", async ({ page }) => {
    await scanned(page);
    await rows(page).filter({ hasText: "Users" }).click();
    const crumbs = page.getByRole("navigation", { name: "Location" });
    await expect(crumbs.locator(".here")).toHaveText("Users");
    await rows(page).filter({ hasText: "sam" }).click();
    await expect(crumbs.locator(".here")).toHaveText("sam");
    await expect(page.getByText("Click the centre to go back")).toBeVisible();
    await crumbs.getByRole("button", { name: "Users" }).click();
    await expect(crumbs.locator(".here")).toHaveText("Users");
    await page.keyboard.press("Escape");
    await expect(crumbs.locator(".here")).toHaveText("/");
  });

  test("ring: hover names the slice, click opens it, centre goes back", async ({ page }) => {
    await scanned(page);
    const canvas = page.locator("canvas");
    const box = (await canvas.boundingBox())!;
    const cx = box.x + box.width / 2;
    const cy = box.y + box.height / 2;
    const centre = page.locator(".centre .what");
    await expect(centre).toHaveText("/");
    // Users is the largest slice and starts at 12 o'clock, going clockwise.
    const x = cx + box.width * 0.12;
    const y = cy - box.height * 0.2;
    await page.mouse.move(x, y);
    await expect(centre).toHaveText("Users");
    await expect(rows(page).filter({ hasText: "Users" })).toHaveClass(/hot/);
    await page.mouse.click(x, y);
    await expect(page.getByRole("navigation", { name: "Location" }).locator(".here")).toHaveText("Users");
    await page.mouse.click(cx, cy);
    await expect(page.getByRole("navigation", { name: "Location" }).locator(".here")).toHaveText("/");
  });

  test("ring has a text description for screen readers", async ({ page }) => {
    await scanned(page);
    await expect(page.getByRole("img", { name: /Ring chart of \/: Users 493 GB/ })).toBeVisible();
  });
});

test.describe("apps", () => {
  test("lists apps with their parts and expands to show where", async ({ page }) => {
    const errors = watchErrors(page);
    await scanned(page);
    await page.getByRole("tab", { name: "Apps" }).click();
    const photos = page.getByRole("button", { name: /Photos/ }).first();
    await expect(photos).toContainText("App 400 MB · Data 118 GB · Cache 2.1 GB");
    await expect(photos).toHaveAttribute("aria-expanded", "false");
    await photos.click();
    await expect(photos).toHaveAttribute("aria-expanded", "true");
    await expect(page.getByText("/Users/sam/Library/Application Support/Photos")).toBeVisible();
    await expect(page.getByRole("button", { name: "Show in Finder" }).first()).toBeVisible();
    expect(errors).toEqual([]);
  });
});

test.describe("largest files", () => {
  test("filter by kind", async ({ page }) => {
    await scanned(page);
    await page.getByRole("tab", { name: "Largest Files" }).click();
    const list = page.locator("li .title");
    await expect(list.first()).toHaveText("Photos-export-2023.zip");
    await expect(list).toHaveCount(7);
    const chips = page.getByRole("group", { name: "Show files of one kind" });
    await chips.getByRole("button", { name: "Videos" }).click();
    await expect(chips.getByRole("button", { name: "Videos" })).toHaveAttribute("aria-pressed", "true");
    await expect(list).toHaveText(["Wedding-raw.mov", "Final-cut-render.mov", "IMG_9356.mov"]);
    await chips.getByRole("button", { name: "All" }).click();
    await expect(list).toHaveCount(7);
  });
});

test.describe("clean up", () => {
  test("rebuildable items start ticked, downloads do not", async ({ page }) => {
    await scanned(page);
    await page.getByRole("tab", { name: "Clean Up" }).click();
    await expect(page.getByText("selected of 39.7 GB found")).toBeVisible();
    await expect(page.locator(".top strong")).toHaveText("18.4 GB");
    const download = page.getByRole("checkbox", { name: /Photos-export-2023.zip/ });
    await expect(download).not.toBeChecked();
    await expect(page.getByRole("checkbox", { name: /Google Chrome/ })).toBeChecked();
  });

  test("review lists every path; cancel changes nothing", async ({ page }) => {
    await scanned(page);
    await page.getByRole("tab", { name: "Clean Up" }).click();
    await page.getByRole("button", { name: "Review…" }).click();
    await expect(page.getByRole("heading", { name: "Move 4 items (18.4 GB) to the Trash?" })).toBeVisible();
    await expect(page.locator(".paths li")).toHaveCount(4);
    await expect(page.locator(".paths")).toContainText("/Users/sam/Library/Caches/Google/Chrome (contents)");
    await page.getByRole("button", { name: "Cancel" }).click();
    await expect(page.locator(".top strong")).toHaveText("18.4 GB");
  });

  test("move to Trash reports what was moved and removes those items", async ({ page }) => {
    await scanned(page);
    await page.getByRole("tab", { name: "Clean Up" }).click();
    await page.getByRole("checkbox", { name: /Docker/ }).uncheck();
    await expect(page.locator(".top strong")).toHaveText("15.4 GB");
    await page.getByRole("button", { name: "Review…" }).click();
    await page.getByRole("button", { name: "Move to Trash" }).click();
    await expect(page.getByRole("status")).toContainText("Moved to the Trash: 15.4 GB");
    await expect(page.getByRole("status")).toContainText("from 3 items");
    await expect(page.getByRole("checkbox", { name: /Google Chrome/ })).toHaveCount(0);
    await expect(page.getByRole("checkbox", { name: /Docker/ })).not.toBeChecked();
  });

  test("nothing ticked means nothing to review", async ({ page }) => {
    await scanned(page);
    await page.getByRole("tab", { name: "Clean Up" }).click();
    for (const name of [/Google Chrome/, /Docker/, /node_modules/, /Xcode/]) {
      await page.getByRole("checkbox", { name }).uncheck();
    }
    await expect(page.getByRole("button", { name: "Review…" })).toBeDisabled();
  });

  test("shows what the Trash holds and offers to open it", async ({ page }) => {
    await scanned(page);
    await page.getByRole("tab", { name: "Clean Up" }).click();
    await expect(page.getByText("The Trash holds 9.0 GB")).toBeVisible();
    await expect(page.getByText(/Strata never empties it for you/)).toBeVisible();
    await page.getByRole("button", { name: "Open Trash" }).click();
  });
});

test.describe("on Windows", () => {
  test.use({ userAgent: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36" });

  test("uses Windows words", async ({ page }) => {
    await scanned(page);
    await expect(page.getByRole("button", { name: "Show this folder in Explorer" })).toBeVisible();
    await page.getByRole("tab", { name: "Clean Up" }).click();
    await expect(page.getByText("The Recycle Bin holds 9.0 GB")).toBeVisible();
    await expect(page.getByRole("button", { name: "Open Recycle Bin" })).toBeVisible();
    await page.getByRole("button", { name: "Review…" }).click();
    await expect(page.getByRole("button", { name: "Move to Recycle Bin" })).toBeVisible();
  });
});

test.describe("look and layout", () => {
  test("dark mode renders every tab without errors", async ({ page }) => {
    const errors = watchErrors(page);
    await page.emulateMedia({ colorScheme: "dark" });
    await scanned(page);
    const bg = await page.evaluate(() => getComputedStyle(document.body).backgroundColor);
    expect(bg).not.toBe("rgb(255, 255, 255)");
    for (const tab of ["Apps", "Largest Files", "Clean Up", "Folders"]) {
      await page.getByRole("tab", { name: tab }).click();
      await expect(page.getByRole("tab", { name: tab })).toHaveAttribute("aria-selected", "true");
    }
    expect(errors).toEqual([]);
  });

  for (const width of [800, 1440]) {
    test(`no sideways scrolling at ${width}px`, async ({ page }) => {
      await page.setViewportSize({ width, height: 800 });
      await scanned(page);
      for (const tab of ["Folders", "Apps", "Largest Files", "Clean Up"]) {
        await page.getByRole("tab", { name: tab }).click();
        const overflow = await page.evaluate(() => {
          const main = document.querySelector("main")!;
          return main.scrollWidth - main.clientWidth;
        });
        expect(overflow, `${tab} at ${width}px`).toBeLessThanOrEqual(1);
      }
    });
  }

  test("keyboard: tabs and folder rows can be reached and used", async ({ page }) => {
    await scanned(page);
    const users = rows(page).filter({ hasText: "Users" });
    await users.focus();
    await expect(users).toHaveClass(/hot/);
    await page.keyboard.press("Enter");
    await expect(page.getByRole("navigation", { name: "Location" }).locator(".here")).toHaveText("Users");
    const apps = page.getByRole("tab", { name: "Apps" });
    await apps.focus();
    await page.keyboard.press("Enter");
    await expect(apps).toHaveAttribute("aria-selected", "true");
  });
});
