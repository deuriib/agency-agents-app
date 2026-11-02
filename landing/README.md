# landing

Static landing page for Agency Agents, served at
**https://deuriib.github.io/agency-agents-app/** as a GitHub Pages project site.

A self-contained static site (no build step). GitHub Actions uploads this
directory as the Pages artifact, so nothing on a private host is involved.

## Files

- `index.html` — the page
- `style.css` — design tokens (dark-first, OKLCH, indigo accent)
- `agency-agents.png` — hero icon (Liquid Glass app icon)
- `favicon.ico`, `favicon-16x16.png`, `favicon-32x32.png`, `apple-touch-icon.png`, `icon-192.png`, `icon-512.png`
- `social-card.png` — 1200×630 Open Graph / Twitter card
- `manifest.json`, `robots.txt`, `sitemap.xml`

Icons are generated from `../docs/icon/agency-icon-*.png` via `sips`; `favicon.ico` is `../src-tauri/icons/icon.ico`.

> **TODO:** add an app screenshot to `screenshots/` and uncomment the `.section-shots`
> block in `index.html`. Capture the Dashboard (cross-tool coverage) for the strongest shot.

## Subpath rule

Pages serves this project site from `/agency-agents-app/`, not from the domain
root. Every asset reference therefore has to be **document-relative**
(`./style.css`, `./screenshots/dashboard-dark.png`, `./manifest.json`) — a
root-absolute `/style.css` resolves to `https://deuriib.github.io/style.css`
and 404s.

Relative paths also keep the page working unchanged if it is ever served from a
domain root (custom domain, local preview). Absolute URLs are still correct in
exactly one place: `canonical`, `og:url`, `og:image`, `twitter:url`,
`twitter:image`, and the JSON-LD `url`/`image`, which must be fully qualified.

## Deploy

Automatic. `.github/workflows/pages.yml` deploys on any push to `main` that
touches `landing/**` or the workflow file, plus `workflow_dispatch` for a manual
run.

The one-time setup is Settings → Pages → Source = "GitHub Actions"; after that,
the deploy is just a merge. The workflow cannot perform that step itself —
`configure-pages` documents that `enablement` needs a token other than the
default `GITHUB_TOKEN`.

## Update flow

1. Edit `index.html` / `style.css` locally
2. View locally: `python3 -m http.server -d . 8089` then open `http://localhost:8089`
3. Open a PR. Merging to `main` triggers the deploy.
4. Verify at `https://deuriib.github.io/agency-agents-app/` — check the Network
   panel for 404s and DevTools → Application → Manifest for `start_url`.

## What no longer lives here

This directory used to be `rsync`ed to a private build host where Caddy served it
at a custom domain. That host is retired for the site, and so is its
`updater.json` endpoint — update checks now read the signed `latest.json`
attached to the GitHub Release (see `../docs/updater-release.md`).
