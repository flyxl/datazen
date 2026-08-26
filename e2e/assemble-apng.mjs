#!/usr/bin/env node
/**
 * Assemble WebDriver screenshot frames into an animated PNG (APNG).
 *
 * Pure Node implementation — no ffmpeg, no native deps, no system permissions.
 * Frame PNGs are parsed at the chunk level and their IDAT payloads are reused
 * verbatim inside fdAT chunks, so there is no pixel re-encoding at all.
 *
 * Frame pacing derives from file mtime deltas (clamped), so playback speed
 * matches how the demo actually ran.
 *
 * Usage:
 *   node e2e/assemble-apng.mjs [--dir e2e/.demo-recording] [--out e2e/demo-recording.png] [--fps 6]
 *   node e2e/assemble-apng.mjs --dir e2e/.demo-recording --concat e2e/.demo-recording/ffconcat.txt
 *     (emits an ffconcat file with per-frame durations for ffmpeg mp4 conversion;
 *      convert with: ffmpeg -f concat -safe 0 -i <concat.txt> -c:v libx264 -pix_fmt yuv420p out.mp4)
 */
import fs from 'node:fs';
import path from 'node:path';

const args = process.argv.slice(2);
function argOf(flag, dflt) {
  const i = args.indexOf(flag);
  return i >= 0 && args[i + 1] ? args[i + 1] : dflt;
}
const hasFlag = (flag) => args.includes(flag);

const DIR = path.resolve(argOf('--dir', 'e2e/.demo-recording'));
const OUT = path.resolve(argOf('--out', 'e2e/demo-recording.png'));
const CONCAT_OUT = argOf('--concat', null);
const FALLBACK_FPS = Number(argOf('--fps', '6'));
const MP4_ONLY = hasFlag('--concat-only');

// ── CRC32 (PNG) ──
const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = -1;
  for (let i = 0; i < buf.length; i++) c = CRC_TABLE[(c ^ buf[i]) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const out = Buffer.alloc(8 + data.length + 4);
  out.writeUInt32BE(data.length, 0);
  out.write(type, 4, 'ascii');
  data.copy(out, 8);
  const body = out.subarray(4, 8 + data.length);
  out.writeUInt32BE(crc32(body), 8 + data.length);
  return out;
}

const PNG_SIG = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);

/** Parse a PNG into { ihdr, idat } — idat is the concatenated zlib stream. */
function parsePng(file) {
  const buf = fs.readFileSync(file);
  if (!buf.subarray(0, 8).equals(PNG_SIG)) throw new Error(`${file}: not a PNG`);
  let off = 8;
  let ihdr = null;
  const idats = [];
  while (off < buf.length) {
    const len = buf.readUInt32BE(off);
    const type = buf.toString('ascii', off + 4, off + 8);
    const data = buf.subarray(off + 8, off + 8 + len);
    if (type === 'IHDR') ihdr = data;
    else if (type === 'IDAT') idats.push(data);
    else if (type === 'IEND') break;
    // all other chunks dropped intentionally (color profiles etc.)
    off += 12 + len;
  }
  if (!ihdr || idats.length === 0) throw new Error(`${file}: missing IHDR/IDAT`);
  return { ihdr: Buffer.from(ihdr), idat: Buffer.concat(idats) };
}

function u32b(n) {
  const b = Buffer.alloc(4);
  b.writeUInt32BE(n >>> 0, 0);
  return b;
}
function u16b(n) {
  const b = Buffer.alloc(2);
  b.writeUInt16BE(n & 0xffff, 0);
  return b;
}

// ── Main ──
const frames = fs
  .readdirSync(DIR)
  .filter((f) => /^frame_\d+\.png$/.test(f))
  .sort();
if (frames.length < 2) {
  console.error(`ERROR: need >= 2 frames in ${DIR}, found ${frames.length}`);
  process.exit(1);
}

const parsed = frames.map((f) => parsePng(path.join(DIR, f)));
const ref = parsed[0].ihdr;
parsed.forEach((p, i) => {
  if (!p.ihdr.equals(ref)) {
    console.error(
      `ERROR: frame ${frames[i]} has a different IHDR than frame ${frames[0]}` +
        ` (window resized mid-run?)`,
    );
    process.exit(1);
  }
});
const W = ref.readUInt32BE(0);
const H = ref.readUInt32BE(4);

// Per-frame delays from mtime deltas, clamped to [1/FALLBACK_FPS…1s]
const mtimes = frames.map((f) => fs.statSync(path.join(DIR, f)).mtimeMs);
const minDelay = Math.round(1000 / FALLBACK_FPS);
const delays = mtimes.map((t, i) => {
  if (i === 0) return minDelay;
  const d = Math.round(t - mtimes[i - 1]);
  return Math.min(1000, Math.max(minDelay, d));
});

// ── Optional: emit ffconcat file (for ffmpeg mp4 conversion with true pacing) ──
if (CONCAT_OUT) {
  const lines = ['ffconcat version 1.0'];
  frames.forEach((f, i) => {
    lines.push(`file '${f}'`);
    lines.push(`duration ${(delays[i] / 1000).toFixed(3)}`);
  });
  // concat demuxer ignores the final duration unless the last file repeats
  lines.push(`file '${frames[frames.length - 1]}'`);
  fs.writeFileSync(CONCAT_OUT, `${lines.join('\n')}\n`);
  const total = delays.reduce((a, b) => a + b, 0);
  console.log(
    `ffconcat written: ${CONCAT_OUT} (${frames.length} frames, ${(total / 1000).toFixed(1)}s)`,
  );
  if (MP4_ONLY) process.exit(0);
}

// ── Mux APNG ──
const parts = [PNG_SIG];
parts.push(chunk('IHDR', ref));

let seqNum = 0;
function fctl(delayMs, isFirst) {
  // dispose_op=0 (none), blend_op=0 (source); full-frame opaque replacements
  return chunk(
    'fcTL',
    Buffer.concat([
      u32b(seqNum++),
      u32b(W),
      u32b(H),
      u32b(0),
      u32b(0),
      u16b(Math.max(1, delayMs)),
      u16b(1000), // delay denominator → millisecond precision
      Buffer.from([isFirst ? 0 : 0, 0]),
    ]),
  );
}

parts.push(chunk('acTL', Buffer.concat([u32b(parsed.length), u32b(0)]))); // loop forever

// Frame 1: fcTL + regular IDAT
parts.push(fctl(delays[0], true));
parts.push(chunk('IDAT', parsed[0].idat));
// Frames 2..n: fcTL + fdAT(seq prefix + same stream bytes)
for (let i = 1; i < parsed.length; i++) {
  parts.push(fctl(delays[i], false));
  parts.push(chunk('fdAT', Buffer.concat([u32b(seqNum++), parsed[i].idat])));
}
parts.push(chunk('IEND', Buffer.alloc(0)));

fs.writeFileSync(OUT, Buffer.concat(parts));

const total = delays.reduce((a, b) => a + b, 0);
console.log('=== APNG assembled ===');
console.log(`  File:     ${OUT} (${(fs.statSync(OUT).size / 1024 / 1024).toFixed(1)} MB)`);
console.log(`  Frames:   ${parsed.length} @ ${W}x${H}`);
console.log(`  Duration: ${(total / 1000).toFixed(1)}s (avg ${(total / parsed.length).toFixed(0)}ms/frame)`);
