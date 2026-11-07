/**
 * DB-03 §3.3 — the **sentences** a rejected paste puts in front of the user.
 *
 * The code paths were covered; the words were not. Three defects lived in exactly that gap:
 *
 * 1. A read-only column produced "…cannot be written and were skipped: auto-increment" — the
 *    *reason* was rendered where the *column name* belongs, so the sentence named no column.
 * 2. Dropped source columns were reported twice: once by the planner with the names, once by
 *    the hook without them, and the nameless copy formatted as "0 … : ''" — and it came
 *    first, so that is what the user read.
 * 3. The cell limit in the "too large" sentence was a literal, free to drift from the
 *    constant the parser actually enforces.
 *
 * These tests translate through the real engine with the real English pack: asserting on the
 * key plus params would have passed while the sentence stayed wrong.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import enQuery from '../../locales/en/query';
import { GRID_PASTE_MAX_CELLS } from '../../lib/gridClipboard';
import { describePasteProblem } from '../useGridClipboard';

let translate: (key: string, params?: Record<string, string | number>) => string;

/** Render a problem's sentence the way the status line does. */
const message = (problem: Parameters<typeof describePasteProblem>[0]) =>
  describePasteProblem(problem, (key, params) => translate(key, params));

beforeEach(async () => {
  // Reset first so every test starts from one consistent module instance: the engine's
  // registry is module state, and the override test swaps a module out. Registering into a
  // half-reset graph is how a suite ends up asserting raw keys.
  vi.resetModules();
  // The real engine with the real English pack: these assertions are about what the user
  // reads, so a fake `t` that echoes keys would defeat the point.
  const i18n = await import('@datazen/ui');
  i18n.registerTranslations({ en: enQuery as unknown as Record<string, string> });
  i18n.setLocale('en');
  translate = i18n.t;
});

describe('describePasteProblem', () => {
  it('names the read-only column instead of the reason it is read-only', () => {
    const text = message({ code: 'grid.cell.readOnly', row: null, column: 'id', detail: 'auto-increment' });

    expect(text).toContain('id');
    // The reason is a diagnosis, not a name: it must not be printed where the column goes.
    expect(text).not.toContain('auto-increment');
    expect(text).toContain('1 column(s)');
  });

  it('says the whole panel is read-only when no column was reached', () => {
    // No column ⇒ the paste never got as far as a column, so the message is about the panel.
    expect(message({ code: 'grid.cell.readOnly', row: null, column: null, detail: 'readOnly' })).toBe(
      enQuery['dataTable.readOnlyPasteDisabled'],
    );
  });

  it('takes the cell limit from the constant the parser enforces', () => {
    const text = message({ code: 'grid.paste.tooLarge', row: null, column: null, detail: '100001' });

    expect(text).toContain('100001');
    // Pins the *link*: change `GRID_PASTE_MAX_CELLS` without updating this sentence and the
    // sentence fails here instead of lying to the user in production.
    expect(text).toContain(String(GRID_PASTE_MAX_CELLS));
  });

  it('follows the limit constant even when it changes', async () => {
    // The test above can only catch a *future* drift (the literal and the constant agree
    // today). Shrinking the constant is what proves the sentence reads it at all: a
    // hard-coded 100000 would keep saying 100000 here.
    vi.resetModules();
    vi.doMock('../../lib/gridClipboard', async (importOriginal) => {
      const actual = await importOriginal<Record<string, unknown>>();
      return { ...actual, GRID_PASTE_MAX_CELLS: 7 };
    });
    try {
      const fresh = await import('../useGridClipboard');
      const text = fresh.describePasteProblem(
        { code: 'grid.paste.tooLarge', row: null, column: null, detail: '8' },
        (key, params) => translate(key, params),
      );
      expect(text).toContain('8');
      expect(text).toContain('7');
      expect(text).not.toContain('100000');
    } finally {
      vi.doUnmock('../../lib/gridClipboard');
      vi.resetModules();
    }
  });

  it('renders dropped source columns with their names and the right count', () => {
    const text = message({
      code: 'grid.paste.columnMismatch',
      row: null,
      column: null,
      detail: 'a, b',
    });

    expect(text).toContain('2 source column(s)');
    expect(text).toContain('a, b');
    // The defect this pins: a copy without `detail` rendered "0 … : ''".
    expect(text).not.toContain('0 source column');
  });

  it('numbers rows from 1 and quotes the column for a NOT NULL refusal', () => {
    const text = message({
      code: 'grid.paste.notNullEmpty',
      row: 2,
      column: 'score',
      detail: 'notNull',
    });

    expect(text).toContain('Row 3');
    expect(text).toContain('score');
  });

  it('reports how many cells were clipped by the page edge', () => {
    expect(message({ code: 'grid.paste.outOfBounds', row: null, column: null, detail: '7' })).toContain(
      '7 cell(s)',
    );
  });
});
