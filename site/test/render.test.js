import { test } from "node:test";
import assert from "node:assert/strict";
import { esc, renderList, renderDetail, routeLabel } from "../lib/render.js";

const row = { game_key: "steam-1", title: "<script>alert(1)</script>", n: 3, works: 2, no_effect: 0, crashes: 1, last: "2026-10-05T10:00:00Z" };

test("escaping covers every HTML-significant character", () => {
  assert.equal(esc(`<a href="x" onclick='y'>&</a>`), "&lt;a href=&quot;x&quot; onclick=&#39;y&#39;&gt;&amp;&lt;/a&gt;");
  assert.equal(esc(null), "");
});

test("the list page escapes titles and links to the game key", () => {
  const html = renderList([row]);
  assert.ok(!html.includes("<script>alert"));
  assert.ok(html.includes("&lt;script&gt;"));
  assert.ok(html.includes('href="/games/steam-1"'));
  assert.ok(html.includes("2 works · 0 no effect · 1 crash"));
  assert.ok(!/<script\b/i.test(html), "no scripts on the page");
});

test("an empty list explains how results get there", () => {
  assert.ok(renderList([]).includes("Share this result"));
});

test("the detail page shows routes, GPUs and recent reports, escaped", () => {
  const html = renderDetail({
    summary: row,
    setups: [{ route: "optiscaler_dlssnr", before_upscale: true, n: 3, works: 2, no_effect: 0, crashes: 1 }],
    gpus: [{ gpu_name: "RTX <5070>", n: 3, works: 2, no_effect: 0, crashes: 1 }],
    recent: [
      { created_at: "2026-10-05T10:00:00Z", route: "reshade_renodx", before_upscale: null, gpu_name: "RTX 5070", gpu_vram_mb: 12227, gpu_driver: "616.56", game_dlss: "310.6.0.0", components: [["reshade", "6.8.0"]], model_signed: true, outcome: "crashes", app_version: "0.1.4" },
    ],
  });
  assert.ok(html.includes("OptiScaler + DLSS 5, before upscaling"));
  assert.ok(html.includes("RTX &lt;5070&gt;"));
  assert.ok(html.includes("12 GB"));
  assert.ok(html.includes("reshade 6.8.0"));
  assert.ok(html.includes('class="bad">Crashes'));
});

test("route labels", () => {
  assert.equal(routeLabel("reshade_renodx", null), "DLSS 5 (ReShade + RenoDX)");
  assert.equal(routeLabel("optiscaler_dlssnr", false), "OptiScaler + DLSS 5, after upscaling");
});
