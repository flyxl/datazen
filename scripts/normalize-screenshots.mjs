#!/usr/bin/env node
/**
 * Normalise the release gallery to one pixel size.
 *
 * ## Why this exists
 *
 * Screenshots used to be captured at whatever size the host monitor happened
 * to report — `e2e/screenshots/site/` accumulated 12 different resolutions
 * (1280x820, 1920x1080, 1920x1440, 2560x1640, 2560x1648, 2880x1800, 3200x1648,
 * 2200x1440, 2000x1400, 2000x1440, 1500x1040, 1280x900). A still-image gallery
 * shrugs that off; a video does not, because every clip that does not match the
 * timeline gets letterboxed or stretched.
 *
 * ## Capture size vs. output size
 *
 * A maximized window on a Retina display is 1440x824 CSS points, and the
 * WebDriver screenshot comes out at the window's *backing* pixels — 2880x1648.
 * That is the native, unresampled size, so it is the default target here:
 * images already at 2880x1648 are left byte-identical rather than being
 * sharpened-then-blurred by a needless round trip.
 *
 * Resizing the window from the driver is a different lever entirely.
 * `browser.setWindowSize()` is honoured 1:1 in CSS pixels and ignores
 * devicePixelRatio, so pinning it to 1920x1080 yields a 1920x1080 image with
 * the UI laid out for 1920x1080 — the same UI, spread thinner, and on a
 * 1440-wide display a window that does not fit at all. Pin it to 2560x1648, as
 * several gallery specs used to, and the capture is 2560x1648 — half a frame
 * of dead desktop on the right. Hence: let a human maximize once, capture at
 * native, normalize afterwards.
 *
 * For the video, pass `--target 1920x1080`. That is a 2:3 downscale, which
 * stays sharp, whereas reaching 1080p by upscaling the old 1280x820 images
 * would not.
 *
 * ## Usage
 *
 *   node scripts/normalize-screenshots.mjs            # rewrite the off-size files
 *   node scripts/normalize-screenshots.mjs --check     # report only, change nothing
 *   node scripts/normalize-screenshots.mjs --target 2560x1440
 *
 *   # the 16:9 video set, written ALONGSIDE the stills
 *   node scripts/normalize-screenshots.mjs --target 1920x1080 --out e2e/screenshots/site-video
 *
 * ## `--out` matters more than it looks
 *
 * Without it this script overwrites the gallery in place. That is what you want
 * for tidying the stills up to one native size, and exactly what you do NOT want
 * when the target is the video size: the 2880x1648 stills are the site's
 * highest-resolution assets, and downscaling them to 1920x1080 in place would
 * throw those pixels away for good. `--out` writes a parallel set instead, so the
 * video compositor reads one directory that is uniform by construction while the
 * gallery keeps its native captures.
 *
 * Requires ffmpeg (sips is used on macOS to read dimensions, never to resample
 * — only ffmpeg rewrites pixels).
 */
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';

const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..');
const SITE_DIR = path.join(ROOT, 'e2e', 'screenshots', 'site');

const argv = process.argv.slice(2);
const checkOnly = argv.includes('--check');
const targetIdx = argv.indexOf('--target');
const targetArg = targetIdx === -1 ? '' : argv[targetIdx + 1];
const outIdx = argv.indexOf('--out');
const outArg = outIdx === -1 ? '' : argv[outIdx + 1];
const DEFAULT_TARGET = '2880x1648';

const target = /^(\d+)x(\d+)$/.exec(targetArg || DEFAULT_TARGET);
if (!target) {
  console.error(`--target must look like 2880x1648, got "${targetArg}"`);
  process.exit(1);
}
const [TARGET_W, TARGET_H] = [Number(target[1]), Number(target[2])];

function hasFfmpeg() {
  try {
    execFileSync('ffmpeg', ['-version'], { stdio: 'ignore' });
    return true;
  } catch {
    return false;
  }
}

/**
 * Pixel dimensions. sips answers instantly and is already on macOS; ffprobe
 * covers everything else. Returns null when neither tool can answer.
 */
function readSize(file) {
  try {
    const out = execFileSync('sips', ['-g', 'pixelWidth', '-g', 'pixelHeight', file], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const w = /pixelWidth:\s*(\d+)/.exec(out)?.[1];
    const h = /pixelHeight:\s*(\d+)/.exec(out)?.[1];
    if (w && h) return { w: Number(w), h: Number(h) };
  } catch {
    /* fall through to ffprobe */
  }
  try {
    const out = execFileSync(
      'ffprobe',
      [
        '-v',
        'error',
        '-select_streams',
        'v:0',
        '-show_entries',
        'stream=width,height',
        '-of',
        'csv=p=0',
        file,
      ],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] },
    );
    const [w, h] = out.trim().split(',').map(Number);
    if (w && h) return { w, h };
  } catch {
    /* unreadable */
  }
  return null;
}

/**
 * Scale to cover the target, then centre-crop. Preserves aspect ratio.
 *
 * lanczos rather than bilinear: a 1.5x upscale is exactly the case where the
 * cheaper filter shows up as mush on 12px UI text.
 *
 * `dest` is the file to write. It is the source itself for the in-place
 * default, and a fresh path under `--out` — the originals are never resampled
 * away, only copied forward.
 */
function rewrite(src, dest) {
  const tmp = `${dest}.norm.png`;
  execFileSync(
    'ffmpeg',
    [
      '-y',
      '-i',
      src,
      '-vf',
      `scale=${TARGET_W}:${TARGET_H}:flags=lanczos:force_original_aspect_ratio=increase,` +
        `crop=${TARGET_W}:${TARGET_H}`,
      '-frames:v',
      '1',
      '-pix_fmt',
      'rgb24',
      tmp,
    ],
    { stdio: ['ignore', 'ignore', 'pipe'] },
  );
  fs.renameSync(tmp, dest);
}

if (!fs.existsSync(SITE_DIR)) {
  console.error(`No such directory: ${SITE_DIR}`);
  process.exit(1);
}
const OUT_DIR = outArg ? path.resolve(ROOT, outArg) : null;
if (OUT_DIR === SITE_DIR) {
  console.error('--out would point back at the gallery itself; that is the in-place case.');
  process.exit(1);
}
const canRewrite = !checkOnly && hasFfmpeg();
if (!checkOnly && !canRewrite) {
  console.error('ffmpeg not found — install it, or re-run with --check.');
  process.exit(1);
}

const files = fs
  .readdirSync(SITE_DIR)
  .filter((f) => f.toLowerCase().endsWith('.png'))
  .sort();

const offSize = [];
const unreadable = [];
for (const f of files) {
  const size = readSize(path.join(SITE_DIR, f));
  if (!size) {
    unreadable.push(f);
    continue;
  }
  if (size.w !== TARGET_W || size.h !== TARGET_H) {
    offSize.push({ f, ...size });
  }
}

const target_ = `${TARGET_W}x${TARGET_H}`;
console.log(`${files.length} file(s) in e2e/screenshots/site`);
console.log(`${files.length - offSize.length - unreadable.length} already ${target_}`);
if (offSize.length) {
  console.log(`\n${offSize.length} need normalising:`);
  for (const s of offSize) {
    console.log(`  ${s.f}  ${s.w}x${s.h}  →  ${target_}`);
  }
}
if (unreadable.length) {
  console.log(`\n${unreadable.length} unreadable:`);
  unreadable.forEach((f) => console.log(`  ${f}`));
}

if (checkOnly) {
  process.exit(offSize.length ? 1 : 0);
}

for (const s of offSize) {
  const src = path.join(SITE_DIR, s.f);
  if (OUT_DIR) {
    fs.mkdirSync(OUT_DIR, { recursive: true });
    rewrite(src, path.join(OUT_DIR, s.f));
  } else {
    rewrite(src, src);
  }
}
console.log(
  OUT_DIR
    ? `\nNormalised ${offSize.length} file(s) to ${target_} into ${path.relative(ROOT, OUT_DIR)}/ (originals untouched).`
    : `\nNormalised ${offSize.length} file(s) to ${target_} in place.`,
);
