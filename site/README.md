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
  Nexus Mods page, forum/Reddit posts that link here, and the "Tested games"
  table (long-tail "<game> DLSS 5" searches). Add a row per real report.
- Regenerate `og.png` after a tagline change (see the PowerShell snippet in the
  git history of this folder) and bump `softwareVersion` in the JSON-LD on
  each release.

## Content rules

- Every claim on the page is something the app does today. Update the page in
  the same commit as the feature.
- Never link to a model or add-on download. The page says where they come from
  and that shim records their hash; nothing more.
- Sponsor link: `https://ko-fi.com/riccardolardi` (same as in-app).
