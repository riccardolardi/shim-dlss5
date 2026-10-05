# shim website

One static page, no build step, no JavaScript. Deployed on Vercel.

## Deploy (first time)

1. vercel.com → **Add New… → Project** → import `riccardolardi/shim-dlss5`
   (the repo must be visible to Vercel; a private repo works with the GitHub
   integration).
2. **Root Directory**: `site`. Framework preset: **Other**. Build command: none.
   Output directory: leave empty (the folder itself is served).
3. Deploy. Every later push to `main` that touches `site/` redeploys.

`vercel.json` adds clean URLs and security headers; the CSP allows only this
origin, Google Fonts, and inline styles. No scripts run on the page.

## Result reports (`/api/report`, `/games`)

The app's "Share this result" POSTs one anonymous report to `/api/report`
(`api/report.js`, validated by `lib/validate.js`); `/games` and
`/games/<key>` are server-rendered by `api/games.js` from Postgres and cached at
the edge for ten minutes. A game is listed from two reports.

Setup, once:

1. Vercel project → **Storage → Create Database → Neon** (Marketplace), connect it
   to this project. That sets `DATABASE_URL`. Tables are created on first use.
2. Project → Settings → Environment Variables → add `REPORT_SALT` with a long
   random value (e.g. `openssl rand -hex 32`). It keys the daily IP hash used
   for rate limiting; rotating it is harmless.
3. Redeploy.

Moderation: delete rows in the Neon console (`delete from reports where id = …`).
Limits live in `lib/db.js` (`PER_HOUR`, `PER_GAME_PER_DAY`, `MIN_REPORTS`).
What a report contains is described in `privacy.html` — change both together.

Tests: `npm ci && npm test` (in `site/`) runs validation, rendering, the
handlers, and the real SQL against PGlite. CI runs them too.

## Local preview

Open `site/index.html` in a browser, or `npx serve site`.

## When the domain changes

Replace `https://shim-dlss5.vercel.app` in `index.html` (canonical, `og:url`,
`og:image`, `twitter:image`, the JSON-LD `url`), `robots.txt` and
`sitemap.xml`. Keep the old host as a Vercel alias so posted links keep working.

## SEO notes

- On-page: keyword-bearing `<title>`/`<h2>`s, meta description, canonical,
  Open Graph + Twitter card with `og.png` (1200×630, rendered by hand), JSON-LD
  `SoftwareApplication` + `FAQPage`, `robots.txt`, `sitemap.xml`, no scripts.
- The levers that actually move a one-page site: the public GitHub README, a
  Nexus Mods page, forum/Reddit posts that link here. A per-game list (long-tail "<game> DLSS 5" searches) is a later option.
- Regenerate `og.png` after a tagline change (see the PowerShell snippet in the
  git history of this folder) and bump `softwareVersion` in the JSON-LD on
  each release.

## Content rules

- Every claim on the page is something the app does today. Update the page in
  the same commit as the feature.
- Never link to a model or add-on download. The page says where they come from
  and that shim records their hash; nothing more.
- Sponsor link: `https://ko-fi.com/riccardolardi` (same as in-app).
