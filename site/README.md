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

## Content rules

- Every claim on the page is something the app does today. Update the page in
  the same commit as the feature.
- Never link to a model or add-on download. The page says where they come from
  and that shim records their hash; nothing more.
- Sponsor link: `https://github.com/sponsors/riccardolardi` (same as in-app).
