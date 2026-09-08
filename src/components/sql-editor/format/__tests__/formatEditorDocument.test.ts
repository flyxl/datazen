/**
 * §4.2 Configurable beautify + selection-scoped formatting.
 */
import { describe, expect, it } from 'vitest';
import { EditorState, EditorSelection } from '@codemirror/state';
import { formatEditorDocument } from '../formatEditorDocument';
import { formatSql, DEFAULT_SQL_FORMAT_OPTIONS } from '../../../../lib/sqlFormat';

function createHarness(doc: string, selection?: { anchor: number; head: number }) {
  let state = EditorState.create({
    doc,
    selection: selection ? EditorSelection.single(selection.anchor, selection.head) : undefined,
  });
  const view = {
    get state() {
      return state;
    },
    dispatch: (tr: any) => {
      state = tr.state ?? state.update(tr).state;
    },
  };
  return {
    view,
    get doc() {
      return state.doc.toString();
    },
    get selection() {
      return state.selection.main;
    },
    get selectedText() {
      return state.sliceDoc(state.selection.main.from, state.selection.main.to);
    },
  };
}

describe('formatSql — configurable options', () => {
  const sql = 'select id, name from users where a = 1 and b = 2';

  it('defaults reproduce the pre-§4.2 uppercase behaviour', () => {
    expect(formatSql(sql, 'postgresql')).toContain('SELECT');
  });

  it('honours keywordCase lower', () => {
    const out = formatSql(sql, 'postgresql', { keywordCase: 'lower' });
    expect(out).toContain('select');
    expect(out).not.toContain('SELECT');
  });

  it('honours keywordCase preserve', () => {
    const out = formatSql('SeLeCt 1', 'postgresql', { keywordCase: 'preserve' });
    expect(out).toContain('SeLeCt');
  });

  it('applies 4-space indentation', () => {
    const out = formatSql(sql, 'postgresql', { indentStyle: '4spaces' });
    expect(out).toMatch(/\n {4}\S/);
  });

  it('applies tab indentation', () => {
    const out = formatSql(sql, 'postgresql', { indentStyle: 'tab' });
    expect(out).toMatch(/\n\t\S/);
  });

  it('breaks before boolean operators when enabled', () => {
    const out = formatSql(sql, 'postgresql', { breakBeforeBooleanOperators: true });
    expect(out).toMatch(/\n\s*AND\b/);
  });

  it('breaks after boolean operators when disabled', () => {
    const out = formatSql(sql, 'postgresql', { breakBeforeBooleanOperators: false });
    expect(out).toMatch(/\bAND\s*\n/);
    expect(out).not.toMatch(/\n\s*AND\b/);
  });

  it('controls blank lines between statements', () => {
    const two = 'select 1; select 2;';
    expect(formatSql(two, 'postgresql', { linesBetweenQueries: 0 })).not.toMatch(/\n\s*\n/);
    expect(formatSql(two, 'postgresql', { linesBetweenQueries: 2 })).toMatch(/\n\s*\n\s*\n/);
  });

  it('leaves blank input untouched', () => {
    expect(formatSql('   ', 'postgresql')).toBe('   ');
  });
});

describe('formatEditorDocument — selection scope', () => {
  it('formats only the selection and leaves surrounding code byte-identical', () => {
    const before = 'SELECT 1;\n';
    const target = 'select  id,name   from users';
    const after = '\nSELECT 3;';
    const harness = createHarness(before + target + after, {
      anchor: before.length,
      head: before.length + target.length,
    });

    expect(formatEditorDocument(harness.view as any, { databaseType: 'postgresql' })).toBe(true);

    expect(harness.doc.startsWith(before)).toBe(true);
    expect(harness.doc.endsWith(after)).toBe(true);
    expect(harness.doc).not.toContain('select  id,name');
  });

  it('keeps the formatted block selected so it can be re-run immediately', () => {
    const target = 'select  id,name   from users';
    const harness = createHarness(target, { anchor: 0, head: target.length });

    formatEditorDocument(harness.view as any, { databaseType: 'postgresql' });

    expect(harness.selection.empty).toBe(false);
    expect(harness.selectedText).toBe(harness.doc);
  });

  it('preserves the leading indentation of an indented selection', () => {
    const doc = '  select  1';
    const harness = createHarness(doc, { anchor: 2, head: doc.length });

    formatEditorDocument(harness.view as any, { databaseType: 'postgresql' });

    expect(harness.doc.startsWith('  ')).toBe(true);
  });

  it('formats the whole document when there is no selection', () => {
    const harness = createHarness('select  id,name   from users');

    expect(formatEditorDocument(harness.view as any, { databaseType: 'postgresql' })).toBe(true);
    expect(harness.doc).toContain('SELECT');
    expect(harness.selection.empty).toBe(true);
  });

  it('anchors the cursor to the same statement after a whole-document format', () => {
    // Cursor starts inside the third statement; after reflow it must still be
    // there rather than at a stale absolute offset.
    const doc = 'select 1;\nselect 2;\nselect  3   from t;';
    const cursor = doc.lastIndexOf('from');
    const harness = createHarness(doc, { anchor: cursor, head: cursor });

    formatEditorDocument(harness.view as any, { databaseType: 'postgresql' });

    const third = harness.doc.lastIndexOf('SELECT');
    expect(harness.selection.head).toBeGreaterThanOrEqual(third);
  });

  it('is a no-op on an empty document', () => {
    const harness = createHarness('   ');
    expect(formatEditorDocument(harness.view as any)).toBe(false);
    expect(harness.doc).toBe('   ');
  });

  it('leaves the buffer intact when the formatter cannot parse the input', () => {
    // Mid-edit SQL is routinely unparseable; corrupting it would be far worse
    // than doing nothing.
    const doc = "select * from t where x = 'unterminated";
    const harness = createHarness(doc);
    formatEditorDocument(harness.view as any, { databaseType: 'postgresql' });
    expect(harness.doc.includes('unterminated')).toBe(true);
  });

  it('reports false when formatting changes nothing', () => {
    const already = formatSql('select 1', 'postgresql', DEFAULT_SQL_FORMAT_OPTIONS);
    const harness = createHarness(already);
    expect(formatEditorDocument(harness.view as any, { databaseType: 'postgresql' })).toBe(false);
  });
});
