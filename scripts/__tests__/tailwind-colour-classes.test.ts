/**
 * A Tailwind colour utility that names a colour nobody defined compiles to
 * nothing: no error, no warning, just a silently unstyled element. The repo
 * shipped four of them (`bg-destructive`, `text-destructive`, and friends)
 * because `destructive` was never a colour here — the token is `danger`.
 *
 * This walks the source and fails on any colour utility whose colour resolves
 * to neither Tailwind's own default palette, nor a `theme.extend.colors` entry
 * in `tailwind.config.ts`, nor a `--color-*` custom property in CSS.
 */
import { describe, expect, it } from 'vitest';
import type { Dirent } from 'node:fs';
import { readFileSync, readdirSync, statSync } from 'node:fs';
import { join, relative } from 'node:path';
import { SKIP_DIR_NAMES, isSkippedPath, readScannedIfPresent } from '../lib/scanTargets.mjs';
import { withProbeLock } from './boundaryMutation';

// The extension is deliberate. Extensionless, this resolves through Vite's
// `resolve.extensions` where `.js` precedes `.ts` — so it silently loaded
// `tailwind.config.js` whenever `tsc -b` had sprayed that compiled copy into the
// repo root (tsconfig.node.json is a composite project over these two files, and
// a composite project must emit). This guard is only as trustworthy as the file
// it reads, so it names the source file outright.
import tailwindConfig from '../../tailwind.config.ts';

/** Utilities that take a colour argument. */
const COLOUR_UTILITIES = [
  'bg',
  'text',
  'border',
  'ring',
  'fill',
  'stroke',
  'from',
  'via',
  'to',
  'outline',
  'decoration',
  'accent',
  'caret',
  'divide',
  'placeholder',
  'shadow',
];

/**
 * Arguments that share a spelling with colour utilities but are not colours —
 * the border directions, the type scale, the shadow scale, the alignment
 * keywords. Reviewed against the tree; anything not listed here and not a
 * known colour is treated as a bug.
 */
const NON_COLOUR_ARGUMENTS = new Set([
  // border direction / style
  'b',
  't',
  'l',
  'r',
  'x',
  'y',
  's',
  'e',
  'w',
  'solid',
  'dashed',
  'dotted',
  'double',
  'hidden',
  'collapse',
  'separate',
  'none',
  // type scale (text-*)
  'xs',
  'sm',
  'md',
  'base',
  'lg',
  'xl',
  '2xl',
  '3xl',
  '4xl',
  '5xl',
  '6xl',
  '7xl',
  '8xl',
  '9xl',
  'thin',
  'extralight',
  'light',
  'normal',
  'medium',
  'semibold',
  'bold',
  'extrabold',
  'black',
  // text alignment / decoration (text-*)
  'left',
  'center',
  'right',
  'justify',
  'start',
  'end',
  'wrap',
  'nowrap',
  'ellipsis',
  'clip',
  'truncate',
  'uppercase',
  'lowercase',
  'capitalize',
  'pretty',
  'balance',
  'ticker',
  'overline',
  'strikethrough',
  'underline',
  'overline',
  'indent',
  'vertical',
  'horizontal',
  'resizable',
  'scroll',
  'autofill',
  'placeholder',
  'file',
  'marker',
  'first',
  'first-letter',
  'first-line',
  'last',
  'even',
  'odd',
  'before',
  'after',
  'selection',
  'accent',
  'backdrop',
  // shadow / ring / outline / divide
  'inner',
  'none',
  'outer',
  'inset',
  '2xs',
  // spacing-ish tokens picked up from cn()-built class strings
  '0',
  '0.5',
  '1',
  '1.5',
  '2',
  '2.5',
  '3',
  '3.5',
  '4',
  '5',
  '6',
  '7',
  '8',
  '9',
  '10',
  '11',
  '12',
  '14',
  '16',
  '20',
  '24',
  '28',
  '32',
  '40',
  '48',
  '64',
]);

const SCAN_ROOTS = ['src', 'packages', 'e2e'];
// The shared set, not a local copy. The four guards that police this repository
// must agree on what "source" means: an independent list here is a place where
// a directory could be added to the shared set, picked up by two guards and
// silently missed by this one — which reads as a colour-class guard that
// stopped seeing the very tree it was pointed at. The extension set is
// deliberately narrower than the shared `SCAN_EXTENSIONS` because this guard
// only cares about files that can carry Tailwind class names.

/** A source file plus its contents, captured in one pass. */
type ScannedFile = { path: string; content: string };

/**
 * Enumerate source files **and read them in the same pass**.
 *
 * A previous revision returned paths and read them later, from the test body.
 * That left a window between enumeration and use in which another process
 * could create and delete a file, and the guard then died with ENOENT on
 * roughly 1 run in 3 — a flaky gate that proves nothing on any given run.
 * Reading here removes the window rather than hiding it: a file that was
 * already gone is simply never enumerated, and one that vanishes mid-read
 * fails loudly here, where the cause is obvious, instead of as a mystery
 * inside a test assertion.
 *
 * The read is deliberately NOT wrapped in try/catch. Swallowing it would let
 * a genuine I/O failure hide behind a guard that reports itself green.
 */
function walk(dir: string, out: ScannedFile[] = []): ScannedFile[] {
  let entries: Dirent[];
  try {
    // `withFileTypes` takes each entry's type from the same readdir syscall, so
    // the former second `statSync` — a second create/delete window — is gone.
    entries = readdirSync(dir, { withFileTypes: true });
  } catch (e) {
    // A missing scan root is legitimate; anything else (permissions, I/O) is a
    // real fault and must surface rather than quietly shrink the scan.
    if ((e as NodeJS.ErrnoException).code === 'ENOENT') return out;
    throw e;
  }
  for (const entry of entries) {
    if (SKIP_DIR_NAMES.has(entry.name)) continue;
    const full = join(dir, entry.name);
    // Some filesystems report DT_UNKNOWN, which leaves both predicates false;
    // fall back to a stat only in that case, so the common path stays windowless.
    const isDir = entry.isDirectory() || (!entry.isFile() && statSync(full).isDirectory());
    if (isDir) {
      // `packages/pro-extensions/**` is independently versioned code that no
      // Host build produces and no Host commit controls — see
      // `SKIP_PATH_PREFIXES` in scripts/lib/scanTargets.mjs for why this has to
      // be a path and not a bare directory name. The dead-colour responsibility
      // for that tree belongs to the extension's own repository, which always
      // has its own checkout.
      if (isSkippedPath(relative(process.cwd(), full).split(/[\\/]/).join('/'))) continue;
      walk(full, out);
    } else if (/\.(ts|tsx|js|jsx|css)$/.test(entry.name)) {
      const content = readScannedIfPresent(full);
      // Deleted between the directory read and this one: not in the tree, so it
      // has no class names for this guard to have an opinion about.
      if (content !== null) out.push({ path: full, content });
    }
  }
  return out;
}

/** Tailwind 4 ships its default palette as `--color-<family>-<shade>` vars. */
function defaultPaletteFamilies(): Set<string> {
  const css = readFileSync('node_modules/tailwindcss/theme.css', 'utf8');
  const families = new Set<string>();
  for (const m of css.matchAll(/--color-([a-z][a-z0-9-]*?)(?:-\d{2,3})?\s*:/g)) {
    families.add(m[1]);
  }
  return families;
}

/** The project's own semantic tokens, nested keys included (`dt.binary`, …). */
function projectColourKeys(): Set<string> {
  const keys = new Set<string>();
  const colors = tailwindConfig.theme?.extend?.colors as Record<string, unknown> | undefined;
  if (!colors) return keys;
  for (const [name, value] of Object.entries(colors)) {
    keys.add(name);
    if (value && typeof value === 'object') {
      for (const child of Object.keys(value as Record<string, unknown>)) {
        keys.add(`${name}-${child}`);
      }
    }
  }
  return keys;
}

/**
 * The real tree, read under the shared probe lock.
 *
 * `SCAN_ROOTS` includes `packages/`, which is where the boundary-guard suites
 * plant their mutation probes — a **real violating file** in the real working
 * tree, which lives there for as long as that suite's critical section lasts.
 * This guard's verdict is a pure function of what it reads, so a probe that
 * happened to name a colour utility would be reported here as a dead class in
 * this repository's own source. Today no probe body does; the lock is what
 * makes that true rather than lucky, and it is the same lock the probe writers
 * take (an on-disk file created with `O_EXCL`, atomic across the worker
 * processes Vitest runs
 * test files in).
 *
 * The enumeration and the read happen in one pass (see `walk`), so the snapshot
 * this file reasons over is the tree as it was under the lock.
 */
const files = withProbeLock(() => SCAN_ROOTS.flatMap((root) => walk(root)));

/** Tokens declared straight in CSS (`--color-foo`) — a valid colour source. */
function cssColourKeys(): Set<string> {
  const keys = new Set<string>();
  for (const { path, content } of files) {
    if (!path.endsWith('.css')) continue;
    for (const m of content.matchAll(/--color-([a-z][a-z0-9-]*)\s*:/g)) {
      keys.add(m[1]);
    }
  }
  return keys;
}

const known = new Set<string>([
  ...defaultPaletteFamilies(),
  ...projectColourKeys(),
  ...cssColourKeys(),
  // Valid everywhere, and not spelled as `--color-*` in the palette file.
  'transparent',
  'current',
  'inherit',
  'black',
  'white',
]);

/**
 * Resolve the colour half of a utility by longest known prefix, so
 * `titlebar-fg-muted` matches the declared `titlebar` tree and `red-400`
 * matches `red`.
 */
function resolve(utility: string, token: string): boolean {
  if (utility === 'stroke' && SVG_STROKE_GEOMETRY.has(token)) return true;
  if (NON_COLOUR_ARGUMENTS.has(token)) return true;
  if (NON_COLOUR_ARGUMENTS.has(token.split('-')[0])) return true;
  for (const candidate of known) {
    if (token === candidate || token.startsWith(`${candidate}-`)) return true;
  }
  return false;
}

const BACKTICK = String.fromCharCode(96);
/**
 * The leading boundary is line-start, whitespace, `"` or a backtick. Two
 * character classes are therefore **invisible to this guard**, and both are
 * deliberate — the costs of admitting them were measured, not guessed:
 *
 * 1. `.` — a utility used as a CSS selector, e.g.
 *    `querySelectorAll('.text-foo')`. Admitting `.` pulls in every non-colour
 *    typography utility (`text-center`, `text-wrap`, `text-ellipsis`, …) and
 *    swamps the signal. This is how the dead `.text-destructive` in
 *    `e2e/specs/zz-screenshots.ts` survived: it was found by reading, not here.
 * 2. `'` — a single-quoted string, e.g. `'bg-ink-900'`. Enabling it was
 *    measured by flipping this regex boundary to include `'` and running this
 *    file. It yields exactly **15 findings**:
 *
 *      `via-saved` ×2, `via-proxy` ×1, `via-ws` ×1  MySQL connection-string
 *                                                     keys (`?via=proxy`), not
 *                                                     gradient utilities.
 *      `from-a` ×5                                 the English preposition.
 *      `from-db0` ×2                               a redis console transcript row.
 *      `stroke-dasharray` ×2                       a real SVG/CSS attribute
 *                                                     (`line.getAttribute(...)`).
 *      `bg-ink-900` ×2                             **one genuinely dead class**
 *                                                     (see below), counted twice
 *                                                     because both occurrences sit
 *                                                     on `ConnectionWorkspaceHomeKvSlot.test.tsx:15`.
 *
 *    So 14 are false positives and **1 is a real dead colour class** under this
 *    guard's own definition: `bg-ink-900` reaches a real `cn()` through
 *    `DbTypeBadge.tsx`, and `ink` is not a declared colour. It is confined to a
 *    test fixture and never ships, so it is left in place — but "0 real bugs"
 *    would be false, and 14-of-15 noise every run is not worth buying.
 *
 *    Reproduce: change the boundary below to `[\\s"'\` + BACKTICK + `]` and run
 *    this file.
 *
 * So this guard covers class names in JSX/TSX `className` strings and double
 * quotes, **not** CSS selectors or single-quoted strings. Those two forms are
 * a review responsibility — do not read a green run here as proof that the
 * repo is free of dead colour classes.
 */
const COLOUR_PATTERN = new RegExp(
  `(?:^|[\\s"${BACKTICK}])((?:[a-z-]+:)?(${COLOUR_UTILITIES.join('|')})-([a-z][a-z0-9-]*))`,
  'g',
);

/**
 * Native SVG presentation attributes that begin with `stroke-`.
 *
 * `stroke` is a real Tailwind colour utility, and it is also the head of an SVG
 * attribute family that hand-built SVG strings write by hand. The guard read
 * `stroke-width="2"` as "the `stroke` utility, colour `width`" and reported it
 * as a dead colour class. The distinction is not guessable from the spelling —
 * `stroke-2` IS a Tailwind width utility while `stroke-width` is not a Tailwind
 * class at all — so the attribute list is spelled out rather than derived.
 *
 * Kept deliberately exhaustive-but-finite: a new `stroke-*` attribute that is
 * not a Tailwind class should be added here, and adding a *colour* one should
 * be a conscious act rather than something that silently passes.
 */
const SVG_STROKE_GEOMETRY = new Set([
  'width',
  'linecap',
  'linejoin',
  'miterlimit',
  'dasharray',
  'dashoffset',
  'opacity',
]);

/** Half-open `[start, end)` character ranges that hold comment text. */
type CommentRange = readonly [start: number, end: number];

/**
 * Find every comment span in a source file, without being fooled by comment
 * markers that live inside string literals.
 *
 * The guard scans lines with a regex that has no notion of syntax, so a JSDoc
 * sentence or a `//` note reads exactly like a class string. The previous
 * repair for that was an allowlist of English words (`ish`, `focused`,
 * `selectable`, `colour`) — a word list that silently swallows any *real* class
 * that happens to be spelled like a word, and that has to grow a new entry for
 * every new comment someone writes. Locating the comments instead fixes the
 * class of bug rather than the instances, so the allowlist entries are gone.
 *
 * Handles `//`, `/* *\/`, `'…'`, `"…"`, and template literals including
 * `${…}` interpolations (a class name inside an interpolation is real code and
 * must still be scanned). Regex literals are the one construct this cannot
 * disambiguate from division without a parser; a `/bg-dead/` regex literal is
 * therefore still reported, which is the safe direction to be wrong in.
 */
function commentRanges(content: string): CommentRange[] {
  const ranges: CommentRange[] = [];
  const stack: { kind: 'template' | 'brace'; depth: number }[] = [];
  let i = 0;
  while (i < content.length) {
    const ch = content[i];
    const next = content[i + 1];
    const top = stack[stack.length - 1];
    if (ch === '/' && next === '/') {
      const start = i;
      const newline = content.indexOf('\n', i);
      i = newline === -1 ? content.length : newline;
      ranges.push([start, i]);
      continue;
    }
    if (ch === '/' && next === '*') {
      const start = i;
      const close = content.indexOf('*/', i + 2);
      i = close === -1 ? content.length : close + 2;
      ranges.push([start, i]);
      continue;
    }
    if (ch === "'" || ch === '"') {
      i = skipQuoted(content, i, ch);
      continue;
    }
    if (ch === BACKTICK) {
      stack.push({ kind: 'template', depth: 0 });
      i += 1;
      continue;
    }
    if (top?.kind === 'template') {
      if (ch === '$' && next === '{') stack.push({ kind: 'brace', depth: 0 });
      i += ch === '$' ? 2 : 1;
      continue;
    }
    if (top?.kind === 'brace') {
      if (ch === '{') top.depth += 1;
      else if (ch === '}') {
        if (top.depth === 0) stack.pop();
        else top.depth -= 1;
      }
    }
    i += 1;
  }
  return ranges;
}

/** Index just past the closing quote of a single-line string literal. */
function skipQuoted(content: string, from: number, quote: string): number {
  let i = from + 1;
  while (i < content.length) {
    const ch = content[i];
    if (ch === '\\') {
      i += 2;
      continue;
    }
    if (ch === quote) return i + 1;
    // An unterminated quote (a stray apostrophe in prose, a template split
    // across lines) must not swallow the rest of the file.
    if (ch === '\n') return i;
    i += 1;
  }
  return i;
}

/** Whether an absolute offset falls inside any comment span. */
function inComment(ranges: CommentRange[], offset: number): boolean {
  for (const [start, end] of ranges) {
    if (offset >= start && offset < end) return true;
    if (start > offset) return false;
  }
  return false;
}

/**
 * The character offset of a match.
 *
 * `matchAll` always sets `index`, but the result type declares it optional, so it
 * has to be narrowed rather than asserted. Narrowing is also the safer reading:
 * a missing index would be fed to `inComment` as `NaN`, every comparison against
 * it is false, and the guard would answer "not in a comment" for a class that may
 * well be in one. Throwing is deliberate — an offset we cannot compute is not
 * something to skip over, because skipping is exactly how a class stops being
 * checked.
 */
function offsetOf(match: RegExpMatchArray): number {
  if (match.index === undefined) {
    throw new Error(
      'regex match has no index, so the comment lookup would be reading the wrong offset',
    );
  }
  return match.index;
}

/**
 * The guard's whole decision, over an arbitrary file list.
 *
 * Split out from {@link deadColourClasses} so the false-positive fixes can be
 * pinned with synthetic fixtures. Asserting "the tree is green today" is not
 * evidence that a scanner bug is fixed: it is equally consistent with the bug
 * being real and the tree happening not to contain an instance. These
 * fixtures contain, on purpose, both the shapes that used to be reported
 * wrongly and the shapes that must still be reported.
 */
function findDeadColourClasses(scan: ScannedFile[]): string[] {
  const dead: string[] = [];
  for (const { path, content } of scan) {
    if (path.endsWith('.css')) continue;
    const lines = content.split('\n');
    // Cheap pre-pass: only pay for the comment scan on files that have at
    // least one candidate at all.
    const candidates = new Map<number, RegExpMatchArray[]>();
    lines.forEach((line, index) => {
      const matches = [...line.matchAll(COLOUR_PATTERN)];
      if (matches.length > 0) candidates.set(index, matches);
    });
    if (candidates.size === 0) continue;
    const comments = commentRanges(content);
    let lineStart = 0;
    lines.forEach((line, index) => {
      const matches = candidates.get(index);
      if (matches) {
        for (const m of matches) {
          if (inComment(comments, lineStart + offsetOf(m))) continue;
          if (!resolve(m[2], m[3])) {
            dead.push(`${relative(process.cwd(), path)}:${index + 1} \`${m[1]}\``);
          }
        }
      }
      lineStart += line.length + 1;
    });
  }
  return dead;
}

function deadColourClasses(): string[] {
  return findDeadColourClasses(files);
}

describe('Tailwind colour utilities reference a colour that exists', () => {
  it('has a default palette and the project tokens to compare against', () => {
    // If Tailwind ever stops shipping theme.css this guard would silently pass
    // everything, so assert the inputs are non-trivial.
    expect(defaultPaletteFamilies().size).toBeGreaterThan(10);
    expect(projectColourKeys()).toContain('danger');
    expect(projectColourKeys()).toContain('titlebar-fg-muted');
  });

  it('finds no dead colour class in the source', () => {
    expect(deadColourClasses()).toEqual([]);
  });

  // ── The two regressions below, pinned on synthetic input ───────────────────
  //
  // Both were found in `packages/pro-extensions/sql-editor-pro`, a separate git
  // repository that this guard scans because it sits under `packages/`. A fix
  // that only made the tree green without changing the scanner would have left
  // both defects live for the next file written into that repo.

  const scanOne = (content: string): string[] =>
    findDeadColourClasses([{ path: 'synthetic/fixture.tsx', content }]);

  it('still reports a dead colour class in a real class string', () => {
    // Negative control. If comment-stripping were implemented by deleting
    // anything that looked prose-ish, this is the assertion that would catch
    // it having deleted the class strings too.
    const found = scanOne(
      [
        'const a = <div className="border-border p-2" />;',
        'const b = <div className="text-muted" />;',
        'const c = <div className="hover:bg-elevated" />;',
        'const d = <div className="bg-destructive" />;',
        'const e = <div className="border-edge bg-surface-raised text-fg-muted" />;',
      ].join('\n'),
    );
    expect(found).toEqual([
      'synthetic/fixture.tsx:1 `border-border`',
      'synthetic/fixture.tsx:2 `text-muted`',
      'synthetic/fixture.tsx:3 `hover:bg-elevated`',
      'synthetic/fixture.tsx:4 `bg-destructive`',
    ]);
  });

  it('reports nothing for comment prose, JSX comments, or SVG attributes', () => {
    // Every line here is one of the 9 false positives this guard used to
    // produce, in the exact form the Pro sources use them.
    expect(
      scanOne(
        [
          '/** Polyline path; `stroke-linejoin: round` rounds the corners. */',
          '/**',
          ' * Operators that only make sense for text-like columns, following the',
          " * host's caret-scoped model.",
          ' */',
          '{/* Header — accent-tinted (same `bg-accent/15` recipe) */}',
          '  // TODO: border-border is a typo, should be border-edge',
          '  /* text-muted in prose, not a class */',
          'const svg =',
          '  \'<circle cx="8" stroke="currentColor" stroke-width="2" fill="none" \' +',
          '  \'stroke-dasharray="28" stroke-dashoffset="8" stroke-linecap="round"/>\' +',
          '  \'<path stroke-width="3" stroke-linejoin="round"/>\';',
        ].join('\n'),
      ),
    ).toEqual([]);
  });

  it('is not fooled by comment markers inside string literals', () => {
    // A `//` inside a URL must not open a comment that eats the rest of the
    // line — including a real dead class further along it.
    expect(
      scanOne(
        [
          "const url = 'https://example.test/a/b';",
          'const link = <a className="bg-destructive">x</a>;',
        ].join('\n'),
      ),
    ).toEqual(['synthetic/fixture.tsx:2 `bg-destructive`']);
  });

  it('still scans class names inside template-literal interpolations', () => {
    // `${…}` is code, not prose. A dead class built at runtime is the hardest
    // kind to notice and must not be skipped as if it were a comment.
    expect(
      scanOne(
        [
          'const cls = `rounded bg-surface p-2 ${active ? "bg-destructive" : "bg-surface-alt"}`;',
        ].join('\n'),
      ),
    ).toEqual(['synthetic/fixture.tsx:1 `bg-destructive`']);
  });

  it('does not exempt a non-stroke utility that merely spells an SVG attribute', () => {
    // The carve-out is scoped to `stroke`. `text-linecap` is still a dead
    // colour, and a blanket "stroke-ish words are fine" rule would hide it.
    expect(scanOne('<div className="text-linecap" />;')).toEqual([
      'synthetic/fixture.tsx:1 `text-linecap`',
    ]);
  });
});
