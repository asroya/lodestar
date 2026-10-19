// Lodestar's site: release lookup, support links and the four live tiles.
// No cookies, no analytics. The only request this script makes is one call to
// GitHub's public API for the latest release, cached for the browser session.
"use strict";

const REPO = "asroya/lodestar";
const RELEASES = `https://github.com/${REPO}/releases/latest`;
const reduced = matchMedia("(prefers-reduced-motion: reduce)").matches;
const $ = (s, el = document) => el.querySelector(s);
const $$ = (s, el = document) => [...el.querySelectorAll(s)];

// ── header hairline once the page scrolls ──
const head = $(".site-head");
const onScroll = () => head.classList.toggle("scrolled", scrollY > 8);
addEventListener("scroll", onScroll, { passive: true });
onScroll();

// ── download: the disk image if the latest release has one, else the zip,
//    else the releases page (which is also the no-JavaScript link) ──
function pickAsset(release) {
  const assets = (release && release.assets) || [];
  return assets.find((a) => /\.dmg$/i.test(a.name)) || assets.find((a) => /\.zip$/i.test(a.name)) || null;
}
function megabytes(bytes) {
  return `${Math.max(1, Math.round(bytes / 1e6))} MB`;
}
function applyRelease(release) {
  const asset = pickAsset(release);
  if (release && release.tag_name) $$("[data-version]").forEach((el) => (el.textContent = release.tag_name));
  if (!asset) return;
  $$("[data-download]").forEach((a) => (a.href = asset.browser_download_url));
  const kind = /\.dmg$/i.test(asset.name) ? "disk image" : "zip";
  $$("[data-download-meta]").forEach((el) => {
    el.textContent = `${release.tag_name} · ${megabytes(asset.size)} ${kind}`;
    el.hidden = false;
  });
}
async function resolveDownload() {
  const key = "lodestar-release";
  try {
    const cached = JSON.parse(sessionStorage.getItem(key) || "null");
    if (cached) return applyRelease(cached);
  } catch {}
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (!res.ok) return;
    const r = await res.json();
    const slim = {
      tag_name: r.tag_name,
      assets: (r.assets || []).map(({ name, size, browser_download_url }) => ({ name, size, browser_download_url })),
    };
    try { sessionStorage.setItem(key, JSON.stringify(slim)); } catch {}
    applyRelease(slim);
  } catch {
    // Offline or rate-limited: the links already point at the releases page.
  }
}
resolveDownload();

// ── support links from config.js ──
(function supportLinks() {
  const box = $("#give");
  const ways = (window.LODESTAR_SUPPORT || []).filter((w) => /^https:\/\//.test(w.url || ""));
  if (!ways.length) {
    $("#give-empty").hidden = false;
    box.hidden = true;
    return;
  }
  ways.forEach((w, i) => {
    const a = document.createElement("a");
    a.className = `btn ${i === 0 ? "btn-primary" : "btn-secondary"}`;
    a.href = w.url;
    a.rel = "noopener";
    a.textContent = w.label;
    if (w.note) {
      const s = document.createElement("span");
      s.className = "sub";
      s.textContent = w.note;
      a.append(s);
    }
    box.append(a);
  });
})();

// ── screens ──
(function screens() {
  const tabs = $$(".tab");
  const img = $("#screen-img");
  const cap = $("#screen-caption");
  const panel = $("#screen");
  // Warm the cache so switching never flashes.
  tabs.forEach((t) => { const i = new Image(); i.src = t.dataset.src; });
  function select(tab) {
    tabs.forEach((t) => {
      const on = t === tab;
      t.setAttribute("aria-selected", on);
      t.tabIndex = on ? 0 : -1;
    });
    img.src = tab.dataset.src;
    img.alt = tab.dataset.alt;
    cap.textContent = tab.dataset.caption;
    panel.setAttribute("aria-labelledby", tab.id);
  }
  tabs.forEach((t, i) => {
    t.tabIndex = i === 0 ? 0 : -1;
    t.addEventListener("click", () => select(t));
    t.addEventListener("keydown", (e) => {
      const d = e.key === "ArrowRight" ? 1 : e.key === "ArrowLeft" ? -1 : 0;
      if (!d) return;
      const next = tabs[(i + d + tabs.length) % tabs.length];
      next.focus();
      select(next);
    });
  });
})();

// ── tile: drop zone ──
(function dropZone() {
  const stage = $("#drop-stage");
  const zone = $("#drop");
  const empty = $("#drop-empty");
  const full = $("#drop-full");
  const sample = $("#sample");
  const SAMPLE = { name: "Stremio", bundle: "com.stremio.pal", version: "2.0.8", platform: "tvOS 17.0 or later", size: "38.4 MB", sample: true };

  function row(k, v) {
    return `<dt class="label">${k}</dt><dd class="data">${v}</dd>`;
  }
  function esc(s) {
    return String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);
  }
  function show(info) {
    empty.hidden = true;
    full.hidden = false;
    zone.style.textAlign = "left";
    full.innerHTML = `
      <div class="app-name">${esc(info.name)}</div>
      <dl class="kv">${info.sample
        ? row("Bundle", info.bundle) + row("Version", info.version) + row("Platform", info.platform) + row("Size", info.size)
        : row("File", esc(info.file)) + row("Size", info.size) + row("Platform", info.platform)}</dl>
      <div class="chip-row"><button type="button" id="drop-reset">Remove</button></div>`;
    sample.hidden = true;
    $("#drop-reset").addEventListener("click", (e) => { e.stopPropagation(); reset(); });
  }
  function reset() {
    empty.hidden = false;
    full.hidden = true;
    full.innerHTML = "";
    zone.style.textAlign = "";
    sample.hidden = false;
  }
  function fromFile(f) {
    const ext = (f.name.split(".").pop() || "").toLowerCase();
    if (ext !== "ipa" && ext !== "tipa") {
      empty.innerHTML = `<strong>That's a .${esc(ext) || "file"}</strong><span>Lodestar takes .ipa and .tipa files. Try the sample.</span>`;
      return;
    }
    show({
      name: f.name.replace(/\.(t?ipa)$/i, ""),
      file: f.name,
      size: `${(f.size / 1e6).toFixed(1)} MB`,
      platform: ext === "tipa" ? "tvOS" : "iOS or iPadOS",
    });
  }
  let depth = 0;
  stage.addEventListener("dragenter", (e) => { e.preventDefault(); depth++; zone.classList.add("over"); });
  stage.addEventListener("dragleave", () => { if (--depth <= 0) { depth = 0; zone.classList.remove("over"); } });
  stage.addEventListener("dragover", (e) => e.preventDefault());
  stage.addEventListener("drop", (e) => {
    e.preventDefault();
    depth = 0;
    zone.classList.remove("over");
    const f = e.dataTransfer.files && e.dataTransfer.files[0];
    if (f) fromFile(f);
    else if (e.dataTransfer.getData("text/plain") === "lodestar-sample") show(SAMPLE);
  });
  sample.addEventListener("dragstart", (e) => e.dataTransfer.setData("text/plain", "lodestar-sample"));
  const activate = () => { if (full.hidden) show(SAMPLE); };
  zone.addEventListener("click", activate);
  zone.addEventListener("keydown", (e) => { if (e.key === "Enter" || e.key === " ") { e.preventDefault(); activate(); } });
})();

// ── tile: six-digit code ──
(function codeTile() {
  const wrap = $("#code");
  const input = $("#code-input");
  const cells = $$(".cell", wrap);
  const status = $("#code-status");
  let timer;
  function render() {
    const v = input.value;
    const focused = document.activeElement === input;
    cells.forEach((c, i) => {
      const ch = v[i];
      if (ch && c.dataset.ch !== ch) c.innerHTML = `<span class="ch">${ch}</span>`;
      if (!ch) c.innerHTML = "";
      c.dataset.ch = ch || "";
      c.classList.toggle("active", focused && i === Math.min(v.length, 5) && v.length < 6);
    });
    const done = v.length === 6;
    wrap.classList.toggle("done", done);
    status.classList.toggle("ok", done);
    status.textContent = done ? "Code accepted. You're signed in." : v.length ? `${6 - v.length} to go.` : "Type any six digits.";
    clearTimeout(timer);
    if (done) timer = setTimeout(() => { input.value = ""; render(); }, 2600);
  }
  input.addEventListener("input", () => { input.value = input.value.replace(/\D/g, "").slice(0, 6); render(); });
  input.addEventListener("focus", render);
  input.addEventListener("blur", render);
})();

// ── tile: install job (the real stage names and messages from the pipeline) ──
(function jobTile() {
  const job = $("#job");
  const bar = $("#job-bar");
  const fill = $("#job-bar i");
  const pct = $("#job-pct");
  const detail = $("#job-detail");
  const btn = $("#job-run");
  const labels = $$("[data-stage]", job);
  const script = [
    [0, 0.02, "Connecting to Living Room…"],
    [1, 0.08, "Registering device with your team…"],
    [1, 0.18, "Unpacking app…"],
    [1, 0.24, "Registering App IDs…"],
    [1, 0.32, "Downloading provisioning profiles…"],
    [2, 0.4, "Signing…"],
    [3, 0.62, "Sending to Living Room…"],
    [3, 0.86, "Installing…"],
  ];
  let running = false;
  function set(stage, frac, msg) {
    fill.style.width = `${Math.round(frac * 100)}%`;
    pct.textContent = `${Math.round(frac * 100)}%`;
    detail.textContent = msg;
    labels.forEach((l, i) => {
      l.classList.toggle("now", i === stage);
      l.classList.toggle("passed", i < stage);
    });
  }
  function reset() {
    job.classList.remove("ok");
    set(-1, 0, "Ready when you are.");
    btn.textContent = "Install";
  }
  btn.addEventListener("click", async () => {
    if (running) return;
    if (job.classList.contains("ok")) return reset();
    running = true;
    btn.disabled = true;
    bar.classList.add("live");
    const step = reduced ? 120 : 520;
    for (const [s, f, m] of script) {
      set(s, f, m);
      await new Promise((r) => setTimeout(r, step));
    }
    set(4, 1, "Stremio is installed on Living Room");
    bar.classList.remove("live");
    job.classList.add("ok");
    btn.textContent = "Run again";
    btn.disabled = false;
    running = false;
  });
})();

// ── tile: seven-day countdown, 10,000× real time ──
(function countdown() {
  const SPEED = 10000;
  const WEEK = 7 * 86400;
  const DUE = 2 * 86400;
  const el = $("#count");
  const meter = $("#meter");
  const state = $("#count-state");
  const note = $("#count-note");
  let start = performance.now();
  function fmt(s) {
    if (s <= 0) return "Expired";
    const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
    return d > 0 ? `${d}d ${String(h).padStart(2, "0")}h ${String(m).padStart(2, "0")}m` : `${h}h ${String(m).padStart(2, "0")}m`;
  }
  function tick(now) {
    const left = Math.max(0, WEEK - ((now - start) / 1000) * SPEED);
    el.textContent = fmt(left);
    const due = left < DUE;
    el.classList.toggle("due", due);
    meter.style.width = `${(left / WEEK) * 100}%`;
    state.textContent = left <= 0 ? "Expired" : due ? "Due" : "Signed";
    note.textContent = left <= 0 ? "Refresh to sign it again" : due ? "Refresh before it runs out" : "Expires in 7 days";
    if (!document.hidden) requestAnimationFrame(tick);
  }
  document.addEventListener("visibilitychange", () => { if (!document.hidden) requestAnimationFrame(tick); });
  $("#count-refresh").addEventListener("click", () => { start = performance.now(); });
  if (reduced) {
    // A still frame two days out, refreshed by the button.
    start = performance.now() - ((WEEK - 1.5 * 86400) / SPEED) * 1000;
  }
  requestAnimationFrame(tick);
})();

// ── copy buttons ──
$$("[data-copy]").forEach((b) => {
  b.addEventListener("click", async () => {
    const text = $("#" + b.dataset.copy).innerText.replace(/\s+# .*$/gm, "").trim();
    try {
      await navigator.clipboard.writeText(text);
      b.textContent = "Copied";
      b.classList.add("done");
    } catch {
      const r = document.createRange();
      r.selectNodeContents($("#" + b.dataset.copy));
      const sel = getSelection();
      sel.removeAllRanges();
      sel.addRange(r);
      b.textContent = "Selected";
    }
    setTimeout(() => { b.textContent = "Copy"; b.classList.remove("done"); }, 1800);
  });
});
