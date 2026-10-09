/**
 * Shared JS/TS source scanner for the repo's boundary guards.
 *
 * Both `scripts/check-module-layers.mjs` and
 * `scripts/check-driver-import-boundaries.mjs` need the *complete* module
 * specifier surface of a file, not just the `from '…'` shape: plain
 * `import` / `export … from`, dynamic `import()`, `require()`, and
 * `vi.mock()` / `vi.doMock()` all end up as string literals, and matching a
 * single keyword is exactly how violations stayed invisible before the guards
 * existed. Keeping one tokenizer in one place means a rule added to either
 * script can never be weaker than the other by accident.
 */

const REGEX_ALLOWED_AFTER = new Set([
  '',
  '(',
  ',',
  '=',
  ':',
  ';',
  '!',
  '&',
  '|',
  '?',
  '{',
  '[',
  '+',
  '-',
  '*',
  '%',
  '~',
  '^',
  '<',
  '>',
  '}',
]);

/**
 * Single-pass tokenizer used by every rule:
 *  - `code`: the source with comments **and string-literal bodies** blanked to
 *    spaces (line breaks preserved) → safe to lint call syntax line by line
 *    without tripping on prose or on a path inside a string;
 *  - `literals`: every static string/template literal with its start line →
 *    the complete import-specifier surface.
 *
 * Escapes, regex literals (`/["']/` must not open a string) and nested template
 * expressions are handled well enough for linting purposes: an ambiguous case
 * degrades to "treat as code", never to a swallowed region.
 *
 * @param {string} source
 * @returns {{ code: string, literals: Array<{ value: string, line: number }> }}
 */
export function scanCode(source) {
  const out = source.split('');
  const literals = [];
  const n = source.length;
  let line = 1;
  let i = 0;
  let prev = ''; // last significant char outside strings/comments

  const blank = (from, to) => {
    for (let k = from; k < to && k < n; k += 1) if (out[k] !== '\n') out[k] = ' ';
  };

  while (i < n) {
    const ch = source[i];
    if (ch === '\n') {
      line += 1;
      i += 1;
      continue;
    }
    if (ch === '/' && source[i + 1] === '/') {
      const start = i;
      while (i < n && source[i] !== '\n') i += 1;
      blank(start, i);
      continue;
    }
    if (ch === '/' && source[i + 1] === '*') {
      const start = i;
      i += 2;
      while (i < n && !(source[i] === '*' && source[i + 1] === '/')) {
        if (source[i] === '\n') line += 1;
        i += 1;
      }
      i = Math.min(i + 2, n);
      blank(start, i);
      continue;
    }
    if (ch === '/' && REGEX_ALLOWED_AFTER.has(prev)) {
      // Regex literal: consume it so quotes inside it are not read as strings.
      i += 1;
      let inClass = false;
      while (i < n) {
        const c = source[i];
        if (c === '\n') break; // unterminated → bail, keep scanning as code
        if (c === '\\') {
          i += 2;
          continue;
        }
        if (c === '[') inClass = true;
        else if (c === ']') inClass = false;
        else if (c === '/' && !inClass) {
          i += 1;
          break;
        }
        i += 1;
      }
      while (i < n && /[dgimsuvy]/.test(source[i])) i += 1;
      prev = '/';
      continue;
    }
    if (ch === '"' || ch === "'") {
      const start = i;
      const startLine = line;
      const quote = ch;
      i += 1;
      let value = '';
      let terminated = false;
      while (i < n) {
        const c = source[i];
        if (c === '\n') break; // JS strings do not span lines: treat as unterminated
        if (c === '\\') {
          value += c + (source[i + 1] ?? '');
          i += 2;
          continue;
        }
        if (c === quote) {
          terminated = true;
          i += 1;
          break;
        }
        value += c;
        i += 1;
      }
      if (terminated) {
        literals.push({ value, line: startLine });
        blank(start, i);
      }
      prev = quote;
      continue;
    }
    if (ch === '`') {
      const start = i;
      const startLine = line;
      i += 1;
      let value = '';
      let dynamic = false;
      let terminated = false;
      let depth = 0;
      while (i < n) {
        const c = source[i];
        if (c === '\\') {
          value += source[i + 1] ?? '';
          i += 2;
          continue;
        }
        if (depth === 0 && c === '`') {
          terminated = true;
          i += 1;
          break;
        }
        if (c === '$' && source[i + 1] === '{') {
          dynamic = true;
          depth += 1;
          i += 2;
          continue;
        }
        if (c === '{' && depth > 0) depth += 1;
        if (c === '}' && depth > 0) depth -= 1;
        if (c === '\n') line += 1;
        value += c;
        i += 1;
      }
      // `${…}` templates carry computed specifiers: not statically checkable.
      if (terminated && !dynamic) literals.push({ value, line: startLine });
      blank(start, i);
      prev = '`';
      continue;
    }
    if (!/\s/.test(ch)) prev = ch;
    i += 1;
  }

  return { code: out.join(''), literals };
}

// ---------------------------------------------------------------------------
// Finding a forbidden needle in blanked source.
// ---------------------------------------------------------------------------

/**
 * Offset of the first `needle` in `code` that begins a whole code token, or -1.
 *
 * Only the **left** edge is checked, and that is enough for the needles the
 * guards use (`fetch(`, `XMLHttpRequest`): the trailing `(` or the word boundary
 * already pins the right one, while the left is where the interesting collisions
 * are. `code.includes('fetch(')` matches `store.refetch()` and `cache.prefetch()`,
 * so the first developer to name a variable after the API they were avoiding
 * would be told their code was wrong — and a guard that reports correct code as
 * broken is a guard the team learns to delete rather than satisfy.
 *
 * @param {string} code comment-blanked source (see {@link scanCode})
 * @param {string} needle
 * @returns {number} offset into `code`, or -1
 */
export function findCodeNeedle(code, needle) {
  for (let at = code.indexOf(needle); at !== -1; at = code.indexOf(needle, at + 1)) {
    const prev = at === 0 ? '' : code[at - 1];
    if (!/[A-Za-z0-9_$]/.test(prev)) return at;
  }
  return -1;
}

/**
 * 1-based line number of `offset` within `text`.
 *
 * Counting over the *blanked* copy is safe, and deliberately so: `blank` only
 * ever replaces a character with a space and never shifts a line, so every
 * `\n` sits at the same offset in `code` as in the file on disk. The line a
 * guard reports is therefore the line a human opens in their editor, with no
 * second copy of the source carried around to drift out of sync.
 *
 * @param {string} text
 * @param {number} offset
 * @returns {number}
 */
export function lineAtOffset(text, offset) {
  let line = 1;
  for (let i = 0; i < offset && i < text.length; i += 1) if (text[i] === '\n') line += 1;
  return line;
}
