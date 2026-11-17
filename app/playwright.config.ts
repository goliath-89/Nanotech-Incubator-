import { defineConfig } from "@playwright/test";

// UI tests run the interface in a browser with its built-in example data
// (src/lib/demo.ts), so they need no Rust side and no real drive.
export default defineConfig({
  testDir: "tests",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  reporter: process.env.CI ? [["list"], ["html", { open: "never" }]] : "list",
  use: {
    baseURL: "http://localhost:4173",
    trace: "off",
    viewport: { width: 1180, height: 760 },
  },
  // A macOS user agent by default (the app picks Trash/Finder or Recycle
  // Bin/Explorer from it); the Windows tests set their own.
  projects: [
    {
      name: "chromium",
      use: {
        browserName: "chromium",
        userAgent:
          "Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0 Safari/537.36",
      },
    },
  ],
  webServer: {
    command: "npm run build && npx vite preview --port 4173 --strictPort",
    url: "http://localhost:4173",
    reuseExistingServer: !process.env.CI,
    timeout: 120_000,
  },
});
