/**
 * Shared primitives for the i18n sync toolchain.
 *
 * ## What the release gate means, in one place
 *
 * `scripts/i18n-sync-check.mjs` is the **release blocker** (`.github/workflows/
 * release.yml` → `pre-release-guards`): it compares the English domain packs
 * (`src/locales/en/<domain>.ts`, the single source of truth) against every
 * non-English locale and fails the build when a locale is behind. The rules
 * the whole toolchain has to agree on:
 *
 * 1. **The key set must match exactly.** Every non-English locale carries the
 *    same keys as the English packs, no more and no fewer. The gate reports
 *    `Missing` and `Extra` for both directions.
 * 2. **Stale = byte-identical to English.** For every key whose English
 *    changed or was added since the compared ref, a locale whose value is
 *    *byte-for-byte equal* to the English is reported as **stale** — the
 *    heuristic for "this string was never translated".
 *    ⚠️ It is a *byte* comparison with no notion of "this value is a
 *    technical token that legitimately stays the same". A locale that keeps
 *    `NULL` / `DEFAULT` / `≈ {count}` verbatim is flagged stale, which is
 *    why the toolchain below compares against the English byte-wise and
 *    reports the difference — a universal-symbol whitelist for the stale rule
 *    is tracked separately (R stage), not invented here.
 * 3. **`LOCALE_FILES` lists 9 locales.** `de es fr ja ko pt-BR ru zh-TW zh-CN`
 *    — zh-CN is included since it was added to the gate; every tool in this
 *    toolchain therefore treats zh-CN like any other locale (no special-casing
 *    on the Chinese scripts).
 *
 * ## What a placeholder is, and what is not
 *
 * A placeholder is `{name}` — braces, a name, no whitespace. The set of
 * placeholder *names* in a translation must equal the English set: a
 * translation may reorder them, but it may not rename, drop, or invent one.
 *
 * Two brace-shaped things are **not** placeholders and must be ignored:
 *   - **Shell-style tab stops**: `${1:placeholder}`, `${N}` — a literal the
 *     editor substitutes. The label inside is *supposed* to be translated
 *     (`${1:table_name}` vs `${1:table_name}`), so comparing it as a
 *     placeholder reports a false difference.
 *   - **JSON examples**: `{ "version": 2, "sourceConnectionId": "..." }` — a
 *     literal JSON snippet, not an interpolation.
 * `extractPlaceholders` implements exactly that distinction.
 */

/** The locales the release gate checks (mirrors `LOCALE_FILES` in the gate). */
export const LOCALE_FILES = ['de', 'es', 'fr', 'ja', 'ko', 'pt-BR', 'ru', 'zh-TW', 'zh-CN'];

/**
 * Match one locale-pack entry: `'key': 'value'` or `'key': "value"`.
 *
 * The value may sit on the same line as the key, or on the next one when
 * prettier wrapped a long value. Both shapes are matched, because the anchor
 * logic has to know where an entry *ends*, not just where it starts.
 *
 * Deliberately NOT global: a `g` flag would carry `lastIndex` across lines
 * and silently skip entries.
 */
const ENTRY_RE = /^\s*'((?:[^'\\]|\\.)*)':\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")?/;

/**
 * Parse a locale pack into an ordered list of entries.
 *
 * @param {string} source
 * @returns {{ key: string, value: string, line: number, endLine: number }[]}
 *   `line` is the 0-based index of the line the key is DECLARED on; `endLine`
 *   is the 0-based index of the line the entry ENDS on (the same line for a
 *   one-line entry, the next one for a prettier-wrapped value).
 */
export function parseEntries(source) {
  const lines = source.split('\n');
  const entries = [];
  for (let i = 0; i < lines.length; i++) {
    const m = ENTRY_RE.exec(lines[i]);
    if (!m) continue;
    const key = m[1].replace(/\\'/g, "'");
    if (m[2] !== undefined || m[3] !== undefined) {
      const raw = m[2] ?? m[3] ?? '';
      entries.push({
        key,
        value: raw
          .replace(/\\'/g, "'")
          .replace(/\\"/g, '"')
          .replace(/\\\\/g, '\\'),
        line: i,
        endLine: i,
      });
      continue;
    }
    // The key has no inline value: prettier put the value on the next line.
    // The value may be single- OR double-quoted — French packs use double
    // quotes for the values that contain an apostrophe.
    const next = lines[i + 1] ?? '';
    const valueMatch = /^\s*(?:'((?:[^'\\]|\\.)*)'|"((?:[^"\\]|\\.)*)")/.exec(next);
    entries.push({
      key,
      value: valueMatch
        ? (valueMatch[1] ?? valueMatch[2] ?? '')
            .replace(/\\'/g, "'")
            .replace(/\\"/g, '"')
            .replace(/\\\\/g, '\\')
        : '',
      line: i,
      endLine: valueMatch ? i + 1 : i,
    });
  }
  return entries;
}

/**
 * Parse a locale pack into a plain key → value map.
 *
 * @param {string} source
 * @returns {Record<string, string>}
 */
export function parsePack(source) {
  const out = {};
  for (const e of parseEntries(source)) out[e.key] = e.value;
  return out;
}

/**
 * The placeholder names in a string, as a Set.
 *
 * `{name}` counts. `${1:tab}` and JSON braces do not: a tab stop's label is
 * meant to be translated, and a JSON snippet is a literal.
 *
 * @param {string} value
 * @returns {Set<string>}
 */
export function extractPlaceholders(value) {
  const out = new Set();
  const re = /\{([A-Za-z_][A-Za-z0-9_]*)\}/g;
  let m;
  while ((m = re.exec(value)) !== null) {
    const before = value[m.index - 1];
    // `$` before the brace ⇒ a shell-style tab stop or variable
    // (`${1:placeholder}`, `${N}`), whose label is meant to be translated.
    if (before === '$') continue;
    // A JSON snippet is recognised by its SHAPE — a quoted key followed by a
    // colon — not by the character in front of the brace: an ordinary
    // placeholder is often written inside quotes (`"{name}"`), and that must
    // still count.
    const after = value[m.index + m[0].length];
    if ((before === '"' || before === "'") && after === ':') continue;
    out.add(m[1]);
  }
  return out;
}

/**
 * Do two placeholder sets agree?
 *
 * @param {string} a
 * @param {string} b
 * @returns {boolean}
 */
export function samePlaceholders(a, b) {
  const pa = extractPlaceholders(a);
  const pb = extractPlaceholders(b);
  if (pa.size !== pb.size) return false;
  for (const p of pa) if (!pb.has(p)) return false;
  return true;
}

/**
 * Escape a value for a single-quoted TypeScript string literal.
 *
 * @param {string} value
 * @returns {string}
 */
export function escapeTsValue(value) {
  return value.replace(/\\/g, '\\\\').replace(/'/g, "\\'").replace(/\n/g, '\\n');
}

/**
 * Render one pack entry the way prettier would.
 *
 * Prettier keeps `'key': 'value',` on one line while it fits the print width
 * and otherwise splits the value onto the next line, indented. Reproducing
 * that here keeps an inserted file prettier-clean without shelling out.
 *
 * @param {string} key
 * @param {string} value
 * @param {number} printWidth
 * @returns {string}
 */
export function renderEntry(key, value, printWidth = 100) {
  const oneLine = `  '${key}': '${escapeTsValue(value)}',`;
  if (oneLine.length <= printWidth) return oneLine;
  return `  '${key}':\n    '${escapeTsValue(value)}',`;
}
