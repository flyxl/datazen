#!/usr/bin/env node
/**
 * Build blog HTML pages from Markdown frontmatter articles.
 *
 * Reads:  docs/blogs/seo/*.zh-CN.md  (Chinese source)
 *         docs/blogs/seo/*.en.md     (English translations, optional)
 * Writes: site/zh/blog-{slug}.html   (Chinese)
 *         site/blog-{slug}.html       (English)
 *         site/zh/blog.html           (Chinese blog list)
 *         site/blog.html              (English blog list)
 */

import { readdirSync, readFileSync, writeFileSync, mkdirSync, existsSync } from 'node:fs';
import { join, basename } from 'node:path';
import { marked } from 'marked';

const ROOT = join(import.meta.dirname, '..');
const SEO_DIR = join(ROOT, 'docs', 'blogs', 'seo');
const SITE_DIR = join(ROOT, 'site');
const SITE_ZH_DIR = join(SITE_DIR, 'zh');
const ORIGIN = 'https://flyxl.github.io/datazen';

// ── Frontmatter parser (simple YAML subset) ────────────────────────
function parseFrontmatter(raw) {
  const m = raw.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
  if (!m) return { meta: {}, body: raw };
  const meta = {};
  for (const line of m[1].split('\n')) {
    const kv = line.match(/^(\w[\w-]*):\s*(.+)$/);
    if (kv) {
      let val = kv[2].trim();
      if (val.startsWith('"') && val.endsWith('"')) val = val.slice(1, -1);
      if (val.startsWith("'") && val.endsWith("'")) val = val.slice(1, -1);
      meta[kv[1]] = val;
    }
  }
  return { meta, body: m[2] };
}

// ── Read all articles ──────────────────────────────────────────────
const zhFiles = readdirSync(SEO_DIR).filter(f => f.endsWith('.zh-CN.md'));
const enFiles = readdirSync(SEO_DIR).filter(f => f.endsWith('.en.md'));
const enMap = new Map();
for (const f of enFiles) {
  const raw = readFileSync(join(SEO_DIR, f), 'utf-8');
  const { meta } = parseFrontmatter(raw);
  // Use frontmatter slug as key, falling back to filename
  const slugKey = meta.slug || f.replace('.en.md', '');
  enMap.set(slugKey, raw);
}

const articles = [];

for (const file of zhFiles.sort()) {
  const raw = readFileSync(join(SEO_DIR, file), 'utf-8');
  const { meta, body } = parseFrontmatter(raw);
  const slug = meta.slug || basename(file, '.zh-CN.md');

  // Check for English translation
  let enMeta = meta, enBody = body;
  if (enMap.has(slug)) {
    const enRaw = enMap.get(slug);
    const parsed = parseFrontmatter(enRaw);
    enMeta = { ...meta, ...parsed.meta };
    enBody = parsed.body;
  }

  articles.push({ file, meta, body, slug, enMeta, enBody });
}

// ── Configure marked ───────────────────────────────────────────────
marked.setOptions({ breaks: true, gfm: true });

// Rewrite relative image paths to GitHub raw URLs in the HTML output
function rewriteImagePaths(html) {
  // Already absolute — leave alone
  return html;
}

// ── Build individual blog pages ────────────────────────────────────
function buildZhPage(article) {
  const { meta, body } = article;
  const slug = meta.slug || basename(article.file, '.zh-CN.md');
  const title = meta.title || slug;
  const description = meta.description || '';
  const date = meta.date || '2026-09-01';
  const url = `${ORIGIN}/zh/blog-${slug}.html`;
  const enUrl = `${ORIGIN}/blog-${slug}.html`;
  const ogImage = `${ORIGIN}/assets/screenshots/01-main-window.png`;

  const htmlBody = marked.parse(body);

  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${escapeXml(title)} — DataZen</title>
    <meta name="description" content="${escapeXml(description)}" />
    <link rel="canonical" href="${url}" />
    <link rel="alternate" hreflang="en" href="${enUrl}" />
    <link rel="alternate" hreflang="zh-CN" href="${url}" />
    <link rel="alternate" hreflang="x-default" href="${enUrl}" />
    <meta property="og:type" content="article" />
    <meta property="og:title" content="${escapeXml(title)}" />
    <meta property="og:description" content="${escapeXml(description)}" />
    <meta property="og:url" content="${url}" />
    <meta property="og:image" content="${ogImage}" />
    <meta property="og:locale" content="zh_CN" />
    <meta property="og:locale:alternate" content="en_US" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="${escapeXml(title)}" />
    <meta name="twitter:description" content="${escapeXml(description)}" />
    <meta name="twitter:image" content="${ogImage}" />
    <link rel="icon" href="../assets/logo.png" />
    <link rel="stylesheet" href="../assets/css/site.css" />
    <script type="application/ld+json">
      {
        "@context": "https://schema.org",
        "@type": "TechArticle",
        "headline": ${JSON.stringify(title)},
        "author": { "@type": "Person", "name": "Xiaolong Wu" },
        "publisher": { "@type": "Organization", "name": "DataZen" },
        "datePublished": "${date}",
        "description": ${JSON.stringify(description)},
        "image": "${ogImage}",
        "mainEntityOfPage": "${url}"
      }
    </script>
    <script type="application/ld+json">
      {
        "@context": "https://schema.org",
        "@type": "BreadcrumbList",
        "itemListElement": [
          { "@type": "ListItem", "position": 1, "name": "首页", "item": "${ORIGIN}/zh/" },
          { "@type": "ListItem", "position": 2, "name": "博客", "item": "${ORIGIN}/zh/blog.html" },
          { "@type": "ListItem", "position": 3, "name": ${JSON.stringify(title)}, "item": "${url}" }
        ]
      }
    </script>
  </head>
  <body>
    <header id="site-header"></header>
    <section class="page-head">
      <div class="wrap">
        <p class="breadcrumb">
          <a href="apps/desktop/index.html">首页</a> /
          <a href="blog.html">博客</a>
        </p>
        <h1>${escapeXml(title)}</h1>
        <p>${escapeXml(description)}</p>
        <p class="muted" style="font-size: 13px; margin-top: 8px">${formatDateZh(date)}</p>
      </div>
    </section>
    <main class="wrap" style="max-width: 860px; padding-bottom: 72px">
      <article class="docs-prose">
${htmlBody}
      </article>
    </main>
    <footer id="site-footer"></footer>
    <script src="../assets/js/site.js"></script>
  </body>
</html>
`;
}

function buildEnPage(article) {
  const { enMeta: meta, enBody: body, slug } = article;
  const title = meta.title || slug;
  const description = meta.description || '';
  const date = meta.date || '2026-09-01';
  const url = `${ORIGIN}/blog-${slug}.html`;
  const zhUrl = `${ORIGIN}/zh/blog-${slug}.html`;
  const ogImage = `${ORIGIN}/assets/screenshots/01-main-window.png`;

  const htmlBody = marked.parse(body);

  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>${escapeXml(title)} — DataZen</title>
    <meta name="description" content="${escapeXml(description)}" />
    <link rel="canonical" href="${url}" />
    <link rel="alternate" hreflang="en" href="${url}" />
    <link rel="alternate" hreflang="zh-CN" href="${zhUrl}" />
    <link rel="alternate" hreflang="x-default" href="${url}" />
    <meta property="og:type" content="article" />
    <meta property="og:title" content="${escapeXml(title)}" />
    <meta property="og:description" content="${escapeXml(description)}" />
    <meta property="og:url" content="${url}" />
    <meta property="og:image" content="${ogImage}" />
    <meta property="og:locale" content="en_US" />
    <meta property="og:locale:alternate" content="zh_CN" />
    <meta name="twitter:card" content="summary_large_image" />
    <meta name="twitter:title" content="${escapeXml(title)}" />
    <meta name="twitter:description" content="${escapeXml(description)}" />
    <meta name="twitter:image" content="${ogImage}" />
    <link rel="icon" href="assets/logo.png" />
    <link rel="stylesheet" href="assets/css/site.css" />
    <script type="application/ld+json">
      {
        "@context": "https://schema.org",
        "@type": "TechArticle",
        "headline": ${JSON.stringify(title)},
        "author": { "@type": "Person", "name": "Xiaolong Wu" },
        "publisher": { "@type": "Organization", "name": "DataZen" },
        "datePublished": "${date}",
        "description": ${JSON.stringify(description)},
        "image": "${ogImage}",
        "mainEntityOfPage": "${url}"
      }
    </script>
  </head>
  <body>
    <header id="site-header"></header>
    <section class="page-head">
      <div class="wrap">
        <p class="breadcrumb">
          <a href="apps/desktop/index.html">Home</a> /
          <a href="blog.html">Blog</a>
        </p>
        <h1>${escapeXml(title)}</h1>
        <p>${escapeXml(description)}</p>
        <p class="muted" style="font-size: 13px; margin-top: 8px">${formatDateEn(date)}</p>
      </div>
    </section>
    <main class="wrap" style="max-width: 860px; padding-bottom: 72px">
      <article class="docs-prose">
${htmlBody}
        <div style="margin-top: 32px; padding: 16px 20px; background: var(--surface); border-radius: 8px; border: 1px solid var(--border);">
          <p style="margin: 0;">
            📖 <a href="${zhUrl}" style="color: var(--accent);">阅读中文版本</a>
          </p>
        </div>
      </article>
    </main>
    <footer id="site-footer"></footer>
    <script src="assets/js/site.js"></script>
  </body>
</html>
`;
}

// ── Blog list page (zh) ───────────────────────────────────────────
function buildZhBlogList() {
  const items = articles
    .sort((a, b) => (b.meta.date || '').localeCompare(a.meta.date || ''))
    .map(a => {
      const slug = a.meta.slug || basename(a.file, '.zh-CN.md');
      const title = a.meta.title || slug;
      const desc = a.meta.description || '';
      const date = a.meta.date || '';
      return `        <li style="padding: 20px 0; border-bottom: 1px solid var(--border);">
          <a href="blog-${slug}.html" style="color: var(--text); text-decoration: none; font-size: 20px; font-weight: 600; display: block; margin-bottom: 6px;">${escapeXml(title)}</a>
          <p style="color: var(--muted); font-size: 14px; margin: 0 0 4px;">${escapeXml(desc)}</p>
          <time style="color: var(--muted); font-size: 13px;">${formatDateZh(date)}</time>
        </li>`;
    })
    .join('\n');

  return `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>博客 — DataZen</title>
    <meta name="description" content="DataZen 官方博客：数据库管理工具的新功能、技术深度解析和使用指南。" />
    <link rel="canonical" href="${ORIGIN}/zh/blog.html" />
    <link rel="alternate" hreflang="en" href="${ORIGIN}/blog.html" />
    <link rel="alternate" hreflang="zh-CN" href="${ORIGIN}/zh/blog.html" />
    <link rel="alternate" hreflang="x-default" href="${ORIGIN}/blog.html" />
    <link rel="icon" href="../assets/logo.png" />
    <link rel="stylesheet" href="../assets/css/site.css" />
    <script type="application/ld+json">
      {
        "@context": "https://schema.org",
        "@type": "CollectionPage",
        "name": "博客",
        "description": "DataZen 官方博客",
        "url": "${ORIGIN}/zh/blog.html"
      }
    </script>
  </head>
  <body>
    <header id="site-header"></header>
    <section class="page-head">
      <div class="wrap">
        <p class="breadcrumb">
          <a href="apps/desktop/index.html">首页</a> / 博客
        </p>
        <h1>博客</h1>
        <p>DataZen 官方博客：新功能发布、技术深度解析和使用指南。</p>
      </div>
    </section>
    <main class="wrap" style="max-width: 860px; padding-bottom: 72px">
      <ul style="list-style: none; padding: 0; margin: 0;">
${items}
      </ul>
    </main>
    <footer id="site-footer"></footer>
    <script src="../assets/js/site.js"></script>
  </body>
</html>
`;
}

// ── Blog list page (en) ───────────────────────────────────────────
function buildEnBlogList() {
  const items = articles
    .sort((a, b) => (b.meta.date || '').localeCompare(a.meta.date || ''))
    .map(a => {
      const slug = a.slug;
      const title = a.enMeta.title || a.meta.title || slug;
      const desc = a.enMeta.description || a.meta.description || '';
      const date = a.meta.date || '';
      return `        <li style="padding: 20px 0; border-bottom: 1px solid var(--border);">
          <a href="blog-${slug}.html" style="color: var(--text); text-decoration: none; font-size: 20px; font-weight: 600; display: block; margin-bottom: 6px;">${escapeXml(title)}</a>
          <p style="color: var(--muted); font-size: 14px; margin: 0 0 4px;">${escapeXml(desc)}</p>
          <time style="color: var(--muted); font-size: 13px;">${formatDateEn(date)}</time>
        </li>`;
    })
    .join('\n');

  return `<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>Blog — DataZen</title>
    <meta name="description" content="DataZen official blog: new features, deep dives, and usage guides for the database management tool." />
    <link rel="canonical" href="${ORIGIN}/blog.html" />
    <link rel="alternate" hreflang="en" href="${ORIGIN}/blog.html" />
    <link rel="alternate" hreflang="zh-CN" href="${ORIGIN}/zh/blog.html" />
    <link rel="alternate" hreflang="x-default" href="${ORIGIN}/blog.html" />
    <link rel="icon" href="assets/logo.png" />
    <link rel="stylesheet" href="assets/css/site.css" />
    <script type="application/ld+json">
      {
        "@context": "https://schema.org",
        "@type": "CollectionPage",
        "name": "Blog",
        "description": "DataZen official blog",
        "url": "${ORIGIN}/blog.html"
      }
    </script>
  </head>
  <body>
    <header id="site-header"></header>
    <section class="page-head">
      <div class="wrap">
        <p class="breadcrumb">
          <a href="apps/desktop/index.html">Home</a> / Blog
        </p>
        <h1>Blog</h1>
        <p>DataZen official blog: new features, deep dives, and usage guides.</p>
      </div>
    </section>
    <main class="wrap" style="max-width: 860px; padding-bottom: 72px">
      <ul style="list-style: none; padding: 0; margin: 0;">
${items}
      </ul>
    </main>
    <footer id="site-footer"></footer>
    <script src="assets/js/site.js"></script>
  </body>
</html>
`;
}

// ── Helpers ────────────────────────────────────────────────────────
function escapeXml(s) {
  return String(s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function formatDateZh(d) {
  if (!d) return '';
  const [y, m, day] = d.split('-');
  return `${y} 年 ${parseInt(m)} 月 ${parseInt(day)} 日`;
}

function formatDateEn(d) {
  if (!d) return '';
  const months = ['January','February','March','April','May','June','July','August','September','October','November','December'];
  const [y, m, day] = d.split('-');
  return `${months[parseInt(m) - 1]} ${parseInt(day)}, ${y}`;
}

// ── Write output ───────────────────────────────────────────────────
mkdirSync(SITE_ZH_DIR, { recursive: true });

let written = 0;
for (const article of articles) {
  const slug = article.slug;

  // Chinese page
  const zhHtml = buildZhPage(article);
  const zhPath = join(SITE_ZH_DIR, `blog-${slug}.html`);
  writeFileSync(zhPath, zhHtml, 'utf-8');
  written++;

  // English page
  const enHtml = buildEnPage(article);
  const enPath = join(SITE_DIR, `blog-${slug}.html`);
  writeFileSync(enPath, enHtml, 'utf-8');
  written++;
}

// Blog list pages
writeFileSync(join(SITE_ZH_DIR, 'blog.html'), buildZhBlogList(), 'utf-8');
writeFileSync(join(SITE_DIR, 'blog.html'), buildEnBlogList(), 'utf-8');
written += 2;

console.log(`✅ Built ${articles.length} articles → ${written} HTML files`);
console.log(`   Chinese: site/zh/blog-*.html`);
console.log(`   English: site/blog-*.html`);
console.log(`   Lists:   site/zh/blog.html, site/blog.html`);
