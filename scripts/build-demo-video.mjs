#!/usr/bin/env node
/**
 * Build the site demo video from the 1920x1080 capture set.
 *
 * Sources live in `site/assets/screenshots-video/` (gitignored — regenerate
 * with `pnpm e2e:shots`). Every frame must already be exactly 1920x1080; a
 * mismatch here is what produced the old 2560x1648 cut, so it is a hard error
 * rather than a silent rescale that would soften the text.
 *
 * Renders in two stages. A 53-input `xfade` graph in one pass holds every
 * source decoded at once, which is a lot of 1080p surface for a slideshow to
 * need; per-slide intermediates keep peak memory to one frame.
 *
 * Usage: node scripts/build-demo-video.mjs
 */
import { execFileSync } from 'node:child_process';
import { mkdtempSync, rmSync, readdirSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const SRC_DIR = join(ROOT, 'site/assets/screenshots-video');
const OUT_DIR = join(ROOT, 'site/assets/video');

const WIDTH = 1920;
const HEIGHT = 1080;
const FPS = 30;
const SLIDE_SECONDS = 1.4; // per slide before transition overlap
const FADE_SECONDS = 0.45; // cross-dissolve between consecutive slides

const MP4 = join(OUT_DIR, 'demo-recording.mp4');
const WEBM = join(OUT_DIR, 'demo-recording.webm');
const POSTER = join(OUT_DIR, 'demo-poster.png');

const run = (args, label) => {
  process.stderr.write(`  ${label}\n`);
  execFileSync('ffmpeg', args, { stdio: ['ignore', 'ignore', 'inherit'] });
};

const probeSize = (file) => {
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
      'csv=s=x:p=0',
      file,
    ],
    { encoding: 'utf8' },
  ).trim();
  const [w, h] = out.split('x').map(Number);
  return { w, h };
};

if (!existsSync(SRC_DIR)) {
  console.error(`Missing ${SRC_DIR}. Regenerate the capture set with \`pnpm e2e:shots\`.`);
  process.exit(1);
}

// Lexicographic order is the intended tour: numeric captures, then Pro
// features, then the wizard. Digits sort before letters, which is what makes
// the `pro-*` tail land last without an explicit list to keep in sync.
const sources = readdirSync(SRC_DIR)
  .filter((f) => f.endsWith('.png'))
  .sort();

if (sources.length < 2) {
  console.error(`Need at least 2 frames, found ${sources.length}.`);
  process.exit(1);
}

console.log(`Frames: ${sources.length}`);

// Fail loudly on a mixed-size set — rescaling is what made the previous cut
// look soft, and a stray capture at the gallery's 2880x1648 would reintroduce
// exactly that.
const wrong = sources.filter((f) => {
  const { w, h } = probeSize(join(SRC_DIR, f));
  return w !== WIDTH || h !== HEIGHT;
});
if (wrong.length) {
  console.error(`Not all frames are ${WIDTH}x${HEIGHT}:`);
  wrong.forEach((f) =>
    console.error(`  ${f} (${probeSize(join(SRC_DIR, f)).w}x${probeSize(join(SRC_DIR, f)).h})`),
  );
  process.exit(1);
}

const work = mkdtempSync(join(tmpdir(), 'datazen-video-'));
const slideFrames = Math.round(SLIDE_SECONDS * FPS);

try {
  // ── Stage 1: one static segment per frame ───────────────────────────────
  // No Ken Burns on purpose: a slow push on a UI screenshot makes the text
  // swim and softens the very thing the shot is meant to show. Static frames,
  // cross-dissolve only.
  const segments = sources.map((name, i) => {
    const out = join(work, `seg-${String(i).padStart(3, '0')}.mp4`);
    // Sources are already exactly 1920x1080 (checked above), so this only has
    // to pin the pixel format and rate that xfade requires of every input.
    // `-loop 1 -framerate 30 -t` is what HOLDS the still for the slide: with
    // no zoompan to expand one input frame into `d` frames, a bare PNG input
    // yields a single frame and the whole chain collapses to 0.1s.
    run(
      [
        '-y',
        '-loop',
        '1',
        '-framerate',
        String(FPS),
        '-t',
        String(SLIDE_SECONDS),
        '-i',
        join(SRC_DIR, name),
        '-vf',
        'setsar=1,format=yuv420p',
        '-frames:v',
        String(slideFrames),
        '-c:v',
        'libx264',
        '-preset',
        'veryslow',
        '-crf',
        '18',
        '-r',
        String(FPS),
        out,
      ],
      `[${i + 1}/${sources.length}] ${name}`,
    );
    return out;
  });

  // ── Stage 2: chain the cross-dissolves ─────────────────────────────────
  // xfade emits `offset + slide` seconds, and it silently truncates to
  // whatever the *first* input can still feed once the transition starts. So
  // the step between offsets is capped at D - F: a larger one would start the
  // next dissolve after the running total has already ended, truncating the
  // video (measured: a D step yields 1.47s for a 3-slide chain).
  const inputs = [];
  for (const seg of segments) inputs.push('-i', seg);

  const step = SLIDE_SECONDS - FADE_SECONDS;
  let last = '[0:v]';
  const chain = [];
  for (let i = 1; i < segments.length; i++) {
    const label = `xf${i}`;
    const offset = (i * step).toFixed(3);
    chain.push(
      `${last}[${i}:v]xfade=transition=fade:duration=${FADE_SECONDS}:offset=${offset}[${label}]`,
    );
    last = `[${label}]`;
  }
  const total = segments.length * SLIDE_SECONDS - (segments.length - 1) * FADE_SECONDS;

  run(
    [
      '-y',
      ...inputs,
      '-filter_complex',
      chain.join(';') + `;[${last === '[0:v]' ? '0:v' : 'xf' + (segments.length - 1)}]null[out]`,
      '-map',
      '[out]',
      '-c:v',
      'libx264',
      '-preset',
      'slow',
      '-crf',
      '20',
      '-pix_fmt',
      'yuv420p',
      '-r',
      String(FPS),
      // `+faststart` moves the index to the front so the hero video starts
      // playing before the whole file lands.
      '-movflags',
      '+faststart',
      MP4,
    ],
    `Cross-dissolving ${segments.length} segments (${total.toFixed(1)}s)`,
  );

  // ── Stage 3: WebM twin ──────────────────────────────────────────────────
  run(
    [
      '-y',
      '-i',
      MP4,
      '-c:v',
      'libvpx-vp9',
      '-crf',
      '32',
      '-b:v',
      '0',
      '-row-mt',
      '1',
      '-pix_fmt',
      'yuv420p',
      WEBM,
    ],
    'Encoding WebM/VP9',
  );

  // ── Stage 4: poster from a clean frame, clear of any cross-dissolve ─────
  // 2.5s lands inside slide 1, not inside a transition.
  run(['-y', '-ss', '2.5', '-i', MP4, '-frames:v', '1', '-q:v', '2', POSTER], 'Writing poster');

  console.log(`\nWrote ${MP4}`);
  console.log(`Wrote ${WEBM}`);
  console.log(`Wrote ${POSTER}`);
} finally {
  rmSync(work, { recursive: true, force: true });
}
