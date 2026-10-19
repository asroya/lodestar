// Checks the website in a real browser before it deploys. Exits non-zero on any failure.
//
//   python3 -m http.server 8765 -d site &
//   npm i --no-save playwright@1.56.1 axe-core@4.10.2 && npx playwright install chromium
//   node packaging/check-site.mjs            # screenshots land in $SITE_SHOTS or a temp folder
//
// Covers: no sideways scroll at phone to desktop widths in both themes, no console
// errors, fonts load, WCAG 2.1 AA (axe), the download link resolving to the disk
// image, then the zip, then the releases page (and without JavaScript), the support
// links from site/config.js, the #support anchor the app opens, and the live tiles.
import { chromium } from "playwright";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createRequire } from "node:module";

const BASE = process.env.SITE_URL || "http://localhost:8765/";
const OUT = (process.env.SITE_SHOTS || fs.mkdtempSync(path.join(os.tmpdir(), "lodestar-site-"))) + "/";
fs.mkdirSync(OUT, { recursive: true });
const require = createRequire(path.join(process.cwd(), "package.json"));
const axe = fs.readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");
const failures = [];
const check = (ok, msg) => { console.log(`${ok ? "PASS" : "FAIL"}  ${msg}`); if (!ok) failures.push(msg); };

const release = (assets) => JSON.stringify({ tag_name: "v1.1.0", assets });
const DMG = { name: "Lodestar-arm64.dmg", size: 9_400_000, browser_download_url: "https://github.com/asroya/lodestar/releases/download/v1.1.0/Lodestar-arm64.dmg" };
const ZIP = { name: "Lodestar-1.1.0-arm64.zip", size: 9_100_000, browser_download_url: "https://github.com/asroya/lodestar/releases/download/v1.1.0/Lodestar-1.1.0-arm64.zip" };

const browser = await chromium.launch();

async function page(opts = {}, api = { status: 200, body: release([ZIP, DMG]) }, config) {
  const ctx = await browser.newContext({ viewport: opts.viewport || { width: 1440, height: 900 }, colorScheme: opts.scheme || "light", reducedMotion: opts.reduced ? "reduce" : "no-preference", javaScriptEnabled: opts.js !== false });
  const p = await ctx.newPage();
  const errors = [];
  p.on("pageerror", (e) => errors.push(String(e)));
  p.on("console", (m) => { if (m.type() === "error") errors.push(m.text()); });
  await p.route("https://api.github.com/**", (r) => r.fulfill({ status: api.status, contentType: "application/json", body: api.body || "{}" }));
  if (config) await p.route("**/config.js", (r) => r.fulfill({ contentType: "text/javascript", body: config }));
  await p.goto(BASE, { waitUntil: "networkidle" });
  await p.evaluate(() => document.fonts.ready);
  return { p, ctx, errors };
}

// 1. Layout at every width and theme: no sideways scroll, no console errors, fonts load.
for (const [w, h] of [[1440, 900], [1024, 768], [390, 844], [360, 740]]) {
  for (const scheme of ["light", "dark"]) {
    const { p, ctx, errors } = await page({ viewport: { width: w, height: h }, scheme });
    const over = await p.evaluate(() => document.documentElement.scrollWidth - document.documentElement.clientWidth);
    check(over <= 0, `${w}px ${scheme}: no horizontal scroll (overflow ${over}px)`);
    check(errors.length === 0, `${w}px ${scheme}: no console errors ${errors.join(" | ")}`);
    const fonts = await p.evaluate(() => [...document.fonts].filter((f) => f.status === "loaded").map((f) => f.family + f.weight));
    check(fonts.some((f) => f.includes("Plex")), `${w}px ${scheme}: Plex fonts loaded (${fonts.length})`);
    const bg = await p.evaluate(() => getComputedStyle(document.body).backgroundColor);
    check(scheme === "dark" ? bg === "rgb(21, 20, 19)" : bg === "rgb(245, 243, 239)", `${w}px ${scheme}: body background ${bg}`);
    await p.screenshot({ path: `${OUT}${w}-${scheme}.png`, fullPage: true });
    if (w === 1440) await p.screenshot({ path: `${OUT}${w}-${scheme}-fold.png` });
    await ctx.close();
  }
}

// 2. Accessibility (axe, WCAG 2.1 AA) in both themes.
for (const scheme of ["light", "dark"]) {
  const { p, ctx } = await page({ scheme });
  await p.addScriptTag({ content: axe });
  const res = await p.evaluate(async () => (await axe.run(document, { runOnly: ["wcag2a", "wcag2aa", "wcag21aa", "best-practice"] })).violations.map((v) => `${v.id} (${v.nodes.length}): ${v.nodes.slice(0, 3).map((n) => n.target.join(" ")).join(", ")}`));
  check(res.length === 0, `axe ${scheme}: ${res.length ? res.join("\n      ") : "no violations"}`);
  await ctx.close();
}

// 3. Download resolution: dmg > zip > releases page, and without JavaScript.
{
  const { p, ctx } = await page();
  const hrefs = await p.$$eval("[data-download]", (as) => as.map((a) => a.href));
  check(hrefs.length >= 3 && hrefs.every((h) => h === DMG.browser_download_url), `DMG preferred when present (${hrefs.length} links)`);
  const meta = await p.textContent("[data-download-meta]");
  check(/v1\.1\.0 · 9 MB disk image/.test(meta), `button shows version and size: "${meta}"`);
  check((await p.textContent("[data-version]")) === "v1.1.0", "hero pill shows the live version");
  await ctx.close();
}
{
  const { p, ctx } = await page({}, { status: 200, body: release([ZIP]) });
  const h = await p.getAttribute("[data-download]", "href");
  check(h === ZIP.browser_download_url, "falls back to the zip when there's no disk image");
  await ctx.close();
}
for (const status of [403, 500]) {
  const { p, ctx, errors } = await page({}, { status });
  const h = await p.getAttribute("[data-download]", "href");
  check(h === "https://github.com/asroya/lodestar/releases/latest", `API ${status}: links stay on the releases page`);
  check(errors.filter((e) => !/Failed to load resource/.test(e)).length === 0, `API ${status}: no script errors`);
  await ctx.close();
}
{
  const { p, ctx } = await page({ js: false });
  const h = await p.getAttribute("[data-download]", "href");
  check(h === "https://github.com/asroya/lodestar/releases/latest", "without JavaScript the download link still works");
  await ctx.close();
}

// 4. Support section: empty config renders no buttons, filled config shows them in order.
{
  const { p, ctx } = await page();
  check((await p.$$("#give a")).length === 0, "no payment links configured: no dead buttons");
  check(!(await p.isVisible("#give")), "no payment links configured: the button row is hidden");
  await ctx.close();
}
{
  const cfg = `window.LODESTAR_SUPPORT=[{label:"Pay by card",note:"Stripe",url:"https://buy.stripe.com/test_123"},{label:"Bad",url:"javascript:alert(1)"},{label:"GitHub Sponsors",url:"https://github.com/sponsors/asroya"}];`;
  const { p, ctx } = await page({}, undefined, cfg);
  const links = await p.$$eval("#give a", (as) => as.map((a) => [a.textContent, a.getAttribute("href"), a.className]));
  check(links.length === 2, `only https links render (${links.length})`);
  check(links[0][1] === "https://buy.stripe.com/test_123" && /btn-primary/.test(links[0][2]), "first configured link is the primary button");
  // The app opens #support in a fresh browser tab, so test exactly that.
  const fresh = await ctx.newPage();
  await fresh.route("**/config.js", (r) => r.fulfill({ contentType: "text/javascript", body: cfg }));
  await fresh.goto(BASE + "#support", { waitUntil: "networkidle" });
  await fresh.evaluate(() => document.fonts.ready);
  await fresh.waitForTimeout(800);
  const top = await fresh.$eval("#support", (el) => el.getBoundingClientRect().top);
  await fresh.screenshot({ path: `${OUT}support-landing.png` });
  check(top >= 0 && top < 140, `#support anchor (the app's link) lands on the section (top ${Math.round(top)}px)`);
  await p.screenshot({ path: `${OUT}support-configured.png` });
  await ctx.close();
}

// 5. Live tiles do what they say.
{
  const { p, ctx } = await page({ viewport: { width: 1280, height: 900 } });
  await p.click("#drop");
  check(/Stremio/.test(await p.textContent("#drop-full")), "drop zone: sample loads on click");
  await p.click("#drop-reset");
  check(await p.isVisible("#drop-empty"), "drop zone: Remove restores the empty state");
  await p.focus("#code-input");
  await p.keyboard.type("12a3456");
  check((await p.inputValue("#code-input")) === "123456", "code: digits only, max six");
  check(/signed in/i.test(await p.textContent("#code-status")), "code: completes on the sixth digit");
  await p.click("#job-run");
  await p.waitForSelector("#job.ok", { timeout: 8000 });
  check((await p.textContent("#job-detail")) === "Stremio is installed on Living Room", "install: runs every stage to success");
  check((await p.textContent("#job-pct")) === "100%", "install: ends at 100%");
  const c1 = await p.textContent("#count");
  await p.waitForTimeout(1500);
  const c2 = await p.textContent("#count");
  check(c1 !== c2, `countdown ticks (${c1} → ${c2})`);
  await p.click("#tab-apps");
  check((await p.getAttribute("#screen-img", "src")) === "images/my-apps-light.jpg", "screens: tabs switch the image");
  await p.focus("#tab-apps");
  await p.keyboard.press("ArrowLeft");
  check((await p.getAttribute("#tab-devices", "aria-selected")) === "true", "screens: arrow keys move between tabs");
  const imgsOk = await p.$$eval("img", (is) => is.filter((i) => i.loading !== "lazy" || i.complete).every((i) => !i.complete || i.naturalWidth > 0));
  check(imgsOk, "images decode");
  await ctx.close();
}

// 6. Reduced motion: tiles still work, and the countdown holds still.
{
  const { p, ctx, errors } = await page({ reduced: true });
  await p.click("#job-run");
  await p.waitForSelector("#job.ok", { timeout: 4000 });
  check(errors.length === 0, "reduced motion: install tile completes without errors");
  const a = await p.textContent("#count");
  await p.waitForTimeout(1500);
  check(a === (await p.textContent("#count")) && a !== "Expired", `reduced motion: countdown is a still frame (${a})`);
  await ctx.close();
}

// 7. The countdown keeps exactly one animation loop across tab hides and shows.
{
  const { p, ctx } = await page();
  await p.evaluate(() => {
    window.__raf = 0;
    const raf = window.requestAnimationFrame.bind(window);
    window.requestAnimationFrame = (f) => { window.__raf++; return raf(f); };
  });
  for (let i = 0; i < 3; i++) {
    await p.evaluate(() => {
      Object.defineProperty(document, "hidden", { configurable: true, get: () => false });
      document.dispatchEvent(new Event("visibilitychange"));
    });
  }
  await p.evaluate(() => (window.__raf = 0));
  await p.waitForTimeout(1000);
  const perSecond = await p.evaluate(() => window.__raf);
  check(perSecond <= 75, `countdown: one loop after repeated tab switches (${perSecond} frames/s)`);
  await ctx.close();
}

await browser.close();
console.log(`\nScreenshots: ${OUT}`);
console.log(failures.length ? `${failures.length} FAILED` : "ALL PASSED");
process.exit(failures.length ? 1 : 0);
