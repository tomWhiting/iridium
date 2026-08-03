// Renders icon.html to a 1024×1024 transparent PNG.
//
// Usage: node render-icon.js <icon.html> <out.png>
//
// Drives the Playwright chromium already cached on this machine — no new
// dependency is installed for the icon. Two environment variables override
// the machine-specific defaults when they move:
//
//   IRIDIUM_PLAYWRIGHT_CORE  path require() can resolve playwright-core from
//   IRIDIUM_CHROMIUM         the chromium executable to launch
//
// The page is loaded from file://, so chromium is launched with
// --allow-file-access-from-files: the tile's @font-face reads the editor's
// own JetBrains Mono out of the repo, and file-to-file font loads are
// otherwise blocked by the per-file origin rule.

"use strict";

const path = require("path");
const { pathToFileURL } = require("url");

const PLAYWRIGHT_CORE =
  process.env.IRIDIUM_PLAYWRIGHT_CORE ||
  "/Users/tom/Developer/ablative/apps/manifold/surface/page/node_modules/playwright-core";
const CHROMIUM =
  process.env.IRIDIUM_CHROMIUM ||
  "/Users/tom/Library/Caches/ms-playwright/chromium-1228/chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing";

const htmlPath = process.argv[2];
const outPath = process.argv[3];
if (!htmlPath || !outPath) {
  console.error("usage: node render-icon.js <icon.html> <out.png>");
  process.exit(1);
}

const { chromium } = require(PLAYWRIGHT_CORE);

(async () => {
  const browser = await chromium.launch({
    headless: true,
    executablePath: CHROMIUM,
    args: ["--allow-file-access-from-files"],
  });
  try {
    const page = await browser.newPage({
      viewport: { width: 1024, height: 1024 },
      deviceScaleFactor: 1,
    });
    await page.goto(pathToFileURL(path.resolve(htmlPath)).href, {
      waitUntil: "load",
    });
    // The "77" must be shaped in JetBrains Mono, not a fallback that loaded
    // faster: wait for every declared font, then verify the face is in.
    await page.evaluate(() => document.fonts.ready);
    const loaded = await page.evaluate(() =>
      document.fonts.check('430px "JetBrains Mono"'),
    );
    if (!loaded) {
      throw new Error("JetBrains Mono did not load; the icon would render in a fallback face");
    }
    await page.screenshot({
      path: path.resolve(outPath),
      omitBackground: true,
      clip: { x: 0, y: 0, width: 1024, height: 1024 },
    });
  } finally {
    await browser.close();
  }
  console.log(`wrote ${outPath}`);
})().catch((error) => {
  console.error(`render-icon: ${error.stack || error}`);
  process.exit(1);
});
