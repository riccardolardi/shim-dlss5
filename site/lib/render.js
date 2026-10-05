// Server-rendered HTML for /games and /games/<key>. No scripts; the shared
// stylesheet is /style.css. Every value from the database goes through esc().

const SITE = "https://shim-dlss5.vercel.app";

export function esc(v) {
  return String(v ?? "")
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

export const ROUTE_LABEL = {
  reshade_renodx: "DLSS 5 (ReShade + RenoDX)",
  reshade_feeder: "DLSS 5 (ReShade + Feeder + RenoDX)",
  optiscaler_dlssnr: "OptiScaler + DLSS 5",
  optiscaler: "OptiScaler only",
  reshade_vulkan: "ReShade (Vulkan)",
};

const OUTCOME_LABEL = { works: "Works", no_effect: "No visible effect", crashes: "Crashes" };

export function routeLabel(route, beforeUpscale) {
  const base = ROUTE_LABEL[route] ?? route;
  if (route !== "optiscaler_dlssnr" || beforeUpscale === null || beforeUpscale === undefined) return base;
  return `${base}, ${beforeUpscale ? "before" : "after"} upscaling`;
}

function date(d) {
  return new Date(d).toISOString().slice(0, 10);
}

/** Stacked bar plus counts, readable without colour. */
function bar(row) {
  const n = row.n || 1;
  const pct = (k) => ((row[k] / n) * 100).toFixed(1);
  return `<div class="tally">
    <div class="bar" role="img" aria-label="${row.works} works, ${row.no_effect} no visible effect, ${row.crashes} crashes">
      <span class="b-works" style="width:${pct("works")}%"></span><span class="b-none" style="width:${pct("no_effect")}%"></span><span class="b-crash" style="width:${pct("crashes")}%"></span>
    </div>
    <span class="mono counts">${row.works} works · ${row.no_effect} no effect · ${row.crashes} crash${row.crashes === 1 ? "" : "es"}</span>
  </div>`;
}

const PAGE_CSS = `
  .tally { display: grid; gap: 4px; min-width: 0; }
  .bar { display: flex; height: 8px; border-radius: 4px; overflow: hidden; background: var(--bg-2); max-width: 260px; }
  .bar span { display: block; height: 100%; }
  .b-works { background: var(--accent); }
  .b-none { background: var(--fg-3); }
  .b-crash { background: #dc4a4a; }
  .counts { font-size: 12px; color: var(--fg-3); }
  .ok { color: var(--accent); } .bad { color: #dc4a4a; } .meh { color: var(--fg-3); }
  td a { color: var(--fg); }
  .note { font-size: 14px; color: var(--fg-3); margin-top: 16px; max-width: 70ch; }
`;

export function layout({ title, description, path, body }) {
  return `<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
<title>${esc(title)}</title>
<meta name="description" content="${esc(description)}">
<link rel="canonical" href="${SITE}${path}">
<meta property="og:title" content="${esc(title)}">
<meta property="og:description" content="${esc(description)}">
<meta property="og:url" content="${SITE}${path}">
<meta property="og:image" content="${SITE}/og.png">
<meta name="twitter:card" content="summary_large_image">
<link rel="icon" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' viewBox='0 0 32 32'%3E%3Crect width='32' height='32' rx='7' fill='%23101612'/%3E%3Ccircle cx='16' cy='16' r='7' fill='%234ade80'/%3E%3C/svg%3E">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;500;600&family=IBM+Plex+Mono:wght@400;500&display=swap">
<link rel="stylesheet" href="/style.css">
<style>${PAGE_CSS}</style>
</head>
<body>
<div class="wrap">
  <header class="top">
    <a class="brand" href="/"><i aria-hidden="true"></i>shim-dlss5</a>
    <nav aria-label="Sections">
      <a href="/#how">How it works</a>
      <a href="/games">Games</a>
      <a href="/#support">Support</a>
      <a href="https://github.com/riccardolardi/shim-dlss5">GitHub</a>
    </nav>
  </header>
${body}
  <footer>
    <span>Results are reported by users of the app and are not verified. DLSS and NVIDIA are trademarks of NVIDIA Corporation; shim-dlss5 is not affiliated with NVIDIA.</span>
    <span><a href="/privacy">Privacy</a> · <a href="https://github.com/riccardolardi/shim-dlss5/releases/latest">Download</a> · <a href="https://ko-fi.com/riccardolardi">Ko-fi</a></span>
  </footer>
</div>
</body>
</html>`;
}

export function renderList(rows) {
  const table = rows.length
    ? `<div class="tablewrap"><table>
        <thead><tr><th>Game</th><th>Reports</th><th>Results</th><th>Latest</th></tr></thead>
        <tbody>${rows
          .map(
            (r) => `<tr>
              <td><a href="/games/${esc(r.game_key)}">${esc(r.title)}</a></td>
              <td class="mono">${r.n}</td>
              <td>${bar(r)}</td>
              <td class="mono">${date(r.last)}</td>
            </tr>`,
          )
          .join("")}</tbody></table></div>`
    : `<p class="sub">No game has two reports yet. Install a game with the app, run it, and use "Share this result" on its page.</p>`;
  return layout({
    title: "DLSS 5 game results — shim-dlss5",
    description: "Which games run DLSS 5 neural rendering with shim-dlss5, by route and GPU, as reported by users of the app.",
    path: "/games",
    body: `<section style="border-top:0">
    <div class="eyebrow">Reported by users</div>
    <h1 style="margin-top:10px;font-size:clamp(28px,4vw,40px)">DLSS 5 game results</h1>
    <p class="sub" style="margin-top:12px">Each row is a game at least two people have run with shim-dlss5 and reported from the app. Results depend on the GPU, the driver and the model build, so open a game for the breakdown.</p>
    ${table}
    <p class="note">Reports are anonymous and unverified; a single report is not shown. How reports work: <a href="/privacy">privacy</a>.</p>
  </section>`,
  });
}

export function renderDetail({ summary, setups, gpus, recent }) {
  const outcome = (o) => `<span class="${o === "works" ? "ok" : o === "crashes" ? "bad" : "meh"}">${esc(OUTCOME_LABEL[o] ?? o)}</span>`;
  const comps = (c) => (Array.isArray(c) ? c.map(([id, v]) => `${esc(id)} ${esc(v)}`).join(", ") : "");
  return layout({
    title: `${summary.title} with DLSS 5 — shim-dlss5 results`,
    description: `${summary.n} user reports of ${summary.title} with DLSS 5 neural rendering via shim-dlss5: ${summary.works} work, ${summary.crashes} crash. Breakdown by route and GPU.`,
    path: `/games/${summary.game_key}`,
    body: `<section style="border-top:0">
    <div class="eyebrow"><a href="/games">All games</a> · ${summary.n} reports · latest ${date(summary.last)}</div>
    <h1 style="margin-top:10px;font-size:clamp(28px,4vw,40px)">${esc(summary.title)}</h1>
    <div style="margin-top:16px">${bar(summary)}</div>
  </section>
  <section>
    <h2>By route</h2>
    <div class="tablewrap"><table>
      <thead><tr><th>Route</th><th>Reports</th><th>Results</th></tr></thead>
      <tbody>${setups.map((s) => `<tr><td>${esc(routeLabel(s.route, s.before_upscale))}</td><td class="mono">${s.n}</td><td>${bar(s)}</td></tr>`).join("")}</tbody>
    </table></div>
  </section>
  <section>
    <h2>By GPU</h2>
    <div class="tablewrap"><table>
      <thead><tr><th>GPU</th><th>Reports</th><th>Results</th></tr></thead>
      <tbody>${gpus.map((g) => `<tr><td>${esc(g.gpu_name)}</td><td class="mono">${g.n}</td><td>${bar(g)}</td></tr>`).join("")}</tbody>
    </table></div>
  </section>
  <section>
    <h2>Latest reports</h2>
    <div class="tablewrap"><table>
      <thead><tr><th>Date</th><th>Result</th><th>Route</th><th>GPU</th><th>Driver</th><th>Game DLSS</th><th>Components</th></tr></thead>
      <tbody>${recent
        .map(
          (r) => `<tr>
            <td class="mono">${date(r.created_at)}</td>
            <td>${outcome(r.outcome)}</td>
            <td>${esc(routeLabel(r.route, r.before_upscale))}</td>
            <td>${esc(r.gpu_name ?? "")}${r.gpu_vram_mb ? ` <span class="mono" style="color:var(--fg-3)">${Math.round(r.gpu_vram_mb / 1024)} GB</span>` : ""}</td>
            <td class="mono">${esc(r.gpu_driver ?? "")}</td>
            <td class="mono">${esc(r.game_dlss ?? "")}</td>
            <td class="mono" style="font-size:12px">${comps(r.components)}</td>
          </tr>`,
        )
        .join("")}</tbody>
    </table></div>
    <p class="note">Unverified user reports. A crash in one setup says little about another GPU or model build.</p>
  </section>`,
  });
}

export function renderNotFound() {
  return layout({
    title: "No results yet — shim-dlss5",
    description: "This game does not have enough reports yet.",
    path: "/games",
    body: `<section style="border-top:0"><h1>No results for this game yet</h1><p class="sub" style="margin-top:12px">A game is shown once two people have reported it from the app. <a href="/games">All games</a></p></section>`,
  });
}

export function renderError() {
  return layout({
    title: "Results unavailable — shim-dlss5",
    description: "The results page could not be loaded.",
    path: "/games",
    body: `<section style="border-top:0"><h1>Results are unavailable right now</h1><p class="sub" style="margin-top:12px">The database did not answer. Try again in a minute.</p></section>`,
  });
}
