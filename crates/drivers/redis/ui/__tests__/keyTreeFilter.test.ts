/**
 * [redis-tree-ui-BUG-001] Table-driven battery for the pure pattern filter.
 *
 * Why a pure module: `list_children` has no pattern parameter (its options are
 * `sep` / `noTtlOnly` / `keyType` — see the Rust dispatch), so the R2 pattern
 * must be applied client-side to the loaded rows. The three things that are easy
 * to get wrong and impossible to see from a DOM test are exactly what is
 * table-driven here:
 *  1. glob semantics being Redis `MATCH` itself (byte-wise, classes, escapes) and
 *     not a regex approximation, so the tree and the flat list cannot disagree
 *     about one pattern (BUG-003 closed the 16 faces a regex-shaped port missed);
 *  2. forking visibility on `row.kind`, **not** on the presence of a `key`
 *     field — a child-level leaf row carries an absolute key, a folder row
 *     carries its full prefix, and both must meet the same pattern;
 *  3. the ancestor back-fill: a surviving deep row keeps a non-matching parent
 *     as a *breadcrumb*, and a breadcrumb is not a row the user can act on.
 */
import { describe, expect, it } from 'vitest';
import type { KeyEntry } from '@datazen/driver-sdk';
import type { KeyTreeRow } from '../key-browser/keyTree';
import {
  compileGlob,
  countSelectableRows,
  filterKeysByPattern,
  filterTreeRowsByPattern,
  globMatcher,
  isBreadcrumbRow,
} from '../key-browser/keyTreeFilter';

/**
 * Single-call convenience over the two exported primitives, rebuilt here: the
 * suite is about *Redis MATCH semantics*, so it names its own matcher instead
 * of leaning on a production shortcut that nothing on the UI path calls.
 */
function redisGlobMatch(name: string, pattern: string): boolean {
  return globMatcher(compileGlob(pattern))(name);
}

/* ── independent reference: literal transliteration of Redis 7.2 util.c ───── */

/**
 * `stringmatchlen(pattern, patternLen, string, stringLen, nocase = 0)` rendered
 * straight out of the C source: windowed byte cursors, the same loop shape, the
 * same fall-through from the `\` arm into the literal compare. Written from
 * `src/util.c`, not from `keyTreeFilter.ts`, so a shared mistake in the port's
 * *structure* is unlikely (a shared mistake in reading the C is still possible —
 * hence the hand table above, whose verdicts were each checked against the C
 * statements quoted in the port's comments).
 */
function refStringmatchlen(pattern: Uint8Array, str: Uint8Array): boolean {
  const impl = (
    pOff: number,
    pLen: number,
    sOff: number,
    sLen: number,
    skip: { v: number },
    nesting: number,
  ): boolean => {
    const at = (b: Uint8Array, o: number, i: number) => (o + i < b.length ? b[o + i] : 0);
    if (nesting > 1000) return false;
    while (pLen > 0 && sLen > 0) {
      const c = at(pattern, pOff, 0);
      if (c === 0x2a) {
        while (pLen > 0 && at(pattern, pOff, 1) === 0x2a) {
          pOff++;
          pLen--;
        }
        if (pLen === 1) return true;
        while (sLen > 0) {
          if (impl(pOff + 1, pLen - 1, sOff, sLen, skip, nesting + 1)) return true;
          if (skip.v) return false;
          sOff++;
          sLen--;
        }
        skip.v = 1;
        return false;
      } else if (c === 0x3f) {
        sOff++;
        sLen--;
      } else if (c === 0x5b) {
        pOff++;
        pLen--;
        let not = false;
        if (at(pattern, pOff, 0) === 0x5e) {
          not = true;
          pOff++;
          pLen--;
        }
        let match = false;
        for (;;) {
          if (at(pattern, pOff, 0) === 0x5c && pLen >= 2) {
            pOff++;
            pLen--;
            if (at(pattern, pOff, 0) === at(str, sOff, 0)) match = true;
          } else if (at(pattern, pOff, 0) === 0x5d) {
            break;
          } else if (pLen === 0) {
            pOff--;
            pLen++;
            break;
          } else if (pLen >= 3 && at(pattern, pOff, 1) === 0x2d) {
            let start = at(pattern, pOff, 0);
            let end = at(pattern, pOff, 2);
            if (start > end) {
              const t = start;
              start = end;
              end = t;
            }
            pOff += 2;
            pLen -= 2;
            const ch = at(str, sOff, 0);
            if (ch >= start && ch <= end) match = true;
          } else {
            if (at(pattern, pOff, 0) === at(str, sOff, 0)) match = true;
          }
          pOff++;
          pLen--;
        }
        if (not) match = !match;
        if (!match) return false;
        sOff++;
        sLen--;
      } else {
        if (c === 0x5c && pLen >= 2) {
          pOff++;
          pLen--;
        }
        if (at(pattern, pOff, 0) !== at(str, sOff, 0)) return false;
        sOff++;
        sLen--;
      }
      pOff++;
      pLen--;
      if (sLen === 0) {
        while (at(pattern, pOff, 0) === 0x2a) {
          pOff++;
          pLen--;
        }
        break;
      }
    }
    return pLen === 0 && sLen === 0;
  };
  return impl(0, pattern.length, 0, str.length, { v: 0 }, 0);
}

/* ── fixtures ─────────────────────────────────────────────────────────────── */

function entry(key: string): KeyEntry {
  return { key, keyType: 'string', ttl: -1, size: 0, preview: '' };
}

/**
 * Fold `levels` with `buildServerTreeRows`-equivalent shapes. Written by hand
 * rather than imported from the fold so a filter test cannot be rescued by a
 * fold change (and vice versa).
 */
function rowFolder(path: string, depth: number, count = 2): KeyTreeRow {
  return { kind: 'folder', path, label: path, depth, count };
}

function rowKey(key: string, depth: number): KeyTreeRow {
  return { kind: 'key', entry: entry(key), depth, label: key };
}

/** ids as `kind:path`, breadcrumbs suffixed `#bc` — compact, order-explicit. */
function ids(rows: KeyTreeRow[]): string[] {
  return rows.map((row) => {
    const id = row.kind === 'folder' ? `f:${row.path}` : `k:${row.entry.key}`;
    return isBreadcrumbRow(row) ? `${id}#bc` : id;
  });
}

/* ── the Redis MATCH dialect (BUG-003) ─────────────────────────────────────── */

/*
 * Every expectation below is read off Redis 7.2 `stringmatchlen_impl` by hand
 * (`src/util.c`, `nocase = 0`) — deliberately **not** computed by a second copy of
 * the matcher: an oracle that shares the implementation's assumptions cannot
 * catch a wrong port. The `BUG-003:<face>` tags name the divergence faces the
 * round-2 Tester measured against a faithful C port, so "the port is complete"
 * stays a checkable claim.
 */
const REDIS_MATCH_CASES: ReadonlyArray<readonly [string, string, boolean, string]> = [
  /* core dialect (aligned in round 1 — kept so a "fix" cannot regress it) */
  ['*', 'anything', true, 'core trailing star takes the rest'],
  // A lone `*` does NOT match the empty key: Redis' loop never runs on an empty
  // string and the final test demands both sides be consumed (`pLen === 0 &&
  // sLen === 0`) — caught by the differential below, against a common assumption.
  ['*', '', false, 'core star needs a byte to anchor on'],
  ['a*', 'abc', true, 'core'],
  ['*c', 'abc', true, 'core'],
  ['a*c', 'abbbc', true, 'core star spans bytes'],
  ['a*c', 'ac', true, 'core star may span nothing (caught a port bug)'],
  ['app:*', 'app:user:1', true, 'core namespace glob'],
  ['app:*', 'appuser:1', false, 'core `:` is a literal'],
  ['app:*', 'other:1', false, 'core'],
  ['a?c', 'abc', true, 'core one byte'],
  ['a?c', 'ac', false, 'core `?` is not optional'],
  ['a?c', 'aXXc', false, 'core `?` is not a run'],
  ['user', 'user', true, 'core exact'],
  ['user', 'username', false, 'core anchored'],
  ['user', 'User', false, 'core case sensitive'],
  ['*user*', 'app:user:1', true, 'core substring'],
  ['*user*', 'app:admin:1', false, 'core'],
  ['root-plain', 'root-plain', true, 'core'],
  ['root-plain', 'root-plainx', false, 'core'],
  ['app.user*', 'app.user:1', true, 'core `.` literal'],
  ['app.user:1', 'appXuser:1', false, 'core `.` is not any-char'],
  ['app:user:1', 'app:user:1', true, 'core `:` literal'],
  ['a+b', 'a+b', true, 'core `+` literal'],
  ['a(b)', 'a(b)', true, 'core parens literal'],
  ['x|y', 'x|y', true, 'core `|` literal'],
  ['a$b^', 'a$b^', true, 'core `$^` literal'],
  ['a{2}', 'a{2}', true, 'core braces literal'],

  /* face: class matching a set of bytes */
  ['h[ae]llo', 'hello', true, 'BUG-003:class-set'],
  ['h[ae]llo', 'hallo', true, 'BUG-003:class-set'],
  ['h[ae]llo', 'hillo', false, 'BUG-003:class-set'],
  /* face: class with a range */
  ['*[0-9]', 'user1', true, 'BUG-003:class-range (the reported cross-view case)'],
  ['*[0-9]', 'user2', true, 'BUG-003:class-range'],
  ['*[0-9]', 'cache:9', true, 'BUG-003:class-range'],
  ['*[0-9]', 'userx', false, 'BUG-003:class-range outside'],
  ['user[0-9]', 'user5', true, 'BUG-003:class-range'],
  ['h[a-b]llo', 'hbllo', true, 'BUG-003:class-range'],
  ['h[a-b]llo', 'hcllo', false, 'BUG-003:class-range outside'],
  ['h[b-a]llo', 'hallo', true, 'BUG-003:class-range operands swapped'],
  /* face: negated class (Redis DOES support `^`; `!` is NOT the negation char) */
  ['h[^e]llo', 'hallo', true, 'BUG-003:class-negate'],
  ['h[^e]llo', 'hello', false, 'BUG-003:class-negate'],
  ['user[^9]', 'user5', true, 'BUG-003:class-negate'],
  ['user[^9]', 'user9', false, 'BUG-003:class-negate'],
  ['h[!e]llo', 'hallo', false, 'BUG-003:class-negate-not-bang'],
  /* face: `\` escape inside a class */
  ['a[\\]]b', 'a]b', true, 'BUG-003:class-escape'],
  ['a[\\]]b', 'a[b', false, 'BUG-003:class-escape'],
  /* face: unterminated class falls back to a literal `[` */
  // An unterminated class is *not* demoted to the literal whole: Redis rewinds
  // onto `[` and then compares `[` literally, so `[abc` matches only a 4-char
  // `[abc`-less key prefix — `'[abc' vs '[abc'` is false (the `[` eats one byte,
  // `abc` must then line up with nothing). All three rows verified against the
  // transliteration below.
  /*
   * An unterminated class does **not** degrade to a literal `[` — Redis rewinds
   * onto the last class byte, `match` stays 0 and the arm returns false, so the
   * whole pattern fails. Two consequences worth pinning (both read off the C, and
   * both are things a "reasonable" implementation gets wrong in opposite
   * directions):
   *  - a normal unterminated class matches nothing at all;
   *  - an unterminated **negated** class matches *any* byte, because the empty
   *    match set is inverted.
   */
  ['[abc', '[abc', false, 'BUG-003:class-unterminated'],
  ['user[0-9', 'user[0-9', false, 'BUG-003:class-unterminated'],
  ['x[ab', 'x[b', false, 'BUG-003:class-unterminated'],
  ['x[ab', 'xab', false, 'BUG-003:class-unterminated'],
  // An unterminated *negated* class is the mirror image: its empty match set is
  // inverted, so it consumes one byte as "any" and `user[^9` matches `user5`.
  ['user[^9', 'user5', true, 'BUG-003:class-unterminated-negated'],
  ['user[^9', 'user55', false, 'BUG-003:class-unterminated-negated length'],
  ['[^abc', 'x', true, 'BUG-003:class-unterminated-negated'],
  /* face: `\` escapes the next byte outside a class */
  ['a\\b', 'ab', true, 'BUG-003:escape-literal'],
  ['a\\b', 'a\\b', false, 'BUG-003:escape-literal consumes the backslash'],
  ['\\*lit', '*lit', true, 'BUG-003:escape-star'],
  ['\\*lit', 'lit', false, 'BUG-003:escape-star consumes the star'],
  /* face: `\` at end of pattern keeps matching the backslash */
  ['ab\\', 'ab\\', true, 'BUG-003:trailing-backslash'],
  /* face: `*` spans newlines (byte-wise, unlike JS `.`) */
  ['a*b', 'a\nb', true, 'BUG-003:star-across-newline'],
  ['*x', 'a\nx', true, 'BUG-003:star-across-newline'],
  /* face: `?` consumes one byte of any value, newline included */
  ['a?c', 'a\nc', true, 'BUG-003:question-is-a-byte'],
  ['a?c', 'a\x00c', true, 'BUG-003:question-matches-NUL'],
  /* faces: `?` counts BYTES, so a multi-byte key matches per UTF-8 length */
  ['?', 'é', false, 'BUG-003:multibyte-? (é is 2 bytes)'],
  ['??', 'é', true, 'BUG-003:multibyte-??'],
  ['?', 'x', true, 'BUG-003:ascii-?'],
  ['???', '用', true, 'BUG-003:multibyte-??? (用 is 3 bytes)'],
  ['??????', '用', false, 'BUG-003:multibyte-six-? overshoots'],
  ['?', '用', false, 'BUG-003:multibyte-? one byte of three'],
  /* face: literal multibyte prefix compares byte-wise */
  ['é*', 'é:x', true, 'BUG-003:multibyte-literal-prefix'],
  ['é*', 'e:x', false, 'BUG-003:multibyte-literal-prefix'],
  /* face: an empty pattern is "no pattern", never "match the empty key" */
  ['', 'anything', false, 'BUG-003:blank-is-no-filter'],
  ['', '', false, 'BUG-003:blank-is-no-filter'],
];

describe('[redis-tree-ui-BUG-001/003] redisGlobMatch is Redis MATCH, not a regex approximation', () => {
  it.each(REDIS_MATCH_CASES)('%j vs %j ⇒ %j  (%s)', (pattern, key, expected) => {
    expect(redisGlobMatch(key, pattern)).toBe(expected);
  });

  it('covers all 16 reported divergence faces', () => {
    const faces = new Set<string>();
    for (const [, , , note] of REDIS_MATCH_CASES) {
      const tag = note.split(' ')[0];
      if (tag.startsWith('BUG-003:')) faces.add(tag);
    }
    /*
     * The 16 faces from the round-2 matrix, mapped onto the tags this table uses:
     * class ×5 (set / range / negate / escape / unterminated, plus the two
     * sub-faces the reference exposed: `!` is not negation and an unterminated
     * *negated* class matches anything), escape ×3, newline ×3, multibyte `?` ×5.
     */
    const reported = [
      'BUG-003:class-set',
      'BUG-003:class-range',
      'BUG-003:class-negate',
      'BUG-003:class-negate-not-bang',
      'BUG-003:class-escape',
      'BUG-003:class-unterminated',
      'BUG-003:class-unterminated-negated',
      'BUG-003:escape-literal',
      'BUG-003:escape-star',
      'BUG-003:trailing-backslash',
      'BUG-003:star-across-newline',
      'BUG-003:question-is-a-byte',
      'BUG-003:question-matches-NUL',
      'BUG-003:multibyte-?',
      'BUG-003:multibyte-??',
      'BUG-003:multibyte-???',
      'BUG-003:multibyte-six-?',
      'BUG-003:ascii-?',
      'BUG-003:multibyte-literal-prefix',
      'BUG-003:blank-is-no-filter',
    ];
    const missing = reported.filter((face) => !faces.has(face));
    expect(missing).toEqual([]);
    expect(faces.size).toBeGreaterThanOrEqual(16);
  });

  /*
   * Differential oracle. The matcher below is a *literal transliteration* of the
   * C source (kept in the test on purpose: it mirrors the upstream shape —
   * pointer+length windows, `while (patternLen && stringLen)`, fall-through — and
   * is written from `util.c`, not from `keyTreeFilter.ts`). If the production port
   * and an independent rendering of the same C ever disagree, this test names the
   * pair. It is what caught the `*`-cannot-span-nothing bug during this round:
   * the hand-written table below did not pin `a*c` vs `ac`, the differential did.
   *
   * The one excluded combination is the blank pattern: Redis' `stringmatchlen`
   * with an empty pattern matches only the empty string, while the product layer
   * defines blank as "no filter" (handled upstream by `isGlobalPattern`), so the
   * reference and the port deliberately differ there and the pair is skipped.
   */
  const REF_PATTERNS = [
    '*',
    'a*',
    '*c',
    'a*c',
    'app:*',
    'a?c',
    'user',
    '*user*',
    'root-plain',
    'app.user*',
    'h[ae]llo',
    '*[0-9]',
    'user[0-9]',
    'h[a-b]llo',
    'h[b-a]llo',
    'h[^e]llo',
    'a[\\]]b',
    '[abc',
    'user[0-9',
    'a\\b',
    '\\*lit',
    'ab\\',
    '*x',
    '?',
    '??',
    '???',
    '??????',
    '?????',
    'é*',
    'a??c',
    '**',
    '**a**',
    'a**b',
    '[]a]',
    '[^a]',
    '[!a]',
    'x\\',
    '\\\\',
    '*a*a*a*a*a*',
    '?[a-c]',
    '[a-c]?',
    '\\?',
  ] as const;
  const REF_KEYS = [
    '',
    'a',
    'ac',
    'abc',
    'a\nc',
    'a\x00c',
    'ab',
    'a\\b',
    '*lit',
    '[abc',
    'user5',
    'user[0-9',
    'hello',
    'hallo',
    'hillo',
    'hbllo',
    'hcllo',
    'a]b',
    'a[b',
    'x',
    'é',
    'é:x',
    'e:x',
    '用',
    '用ab',
    'aaaaab',
    'aaa',
    'cache:9',
    'user1',
    'app:user:1',
    'appXuser:1',
    'root-plain',
    'a{2}',
    'a(b)',
    'a$b^',
    'x|y',
    '[]a]',
    '[!a]',
    ']',
  ] as const;

  it('agrees with a literal C transliteration on every pattern/key pair', () => {
    const enc = new TextEncoder();
    const mismatches: string[] = [];
    for (const pattern of REF_PATTERNS) {
      if (pattern.length === 0) continue; // blank: product policy, see above
      for (const key of REF_KEYS) {
        const got = redisGlobMatch(key, pattern);
        const want = refStringmatchlen(enc.encode(pattern), enc.encode(key));
        if (got !== want) {
          mismatches.push(
            `${JSON.stringify(pattern)} vs ${JSON.stringify(key)}: port=${got} ref=${want}`,
          );
        }
      }
    }
    expect(mismatches).toEqual([]);
  });

  /*
   * Redis' `skipLongerMatches` early-out is what keeps a pattern of many `*`
   * groups off an exponential backtracking blowup. The claim worth pinning is
   * "terminates fast AND keeps the right verdict" — the verdict here is *true*
   * (a trailing `*` takes the remainder), which is itself a hand-table row I
   * initially guessed wrong, so it is recorded rather than smoothed over.
   */
  it('a many-star pattern is bounded by the early-out, not exponential', () => {
    const start = Date.now();
    expect(redisGlobMatch('a'.repeat(30) + 'b', '*a*a*a*a*a*a*a*a*a*a*')).toBe(true);
    expect(redisGlobMatch('a'.repeat(30) + 'b', '*a*a*a*a*a*a*a*a*a*ab')).toBe(true);
    expect(redisGlobMatch('a'.repeat(30), '*a*a*a*a*a*a*a*a*a*ab')).toBe(false);
    expect(Date.now() - start).toBeLessThan(500);
  });
});

/* ── pattern → prefix (the routing half of the fix) ────────────────────────── */

describe('[redis-tree-ui-BUG-001] filterTreeRowsByPattern forks on row.kind', () => {
  /** Root level: one folder + one loose key; `app:` expanded with two leaves. */
  function rows(): KeyTreeRow[] {
    return [
      rowFolder('app:', 0),
      rowKey('app:user:1', 1),
      rowKey('app:user:2', 1),
      rowKey('root-plain', 0),
    ];
  }

  it('a blank or `*` pattern is no filter at all (identity, same array)', () => {
    const input = rows();
    expect(filterTreeRowsByPattern(input, '')).toBe(input);
    expect(filterTreeRowsByPattern(input, '*')).toBe(input);
    expect(filterTreeRowsByPattern(input, '   ')).toBe(input);
  });

  it('nothing matched ⇒ empty list, which is what makes no-match reachable', () => {
    expect(filterTreeRowsByPattern(rows(), 'zzz')).toEqual([]);
    expect(countSelectableRows(filterTreeRowsByPattern(rows(), 'zzz'))).toBe(0);
  });

  it('an uncompilable pattern is "matches nothing", never "matches everything"', () => {
    expect(ids(filterTreeRowsByPattern(rows(), '['))).toEqual([]);
  });

  it('a matching folder keeps its own children only where they match too', () => {
    // `app:*` matches the folder `app:` *and* both leaves: no breadcrumb needed.
    expect(ids(filterTreeRowsByPattern(rows(), 'app:*'))).toEqual([
      'f:app:',
      'k:app:user:1',
      'k:app:user:2',
    ]);
    expect(filterTreeRowsByPattern(rows(), 'app:*').some((row) => isBreadcrumbRow(row))).toBe(
      false,
    );
  });

  it('a folder-only match does not smuggle non-matching children through', () => {
    // Pattern hits the prefix exactly; the leaves are longer, so only the
    // (collapsed-or-expanded) folder row itself survives.
    expect(ids(filterTreeRowsByPattern(rows(), 'app:'))).toEqual(['f:app:']);
  });

  it('a child-only match back-fills its parent as a breadcrumb', () => {
    const out = filterTreeRowsByPattern(rows(), 'app:user:1');
    expect(ids(out)).toEqual(['f:app:#bc', 'k:app:user:1']);
    const crumb = out[0]!;
    expect(crumb.kind).toBe('folder');
    if (crumb.kind === 'folder') {
      expect(crumb.path).toBe('app:');
      // The breadcrumb still reports the server count: it is the same folder.
      expect(crumb.count).toBe(2);
    }
    // Breadcrumbs are not rows the user acts on.
    expect(countSelectableRows(out)).toBe(1);
  });

  it('a deep match keeps the whole ancestor chain, each as a breadcrumb', () => {
    const deep: KeyTreeRow[] = [
      rowFolder('app:', 0, 5),
      rowFolder('app:user:', 1, 3),
      rowFolder('app:user:session:', 2, 1),
      rowKey('app:user:session:token-9', 3),
    ];
    expect(ids(filterTreeRowsByPattern(deep, '*token-9'))).toEqual([
      'f:app:#bc',
      'f:app:user:#bc',
      'f:app:user:session:#bc',
      'k:app:user:session:token-9',
    ]);
    expect(countSelectableRows(filterTreeRowsByPattern(deep, '*token-9'))).toBe(1);
  });

  it('an empty folder is never painted, matching or not', () => {
    const withEmpty: KeyTreeRow[] = [rowFolder('app:', 0, 0), rowKey('root-plain', 0)];
    expect(ids(filterTreeRowsByPattern(withEmpty, 'app:*'))).toEqual([]);
    expect(ids(filterTreeRowsByPattern(withEmpty, '*'))).toEqual(['f:app:', 'k:root-plain']);
    expect(ids(filterTreeRowsByPattern(withEmpty, 'root-plain'))).toEqual(['k:root-plain']);
  });

  it('a matching sibling does not drag its non-matching siblings in', () => {
    const out = filterTreeRowsByPattern(rows(), 'root-plain');
    expect(ids(out)).toEqual(['k:root-plain']);
  });

  it('preserves pre-order (parents before children, server order otherwise)', () => {
    const mixed: KeyTreeRow[] = [
      rowFolder('m:', 0),
      rowKey('m:one', 1),
      rowKey('m:two', 1),
      rowFolder('z:', 0),
      rowKey('z:three', 1),
    ];
    expect(ids(filterTreeRowsByPattern(mixed, 'm:*'))).toEqual(['f:m:', 'k:m:one', 'k:m:two']);
    // Every leaf matches ⇒ nothing is dropped, order is byte-for-byte the input.
    expect(filterTreeRowsByPattern(mixed, '*')).toBe(mixed);
    // `*:t*` hits `m:two` / `z:three` but not the folder paths themselves, so
    // both parents come back as breadcrumbs and pre-order is preserved.
    expect(ids(filterTreeRowsByPattern(mixed, '*:t*'))).toEqual([
      'f:m:#bc',
      'k:m:two',
      'f:z:#bc',
      'k:z:three',
    ]);
  });
});

/* ── the collapsed-subtree probe (why a folder survives without matching) ──── */

describe('[redis-tree-ui-BUG-001] filterTreeRowsByPattern probes collapsed subtrees', () => {
  /*
   * A *collapsed* folder's children were never folded into rows, so the row list
   * cannot answer "does anything under here match?" — the caller's filtered key
   * set can, via `hasVisibleDescendant`. Without the probe, `*user*` would blank
   * the `app:` folder that demonstrably contains `app:user:1`, and R1's counter
   * (reading that same key set) would disagree with the tree all over again.
   */
  const collapsed: KeyTreeRow[] = [rowFolder('app:', 0, 2), rowKey('root-plain', 0)];

  it('a collapsed folder whose subtree can match stays an ordinary clickable row', () => {
    const out = filterTreeRowsByPattern(collapsed, '*user*', (path) => path === 'app:');
    expect(ids(out)).toEqual(['f:app:']);
    expect(isBreadcrumbRow(out[0]!)).toBe(false);
    expect(countSelectableRows(out)).toBe(1);
  });

  it('a collapsed folder whose subtree cannot match is dropped', () => {
    // `root-*` hits the loose key; `app:` fails the glob and the probe says no
    // matching key lives under it ⇒ the folder goes, the leaf stays.
    expect(ids(filterTreeRowsByPattern(collapsed, 'root-*', () => false))).toEqual([
      'k:root-plain',
    ]);
    expect(ids(filterTreeRowsByPattern(collapsed, 'zzz*', () => false))).toEqual([]);
  });

  it('the probe defaults to false — no caller, no unwarranted survival', () => {
    expect(ids(filterTreeRowsByPattern(collapsed, '*user*'))).toEqual([]);
    expect(ids(filterTreeRowsByPattern(collapsed, '*user*', undefined))).toEqual([]);
  });

  it('a folder matching on its own needs no probe', () => {
    expect(ids(filterTreeRowsByPattern(collapsed, 'app:*', () => false))).toEqual(['f:app:']);
  });

  it('an empty folder is never revived by the probe', () => {
    const empty: KeyTreeRow[] = [rowFolder('app:', 0, 0)];
    expect(ids(filterTreeRowsByPattern(empty, '*user*', () => true))).toEqual([]);
  });
});

/* ── the selectable key set (single source of truth) ───────────────────────── */

describe('[redis-tree-ui-BUG-001] filterKeysByPattern is the one set everything reads', () => {
  const keys = ['app:user:1', 'app:user:2', 'root-plain'];

  it('blank / `*` / whitespace are no filter (same array back)', () => {
    expect(filterKeysByPattern(keys, '')).toBe(keys);
    expect(filterKeysByPattern(keys, '*')).toBe(keys);
    expect(filterKeysByPattern(keys, ' * ')).toBe(keys);
  });

  it('narrows to exactly the glob hits', () => {
    expect(filterKeysByPattern(keys, 'app:*')).toEqual(['app:user:1', 'app:user:2']);
    expect(filterKeysByPattern(keys, '*plain')).toEqual(['root-plain']);
    expect(filterKeysByPattern(keys, 'zzz')).toEqual([]);
    expect(filterKeysByPattern(keys, '[')).toEqual([]);
  });

  it('agrees with the row filter about which leaves are visible', () => {
    const rows: KeyTreeRow[] = [
      rowFolder('app:', 0),
      rowKey('app:user:1', 1),
      rowKey('app:user:2', 1),
      rowKey('root-plain', 0),
    ];
    for (const pattern of ['app:*', '*user:1', 'root-plain', 'zzz', '*']) {
      const fromRows = filterTreeRowsByPattern(rows, pattern)
        .filter((row) => row.kind === 'key' && !isBreadcrumbRow(row))
        .map((row) => (row.kind === 'key' ? row.entry.key : ''));
      expect(fromRows, pattern).toEqual(filterKeysByPattern(keys, pattern));
    }
  });
});
