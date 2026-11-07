import type { KeyTreeRow } from './keyTree';
import { isGlobalPattern } from './treeEmptyState';

/**
 * Client-side application of the R2 pattern to the *tree* rows
 * (redis-tree-ui-BUG-001).
 *
 * `list_children` takes no pattern (its options are `sep` / `noTtlOnly` /
 * `keyType`; a Rust-side contract change is outside this track), so the pattern
 * the flat `scan_keys` list already honours has to be applied to the tree here.
 * Everything in this module is a pure function over already-loaded rows, which
 * is what makes the glob semantics, the breadcrumb back-fill and the "nothing
 * matched" result testable without a DOM.
 *
 * The matcher is a **port of Redis' own `stringmatchlen_impl`** (`src/util.c`,
 * 7.2, `nocase = 0`) — byte-level alphabet, `[...]` classes with `^` negation,
 * `-` ranges, `\` escapes, the unterminated-class fallback, and a `?` that
 * consumes one *byte*. That fidelity is the whole point (redis-tree-ui-BUG-003):
 * a regex-shaped approximation is *almost* the same dialect, and "almost" is what
 * let the list view show three keys for `*[0-9]` while the tree asserted
 * `no-match` about the very same keys. `patternHasGlob` already treats `[` and
 * `\` as glob characters and sends them to the server verbatim, so the product
 * invites class expressions — both views must then mean the same thing by them.
 *
 * Redis matches on the **UTF-8 bytes** of a key, so names are encoded before
 * comparison and a `?` over a multi-byte character consumes one byte, exactly as
 * it does server-side.
 *
 * Visibility forks on `row.kind` — never on whether a `key` field happens to be
 * present. A child-level leaf row's `entry.key` is the *full* key the server
 * returned (Rust `split_children` keeps it absolute) and a folder row's `path`
 * is the full prefix including its trailing separator, so both are matched
 * against the same pattern string with the same meaning.
 *
 * Known limitation, by ruling: the pattern is applied to the *loaded* row set
 * only. A level whose scan has not finished holds keys the filter has never
 * seen, so `(n+)` counts stay server-side and `redis.tree.filterUnloaded` names
 * the gap. Full-keyset matching belongs to R / Wave 4.
 */

/* Redis compares raw bytes; these are the metacharacters of that grammar. */
const BYTE_ASTERISK = 0x2a; /* * */
const BYTE_QUESTION = 0x3f; /* ? */
const BYTE_OPEN_BRACKET = 0x5b; /* [ */
const BYTE_CLOSE_BRACKET = 0x5d; /* ] */
const BYTE_CARET = 0x5e; /* ^ */
const BYTE_HYPHEN = 0x2d; /* - */
const BYTE_BACKSLASH = 0x5c; /* \ */

/** Recursion ceiling copied from Redis ("protection against abusive patterns"). */
const MAX_NESTING = 1000;

const utf8Encoder = new TextEncoder();

/** A byte window — the `char *ptr, int len` pair Redis threads through. */
interface ByteSeq {
  readonly bytes: Uint8Array;
  readonly from: number;
  readonly len: number;
}

function encode(value: string): ByteSeq {
  const bytes = utf8Encoder.encode(value);
  return { bytes, from: 0, len: bytes.length };
}

/** Byte at `i`, or 0: what C reads once `len` is exhausted (the NUL). */
function byteAt(seq: ByteSeq, i: number): number {
  return i < seq.len ? seq.bytes[seq.from + i] : 0;
}

function sliceOf(seq: ByteSeq, from: number, len: number): ByteSeq {
  return { bytes: seq.bytes, from: seq.from + from, len };
}

/**
 * `stringmatchlen_impl(pattern, patternLen, string, stringLen, 0,
 * &skipLongerMatches, nesting)` — ported arm by arm from Redis 7.2 `src/util.c`,
 * quirks included: consecutive `*` collapse, a trailing `*` matching an empty
 * remainder, the `skipLongerMatches` early-out that keeps a pathological pattern
 * from backtracking exponentially, the unterminated-class rewind (so `[abc`
 * matches the literal), `[^...]` negation, `[a-b]` ranges with the operands
 * swapped when inverted, and `\` escaping the byte that follows it.
 *
 * `state.skip` is the by-reference `skipLongerMatches` flag Redis threads
 * through the recursion.
 */
function matchSeq(
  pattern: ByteSeq,
  str: ByteSeq,
  state: { skip: number },
  nesting: number,
): boolean {
  if (nesting > MAX_NESTING) return false;
  let p = 0;
  let pLen = pattern.len;
  let s = 0;
  let sLen = str.len;

  while (pLen > 0 && sLen > 0) {
    const c = byteAt(pattern, p);

    if (c === BYTE_ASTERISK) {
      while (pLen > 0 && byteAt(pattern, p + 1) === BYTE_ASTERISK) {
        p++;
        pLen--;
      }
      if (pLen === 1) return true; // a trailing `*` takes the rest, even nothing
      /*
       * Redis tries the remainder *before* consuming a byte (`while (stringLen)`
       * recurses with the current `string`), which is what lets `*` span nothing:
       * `a*c` matches `ac` and `*x` matches `x`. Starting one byte in was a real
       * port bug, caught by cross-checking against a literal transliteration.
       */
      while (sLen > 0) {
        if (
          matchSeq(sliceOf(pattern, p + 1, pLen - 1), sliceOf(str, s, sLen), state, nesting + 1)
        ) {
          return true;
        }
        if (state.skip !== 0) return false;
        s++;
        sLen--;
      }
      // Nothing matched the tail from any position: an earlier `*` need not try
      // a longer substring either (Redis' early-termination note).
      state.skip = 1;
      return false;
    }

    if (c === BYTE_QUESTION) {
      s++;
      sLen--;
    } else if (c === BYTE_OPEN_BRACKET) {
      p++;
      pLen--;
      const negated = byteAt(pattern, p) === BYTE_CARET;
      if (negated) {
        p++;
        pLen--;
      }
      let hit = false;
      for (;;) {
        const cur = byteAt(pattern, p);
        if (cur === BYTE_BACKSLASH && pLen >= 2) {
          p++;
          pLen--;
          if (byteAt(pattern, p) === byteAt(str, s)) hit = true;
        } else if (cur === BYTE_CLOSE_BRACKET) {
          break;
        } else if (pLen === 0) {
          p--;
          pLen++;
          break;
        } else if (pLen >= 3 && byteAt(pattern, p + 1) === BYTE_HYPHEN) {
          let start = cur;
          let end = byteAt(pattern, p + 2);
          if (start > end) {
            const swap = start;
            start = end;
            end = swap;
          }
          p += 2;
          pLen -= 2;
          const ch = byteAt(str, s);
          if (ch >= start && ch <= end) hit = true;
        } else if (cur === byteAt(str, s)) {
          hit = true;
        }
        p++;
        pLen--;
      }
      if (negated) hit = !hit;
      if (!hit) return false;
      s++;
      sLen--;
    } else {
      if (c === BYTE_BACKSLASH && pLen >= 2) {
        // `\` escapes the next byte and *falls through* to the literal compare,
        // exactly as the C `switch` has no `break` here.
        p++;
        pLen--;
      }
      if (byteAt(pattern, p) !== byteAt(str, s)) return false;
      s++;
      sLen--;
    }

    p++;
    pLen--;
    if (sLen === 0) {
      while (byteAt(pattern, p) === BYTE_ASTERISK) {
        p++;
        pLen--;
      }
      break;
    }
  }

  return pLen === 0 && sLen === 0;
}

/**
 * Compile an R2 pattern into the byte sequence the matcher walks. `null` means
 * "there is no pattern to match": a blank pattern is *not* Redis' "match the
 * empty key" — the R2 row already treats blank as "filter off" upstream, and
 * callers must keep reading `null` that way rather than as "match everything".
 */
export function compileGlob(pattern: string): ByteSeq | null {
  const trimmed = pattern.trim();
  return trimmed.length === 0 ? null : encode(trimmed);
}

/** A reusable predicate over key names for one compiled pattern. */
export type GlobMatcher = (name: string) => boolean;

/** Compile once, match many: the row and key filters take this path per render. */
export function globMatcher(compiled: ByteSeq | null): GlobMatcher {
  if (!compiled) return () => false;
  return (name: string) => matchSeq(compiled, encode(name), { skip: 0 }, 0);
}

/**
 * The identity a pattern is matched against, chosen by `row.kind` — the single
 * place allowed to reach for `path` vs `entry.key`. Forking on whether some field
 * is *present* instead would mis-handle a child-level leaf row, whose `entry.key`
 * is absolute while a folder's `path` carries its trailing separator.
 */
function rowMatchTarget(row: KeyTreeRow): string {
  return row.kind === 'folder' ? row.path : row.entry.key;
}

/**
 * The rows to paint under `pattern`, in pre-order.
 *
 *  - blank / `*` ⇒ the input unchanged (no filter is on);
 *  - a folder row survives when its full `path` matches; an **empty** folder
 *    (`count === 0`) never shows, matching or not;
 *  - a key row survives when its full key matches. A matching folder does *not*
 *    make its children visible as a group: expanded children are judged by their
 *    own match (a collapsed folder renders no children at all, so this is the
 *    same rule the tree already follows);
 *  - a non-matching folder that owns a surviving descendant is re-emitted as a
 *    **breadcrumb** (`breadcrumb: true`) so the row never floats without its
 *    path. It is decoration: not clickable, no checkbox, excluded from
 *    `data-row-count` and from every selection set;
 *  - a non-matching folder whose *subtree* holds a key that matches (per
 *    `hasVisibleDescendant`, i.e. the caller's filtered key set) survives as a
 *    normal clickable row — that is what keeps `*user*` from blanking the
 *    `app:` folder it obviously contains, while a pattern with nothing under it
 *    (`zzz`) does drop it;
 *  - nothing matched ⇒ `[]`, which is what makes I-11's `no-match` reachable in
 *    the default (tree) view.
 */
export function filterTreeRowsByPattern(
  rows: KeyTreeRow[],
  pattern: string,
  hasVisibleDescendant: (folderPath: string) => boolean = () => false,
): KeyTreeRow[] {
  if (isGlobalPattern(pattern)) return rows;

  const matches = globMatcher(compileGlob(pattern));

  /*
   * Two passes, because an ancestor can be needed by a descendant that appears
   * later in the list: pass one decides keeps and marks the ancestor chain of
   * every kept row, pass two emits in order. `stack[d]` is the index of the
   * folder row at depth `d` on the current path.
   */
  const matched = new Array<boolean>(rows.length);
  const candidate = new Array<boolean>(rows.length);
  const needed = new Array<boolean>(rows.length);
  const stack: number[] = [];

  for (let i = 0; i < rows.length; i++) {
    const row = rows[i]!;
    while (stack.length > row.depth) stack.pop();
    const isFolder = row.kind === 'folder';
    // The fork is on `row.kind`, never on a field being present (`rowMatchTarget`).
    const target = rowMatchTarget(row);
    const match = isFolder ? matches(target) && row.count > 0 : matches(target);
    // A folder whose *own* name fails the glob is not automatically irrelevant:
    // a collapsed subtree is judged by `hasVisibleDescendant` (the caller's
    // filtered key set), because that is the only honest source for keys the tree
    // never loaded. Such a folder stays an ordinary, expandable row.
    const probed = !match && isFolder && row.count > 0 && hasVisibleDescendant(target);
    if (probed) candidate[i] = true;
    matched[i] = match;
    if (match) for (const ancestor of stack) needed[ancestor] = true;
    if (isFolder) {
      // Any folder row is a possible ancestor route to deeper expanded rows,
      // matching or not — that is what turns it into a breadcrumb later.
      stack[row.depth] = i;
      stack.length = row.depth + 1;
    } else {
      stack.length = row.depth;
    }
  }

  /*
   * Emit precedence: a row that matches on its own wins; an ancestor of one does
   * so as a breadcrumb; and only a folder with *no* painted survivor underneath
   * may instead be kept by the probe (its whole subtree is unloaded, so
   * expanding it is the user's only way in — it stays a normal clickable row).
   */
  const out: KeyTreeRow[] = [];
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i]!;
    if (matched[i]) {
      out.push(row);
      continue;
    }
    // Precedence: a survivor's ancestor comes back as a *breadcrumb* (it is
    // already painted as a path, so making it inert is the honest reading), and
    // only a folder with nothing painted beneath it may be kept by the probe.
    if (needed[i] && row.kind === 'folder' && row.count > 0) {
      out.push({ ...row, breadcrumb: true });
      continue;
    }
    if (candidate[i]) out.push(row);
  }
  return out;
}

/** A breadcrumb row is path context only: never selectable, never counted. */
export function isBreadcrumbRow(row: KeyTreeRow): boolean {
  return row.kind === 'folder' && row.breadcrumb === true;
}

/** Rows the user can act on — `data-row-count`, selection and navigation. */
export function countSelectableRows(rows: KeyTreeRow[]): number {
  let n = 0;
  for (const row of rows) if (!isBreadcrumbRow(row)) n += 1;
  return n;
}

/**
 * The key names the visible row set stands for: the *single source* behind R1's
 * counter, 「全选已加载」, the folder-checkbox cascade and I-11's `{loaded}`
 * (BUG-001's contradiction pair dies here — they can no longer disagree about
 * which set is on screen). Blank / `*` ⇒ the input unchanged.
 */
export function filterKeysByPattern(keys: string[], pattern: string): string[] {
  if (isGlobalPattern(pattern)) return keys;
  return keys.filter(globMatcher(compileGlob(pattern)));
}
