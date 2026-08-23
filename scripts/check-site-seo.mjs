#!/usr/bin/env node
import fs from "node:fs";
import path from "node:path";

const ROOT = path.resolve("site");
const ORIGIN = "https://flyxl.github.io/datazen";
const PAGES = [
  "index.html",
  "features.html",
  "ai.html",
  "charts.html",
  "workflow.html",
  "databases.html",
  "download.html",
  "manual.html",
];
const errors = [];

function fail(msg) {
  errors.push(msg);
}

function read(rel) {
  const p = path.join(ROOT, rel);
  if (!fs.existsSync(p)) {
    fail(`missing file: ${rel}`);
    return null;
  }
  return fs.readFileSync(p, "utf8");
}

function mustInclude(html, rel, needle) {
  if (html && !html.includes(needle)) fail(`${rel}: missing ${needle}`);
}

function mustNotInclude(html, rel, needle) {
  if (html && html.includes(needle)) fail(`${rel}: must not contain ${needle}`);
}

if (fs.existsSync(path.join(ROOT, "assets/video"))) {
  fail("assets/video must not exist");
}

for (const file of PAGES) {
  const enRel = file;
  const zhRel = path.join("zh", file);
  const en = read(enRel);
  const zh = read(zhRel);

  const enCanon =
    file === "index.html" ? `${ORIGIN}/` : `${ORIGIN}/${file}`;
  const zhCanon =
    file === "index.html" ? `${ORIGIN}/zh/` : `${ORIGIN}/zh/${file}`;

  if (en) {
    mustInclude(en, enRel, 'lang="en"');
    mustInclude(en, enRel, `rel="canonical" href="${enCanon}"`);
    mustInclude(en, enRel, `hreflang="en" href="${enCanon}"`);
    mustInclude(en, enRel, `hreflang="zh-CN" href="${zhCanon}"`);
    mustInclude(en, enRel, `hreflang="x-default" href="${enCanon}"`);
    mustInclude(en, enRel, 'property="og:title"');
    mustInclude(en, enRel, 'property="og:description"');
    mustInclude(en, enRel, 'property="og:image"');
    mustInclude(en, enRel, 'property="og:url"');
    mustInclude(en, enRel, 'property="og:locale" content="en_US"');
    mustInclude(en, enRel, 'name="twitter:card" content="summary_large_image"');
    mustInclude(en, enRel, 'href="assets/css/site.css"');
    mustInclude(en, enRel, 'src="assets/js/site.js"');
    mustNotInclude(en, enRel, "60 秒");
    mustNotInclude(en, enRel, "demo.mp4");
    mustNotInclude(en, enRel, "video-box");
  }

  if (zh) {
    mustInclude(zh, zhRel, 'lang="zh-CN"');
    mustInclude(zh, zhRel, `rel="canonical" href="${zhCanon}"`);
    mustInclude(zh, zhRel, `hreflang="en" href="${enCanon}"`);
    mustInclude(zh, zhRel, `hreflang="zh-CN" href="${zhCanon}"`);
    mustInclude(zh, zhRel, `hreflang="x-default" href="${enCanon}"`);
    mustInclude(zh, zhRel, 'property="og:title"');
    mustInclude(zh, zhRel, 'property="og:description"');
    mustInclude(zh, zhRel, 'property="og:image"');
    mustInclude(zh, zhRel, 'property="og:url"');
    mustInclude(zh, zhRel, 'property="og:locale" content="zh_CN"');
    mustInclude(zh, zhRel, 'name="twitter:card" content="summary_large_image"');
    mustInclude(zh, zhRel, 'href="../assets/css/site.css"');
    mustInclude(zh, zhRel, 'src="../assets/js/site.js"');
    mustNotInclude(zh, zhRel, "60 秒");
    mustNotInclude(zh, zhRel, "demo.mp4");
    mustNotInclude(zh, zhRel, "video-box");
  }
}

const enIndex = read("index.html");
const zhIndex = read("zh/index.html");
mustInclude(enIndex, "index.html", "application/ld+json");
mustInclude(enIndex, "index.html", "SoftwareApplication");
mustInclude(zhIndex, "zh/index.html", "application/ld+json");
mustInclude(zhIndex, "zh/index.html", "SoftwareApplication");

const robots = read("robots.txt");
mustInclude(robots, "robots.txt", `Sitemap: ${ORIGIN}/sitemap.xml`);

const sitemap = read("sitemap.xml");
if (sitemap) {
  for (const file of PAGES) {
    const enUrl =
      file === "index.html" ? `${ORIGIN}/` : `${ORIGIN}/${file}`;
    const zhUrl =
      file === "index.html" ? `${ORIGIN}/zh/` : `${ORIGIN}/zh/${file}`;
    mustInclude(sitemap, "sitemap.xml", enUrl);
    mustInclude(sitemap, "sitemap.xml", zhUrl);
  }
}

if (errors.length) {
  for (const e of errors) console.error("FAIL:", e);
  console.error(`\n${errors.length} check(s) failed`);
  process.exit(1);
}
console.log("OK: site SEO/i18n structural checks passed");
