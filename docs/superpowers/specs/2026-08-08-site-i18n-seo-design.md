# Design: GitHub Pages Site — Bilingual (EN/zh) + SEO

**Date:** 2026-08-08  
**Branch:** `feature/site-i18n-seo`  
**Status:** Approved  
**Deploy base:** `https://flyxl.github.io/datazen/`

## Goals

1. **SEO first:** make the marketing site discoverable and shareable (canonical, hreflang, full OG/Twitter, sitemap, robots, homepage JSON-LD).
2. **Bilingual:** English at site root; Simplified Chinese under `/zh/`; manual language switch in nav.
3. **No OS-language auto-redirect:** root always serves English; users switch via UI.

## Decisions (approved)

| Topic | Choice |
|-------|--------|
| URL structure | Path-based: `/` = English, `/zh/` = Chinese |
| Root behavior | Fixed English content; **no** auto-redirect |
| OS language detection | **None** (user switches manually) |
| Implementation | Duplicate static HTML (Approach A); shared `assets/` |
| Language preference storage | None — URL is the source of truth |
| Scope of SEO | Full-site (not homepage-only) |
| Locales in v1 | `en` + `zh-CN` only |
| Homepage video | **Removed site-wide** — no 「60 秒快速了解」 / demo video on any page |

## Current state

- Static site under `site/` (7 HTML pages + `assets/css`, `assets/js`, screenshots).
- Homepage 「60 秒快速了解」 demo video block and `assets/video/` **removed** (no video module on any page).
- All copy hardcoded Chinese; nav/footer injected by `site/assets/js/site.js`.
- Basic SEO today: per-page `title` + `description`; OG/Twitter **only on homepage**; no canonical, hreflang, sitemap, robots, or JSON-LD.
- Deploy: `.github/workflows/pages.yml` uploads `site/` on `main` pushes.

## Directory layout

```
site/
├── index.html, features.html, ai.html, charts.html,
│   workflow.html, databases.html, download.html   # English
├── zh/
│   └── (same filenames)                           # Chinese (migrate existing)
├── assets/                                        # shared CSS/JS/media
├── robots.txt
├── sitemap.xml
└── .nojekyll
```

| Public URL | Language |
|------------|----------|
| `/datazen/`, `/datazen/features.html`, … | English |
| `/datazen/zh/`, `/datazen/zh/features.html`, … | Chinese |

Asset hrefs: English pages use `assets/…`; Chinese pages use `../assets/…`.

## Language switcher & shared JS

- Extend `site.js` to detect locale from pathname (`/zh/` → `zh`, else `en`).
- Render nav + footer strings from `en` / `zh` dictionaries.
- Add language control next to CTA:
  - On EN page: link labeled `中` → counterpart under `/zh/`
  - On ZH page: link labeled `EN` → counterpart at root
- Resolve counterpart URL relative to GitHub Pages base (`/datazen`), preserving filename (treat `/` and empty as `index.html`).
- Do **not** write `localStorage` for locale.

## SEO requirements (every HTML page)

Absolute URL prefix: `https://flyxl.github.io/datazen`

1. `<html lang="en">` or `lang="zh-CN">`
2. Localized `<title>` and `<meta name="description">`
3. `<link rel="canonical" href="{self absolute URL}">`
4. Alternate links:
   - `hreflang="en"` → English counterpart
   - `hreflang="zh-CN"` → Chinese counterpart
   - `hreflang="x-default"` → English counterpart (root)
5. Open Graph: `og:title`, `og:description`, `og:image`, `og:url`, `og:type` (`website`), `og:locale` (`en_US` / `zh_CN`), plus `og:locale:alternate`
6. Twitter Card: `summary_large_image` + title/description/image
7. Meaningful `alt` on images; keep heading hierarchy intact

### Site-wide files

- `site/robots.txt` — allow all; `Sitemap: https://flyxl.github.io/datazen/sitemap.xml`
- `site/sitemap.xml` — all 7 EN + 7 ZH URLs with `xhtml:link` alternates (or separate url entries per locale)

### Homepage structured data

- EN + ZH `index.html`: JSON-LD `SoftwareApplication` (name, description, url, license GPLv3, OS macOS/Windows, offers free).

## Content migration

1. Move current Chinese HTML bodies into `site/zh/*.html`; fix asset/script paths to `../assets/…`.
2. Author English HTML at `site/*.html` with full translation of visible copy and meta.
3. Do **not** reintroduce any 「60 秒快速了解」 / demo video section when building EN or ZH pages; `assets/video/` is deleted.
4. Keep screenshots shared (no locale-specific media in v1).
5. Update any internal docs that still say `docs/` is the Pages root (e.g. `docs/marketing/GITHUB_PAGES.md`) only if touched for accuracy — optional follow-up, not blocking.

## Deploy / CI

- No workflow change required if artifact path remains `site/`.
- Pages still deploys only from `main`; feature branch merges when ready.

## Out of scope

- Additional locales beyond `en` / `zh-CN`
- OS/browser language auto-redirect
- Locale-specific screenshots
- Replacing the removed demo video with another media block
- Search Console / analytics wiring
- Build step / SSG

## Acceptance checklist

- [ ] Root URLs show English; `/zh/` URLs show Chinese
- [ ] Nav language switch lands on the correct counterpart page
- [ ] No auto-redirect based on `navigator.language`
- [ ] No page contains 「60 秒快速了解」 / demo video; `assets/video/` absent
- [ ] Every page has canonical + hreflang + OG/Twitter
- [ ] `robots.txt` and `sitemap.xml` list all locale URLs
- [ ] Homepage JSON-LD present on EN and ZH
- [ ] Local preview via static server works under a `/datazen` base or documented equivalent
- [ ] Work delivered on branch `feature/site-i18n-seo`

## Testing notes

- Local: from repo root, serve `site/` (or parent with path prefix) and spot-check links/meta with View Source.
- After merge to `main`: verify live Pages deploy and sample OG debugger if desired.
