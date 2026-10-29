/** @vitest-environment node */
/**
 * The i18n sync toolchain: `scripts/i18n-sync-apply.mjs`,
 * `scripts/i18n-sync-verify.mjs`, `scripts/i18n-sync-verify-batch.mjs` and
 * `scripts/i18n-sync-harmonize.mjs`.
 *
 * These tools are the write/audit half of the i18n sync round (the read half
 * is `scripts/i18n-sync-check.mjs`, the release blocker). Every round re-runs
 * them, so the properties that matter are:
 *
 *  · **the wrapped-value bug** — prettier wraps a long value onto the next
 *    line, and an earlier version of the applier anchored on the key's
 *    DECLARATION line. That put new keys between a key and its value and
 *    left the file syntactically broken. The regression test below feeds the
 *    applier exactly that shape and asserts the file still parses.
 *  · **idempotence** — the same batch applied twice must change nothing the
 *    second time, or every rebase of the English side would double-apply.
 *  · **the gate's rules** — stale is a byte comparison; the placeholder set
 *    is a NAME set, and `${1:tab}` / JSON braces are not placeholders.
 */
import { describe, expect, it, afterEach, beforeEach } from 'vitest';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { applyBatch, planInsertion } from '../i18n-sync-apply.mjs';
import { verifyBatch } from '../i18n-sync-verify-batch.mjs';
import { verifyLocale } from '../i18n-sync-verify.mjs';
import { harmonizeLocale } from '../i18n-sync-harmonize.mjs';
import { extractPlaceholders, parseEntries, renderEntry } from '../lib/i18nSync.mjs';

/** A minimal English pack with one prettier-wrapped (two-line) entry. */
const EN_PACK = `/** Auto-split domain: query (en) */
const pack = {
  'filter.filter': 'Filter',
  'pagination.estimateTooltip':
    'Estimated row count — the real number may differ. Use Count for the exact value.',
  'filter.clear': 'Clear',
} as const;
export default pack;
`;

/** The same pack with the wrapped entry written out as one long line. */
const EN_PACK_INLINE = `/** Auto-split domain: query (en) */
const pack = {
  'filter.filter': 'Filter',
  'pagination.estimateTooltip': 'Estimated row count — the real number may differ. Use Count for the exact value.',
  'filter.clear': 'Clear',
} as const;
export default pack;
`;

/** A target pack whose only entry is the one with the wrapped value. */
const LOCALE_PACK = `/** Auto-split domain: query (de) */
const pack = {
  'filter.filter': 'Filter',
  'pagination.estimateTooltip':
    'Geschätzte Zeilenanzahl — die tatsächliche Zahl kann abweichen. Verwenden Sie „Zählen“ für den exakten Wert.',
} as const;
export default pack;
`;

/** The key the batch adds: it is NOT in the target pack yet. */
const NEW_KEY = 'filter.clear';
const NEW_VALUE = 'Löschen';

/** A scratch repo with the locale packs the tools read. */
let root;

beforeEach(() => {
  root = mkdtempSync(join(tmpdir(), 'datazen-i18n-sync-'));
  mkdirSync(join(root, 'packages/frontend/src/locales/en'), { recursive: true });
  mkdirSync(join(root, 'packages/frontend/src/locales/de'), { recursive: true });
  writeFileSync(join(root, 'packages/frontend/src/locales/en/query.ts'), EN_PACK);
  writeFileSync(join(root, 'packages/frontend/src/locales/de/query.ts'), LOCALE_PACK);
});

afterEach(() => {
  rmSync(root, { recursive: true, force: true });
});

/** Read the target pack back. */
const readLocale = () => readFileSync(join(root, 'packages/frontend/src/locales/de/query.ts'), 'utf-8');

/** The keys a source string declares, in order. */
const keysOf = (source) => parseEntries(source).map((e) => e.key);

describe('i18n sync toolchain — the wrapped-value bug', () => {
  it('does not split a key from its prettier-wrapped value', () => {
    // The anchor must be the line the entry ENDS on. Anchoring on the
    // declaration line would splice the new key between
    //   'pagination.estimateTooltip':
    // and its value, which is exactly what broke the file last round.
    const plan = planInsertion(
      EN_PACK,
      LOCALE_PACK,
      { [NEW_KEY]: NEW_VALUE },
      false,
    );
    const source = plan.lines.join('\n');
    const entries = parseEntries(source);
    const tooltip = entries.find((e) => e.key === 'pagination.estimateTooltip');
    expect(tooltip).toBeDefined();
    expect(tooltip.value).toBe(
      'Geschätzte Zeilenanzahl — die tatsächliche Zahl kann abweichen. Verwenden Sie „Zählen“ für den exakten Wert.',
    );
    // The value is still on the line after the key, and the new key is not
    // wedged between them.
    const lines = source.split('\n');
    const declIndex = lines.findIndex((l) => l.includes("'pagination.estimateTooltip'"));
    expect(lines[declIndex + 1].trim().startsWith("'Geschätzte")).toBe(true);
  });

  it('keeps the file syntactically valid after an insertion', () => {
    const result = applyBatch(
      root,
      'de',
      { 'query.ts': { [NEW_KEY]: NEW_VALUE } },
      {},
    );
    expect(result.conflicts).toEqual([]);
    expect(result.inserted).toEqual([NEW_KEY]);
    // The pack still declares every key exactly once, and the wrapped
    // entry still holds its own value.
    const entries = parseEntries(readLocale());
    expect(entries.map((e) => e.key)).toEqual([
      'filter.filter',
      'pagination.estimateTooltip',
      'filter.clear',
    ]);
    expect(entries.find((e) => e.key === 'pagination.estimateTooltip').value).toContain('Geschätzte');
    expect(entries.find((e) => e.key === NEW_KEY).value).toBe(NEW_VALUE);
  });

  it('places a new key right after its English predecessor', () => {
    applyBatch(root, 'de', { 'query.ts': { [NEW_KEY]: NEW_VALUE } }, {});
    const keys = keysOf(readLocale());
    // The new key follows 'pagination.estimateTooltip' in the English
    // order, so it must follow it here too.
    expect(keys.indexOf(NEW_KEY)).toBe(keys.indexOf('pagination.estimateTooltip') + 1);
  });
});

describe('i18n sync toolchain — idempotence', () => {
  it('is a no-op when the same batch is applied twice', () => {
    const batch = { 'query.ts': { [NEW_KEY]: NEW_VALUE } };
    const first = applyBatch(root, 'de', batch, {});
    expect(first.inserted).toEqual([NEW_KEY]);
    const afterFirst = readLocale();

    const second = applyBatch(root, 'de', batch, {});
    expect(second.inserted).toEqual([]);
    expect(second.skipped).toEqual([NEW_KEY]);
    expect(second.conflicts).toEqual([]);
    expect(readLocale()).toBe(afterFirst);
  });

  it('reports a conflict when the value differs, and --force overwrites it', () => {
    applyBatch(root, 'de', { 'query.ts': { [NEW_KEY]: NEW_VALUE } }, {});
    const conflict = applyBatch(root, 'de', { 'query.ts': { [NEW_KEY]: 'Verwerfen' } }, {});
    expect(conflict.conflicts).toHaveLength(1);
    expect(conflict.conflicts[0]).toContain('already present with a different value');
    // The conflicting run wrote nothing.
    expect(
      parseEntries(readLocale()).find((e) => e.key === NEW_KEY).value,
    ).toBe(NEW_VALUE);

    const forced = applyBatch(root, 'de', { 'query.ts': { [NEW_KEY]: 'Verwerfen' } }, { force: true });
    expect(forced.conflicts).toEqual([]);
    expect(
      parseEntries(readLocale()).find((e) => e.key === NEW_KEY).value,
    ).toBe('Verwerfen');
  });

  it('rejects a key the English side does not have', () => {
    const result = applyBatch(root, 'de', { 'query.ts': { 'filter.nonexistent': 'x' } }, {});
    expect(result.conflicts).toHaveLength(1);
    expect(result.conflicts[0]).toContain('not present in the English pack');
  });
});

describe('i18n sync toolchain — the gate rules the tools encode', () => {
  it('treats stale as a byte comparison against English', () => {
    writeFileSync(
      join(root, 'packages/frontend/src/locales/de/query.ts'),
      `/** Auto-split domain: query (de) */
const pack = {
  'filter.filter': 'Filter',
  'pagination.estimateTooltip':
    'Geschätzte Zeilenanzahl — die tatsächliche Zahl kann abweichen. Verwenden Sie „Zählen“ für den exakten Wert.',
  'filter.clear': 'Clear',
} as const;
export default pack;
`,
    );
    const { report } = verifyLocale(root, 'de', 'HEAD');
    // 'filter.clear' is byte-identical to the English → the gate's stale rule.
    // 'filter.filter' is too ('Filter' is the same word in both).
    expect(report.identicalCount).toBe(2);
    expect(report.identicalCount).toBeGreaterThan(0);
  });

  it('does not count a tab stop or a JSON brace as a placeholder', () => {
    // `${1:placeholder}` is a shell-style tab stop whose label is meant to
    // be translated; `{ "version": 2 }` is a literal JSON snippet. Neither is
    // an i18n placeholder, so neither may be compared as one.
    expect([...extractPlaceholders('Use ${1:placeholder} for ${N}')]).toEqual([]);
    expect([...extractPlaceholders('{ "version": 2, "sourceConnectionId": "..." }')]).toEqual([]);
    expect([...extractPlaceholders('{count} rows × {cols} columns')]).toEqual(['count', 'cols']);
  });

  it('reads a double-quoted value, as the French packs use', () => {
    // French packs double-quote the values that contain an apostrophe, and
    // prettier wraps the long ones onto the next line. A parser that only
    // understood single quotes would read those values as empty and then
    // report the placeholders inside them as missing.
    const source = [
      '/** doc */',
      'const pack = {',
      "  'sync.explainDiffPrompt':",
      '    "Résumez ce résultat (lignes {count}) en langage clair.\\n\\n{report}",',
      "  'other.key': 'x',",
      '} as const;',
      'export default pack;',
      '',
    ].join('\n');
    const entries = parseEntries(source);
    const prompt = entries.find((e) => e.key === 'sync.explainDiffPrompt');
    expect(prompt.value).toContain('{count}');
    expect(prompt.value).toContain('{report}');
    expect(prompt.endLine).toBe(prompt.line + 1);
    expect([...extractPlaceholders(prompt.value)].sort()).toEqual(['count', 'report']);
  });

  it('counts a placeholder that sits inside quotes', () => {
    // `"{name}"` is an ordinary placeholder written inside quotes; only a
    // JSON-shaped `"key":` is a literal.
    expect([...extractPlaceholders('Delete connection "{name}"?')]).toEqual(['name']);
    expect([...extractPlaceholders('Delete connection \\"{name}\\"?')]).toEqual(['name']);
    expect([...extractPlaceholders('{"version": 2}')]).toEqual([]);
  });

  it('compares placeholder names as a set', () => {
    // A translation may reorder the placeholders; that is not a difference.
    const { problems } = verifyBatch(root, {
      'query.ts': {
        'filter.filter': 'Filter {a} {b}',
      },
    });
    // The English has no {a}/{b} at all, so this IS a difference.
    expect(problems.join('\n')).toContain('placeholder mismatch');
  });
});

describe('i18n sync toolchain — batch verification', () => {
  it('accepts a clean batch', () => {
    const { problems } = verifyBatch(root, {
      'query.ts': { 'filter.clear': 'Zurücksetzen ist nicht leer' },
    });
    expect(problems).toEqual([]);
  });

  it('reports a byte-identical value, a missing English key and a raw apostrophe', () => {
    const { problems } = verifyBatch(root, {
      'query.ts': {
        'filter.clear': 'Clear', // byte-identical → stale
        'filter.nope': 'x', // not an English key
        'filter.filter': "lit's", // raw apostrophe
      },
    });
    const text = problems.join('\n');
    expect(text).toContain('byte-identical to English');
    expect(text).toContain('is not an English key');
    expect(text).toContain('raw apostrophe');
  });
});

describe('i18n sync toolchain — terminology harmonization', () => {
  it('applies a rule and is a no-op the second time', () => {
    const rules = {
      de: {
        'pagination.estimateTooltip': {
          from: 'Geschätzte Zeilenanzahl — die tatsächliche Zahl kann abweichen. Verwenden Sie „Zählen“ für den exakten Wert.',
          to: 'Geschätzte Zeilenanzahl — der exakte Wert weicht ab. „Zählen“ liefert ihn.',
        },
      },
    };
    const first = harmonizeLocale(root, 'de', rules.de, {});
    expect(first.changed).toEqual(['pagination.estimateTooltip']);
    expect(first.problems).toEqual([]);
    expect(
      parseEntries(readLocale()).find((e) => e.key === 'pagination.estimateTooltip').value,
    ).toBe('Geschätzte Zeilenanzahl — der exakte Wert weicht ab. „Zählen“ liefert ihn.');

    const second = harmonizeLocale(root, 'de', rules.de, {});
    expect(second.changed).toEqual([]);
    expect(second.noop).toEqual(['pagination.estimateTooltip']);
  });

  it('refuses a rule whose expected old value no longer matches', () => {
    const result = harmonizeLocale(
      root,
      'de',
      { 'pagination.estimateTooltip': { from: 'Etwas ganz anderes', to: 'Geschätzt' } },
      {},
    );
    expect(result.changed).toEqual([]);
    expect(result.problems).toHaveLength(1);
    expect(result.problems[0]).toContain('expected');
    // Nothing was written.
    expect(
      parseEntries(readLocale()).find((e) => e.key === 'pagination.estimateTooltip').value,
    ).toContain('Geschätzte Zeilenanzahl');
  });
});

describe('i18n sync toolchain — entry rendering', () => {
  it('wraps a long value the way prettier does', () => {
    const long = 'x'.repeat(120);
    const rendered = renderEntry('some.key', long);
    expect(rendered.split('\n')).toHaveLength(2);
    expect(rendered.split('\n')[1].trim().startsWith("'")).toBe(true);
  });

  it('keeps a short value on one line', () => {
    expect(renderEntry('some.key', 'short')).toBe("  'some.key': 'short',");
  });
});
