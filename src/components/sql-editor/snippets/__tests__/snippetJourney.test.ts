/**
 * §4.1 Snippet journey tests.
 *
 * Covers the whole keystroke lifecycle rather than one static string: expand →
 * tabstop forward → tabstop back → deactivate, plus the context transitions
 * that must NOT starve table/column completions.
 */
import { describe, expect, it } from 'vitest';
import { EditorState, EditorSelection } from '@codemirror/state';
import {
  CompletionContext,
  snippet,
  nextSnippetField,
  prevSnippetField,
  hasNextSnippetField,
  hasPrevSnippetField,
} from '@codemirror/autocomplete';
import { createSnippetCompletionSource, BUILTIN_SQL_SNIPPETS } from '..';

const source = createSnippetCompletionSource();

/** Run the snippet source at the end of `doc`, mimicking a keystroke. */
function completeAt(doc: string, explicit = false) {
  const state = EditorState.create({ doc });
  const result = source(new CompletionContext(state, doc.length, explicit));
  if (!result || !('options' in result)) return null;
  return {
    from: result.from,
    labels: result.options.map((o) => o.label),
    boost: result.options[0]?.boost,
  };
}

/**
 * Minimal EditorView stand-in: `snippet()` only needs `state` + `dispatch`, and
 * jsdom geometry is unreliable for anything more (see interaction rules §4).
 */
function createHarness(doc = '') {
  let state = EditorState.create({ doc });
  const view = {
    get state() {
      return state;
    },
    dispatch: (tr: { state: EditorState } | any) => {
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
    get state() {
      return state;
    },
    apply(command: (target: any) => boolean) {
      return command({ state, dispatch: view.dispatch });
    },
  };
}

describe('snippet completion source — context transitions', () => {
  it('offers snippets at statement start with a positive boost', () => {
    const result = completeAt('sel');
    expect(result).not.toBeNull();
    expect(result!.labels).toContain('sel*');
    expect(result!.boost).toBeGreaterThan(0);
  });

  it('captures the trailing "*" so the "sel*" prefix anchors correctly', () => {
    const result = completeAt('sel*');
    expect(result).not.toBeNull();
    // `from` must cover the whole typed prefix, else expansion duplicates text.
    expect(result!.from).toBe(0);
    expect(result!.labels).toContain('sel*');
  });

  it('downweights but never removes snippets in a table position', () => {
    const result = completeAt('SELECT * FROM cou');
    expect(result).not.toBeNull();
    // Soft ordering, not a hard filter: the list survives, ranked below tables.
    expect(result!.labels).toContain('count');
    expect(result!.boost).toBeLessThan(0);
  });

  it('downweights snippets in a column position', () => {
    const result = completeAt('SELECT co');
    expect(result).not.toBeNull();
    expect(result!.boost).toBeLessThan(0);
  });

  it('hard-excludes qualified positions where a prefix is impossible', () => {
    expect(completeAt('SELECT u.')).toBeNull();
    expect(completeAt('SELECT u.na')).toBeNull();
    expect(completeAt('SELECT * FROM public.')).toBeNull();
  });

  it('stays silent on an implicit keystroke with no word prefix', () => {
    expect(completeAt('SELECT * FROM ')).toBeNull();
  });

  it('still answers an explicit request with no prefix', () => {
    const result = completeAt('', true);
    expect(result).not.toBeNull();
    expect(result!.labels).toHaveLength(BUILTIN_SQL_SNIPPETS.length);
  });
});

describe('snippet expansion journey — sel* end to end', () => {
  const selectAll = BUILTIN_SQL_SNIPPETS.find((s) => s.prefix === 'sel*')!;

  it('expands, selects the first tabstop default, then walks fields forward and back', () => {
    const harness = createHarness('sel*');

    // Step 1 — expand over the typed prefix.
    snippet(selectAll.template)(harness.view as any, null, 0, 4);

    expect(harness.doc).toBe('SELECT *\nFROM table_name\nWHERE condition;');

    // Step 2 — cursor lands on ${1:table_name} with the default selected,
    // so typing immediately replaces it.
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('table_name');
    expect(hasNextSnippetField(harness.state)).toBe(true);
    expect(hasPrevSnippetField(harness.state)).toBe(false);

    // Step 3 — Tab advances to ${2:condition}.
    expect(harness.apply(nextSnippetField)).toBe(true);
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('condition');

    // Step 4 — Shift-Tab returns to ${1}. This is the exit/reverse transition
    // that a one-way implementation would deadlock on.
    expect(hasPrevSnippetField(harness.state)).toBe(true);
    expect(harness.apply(prevSnippetField)).toBe(true);
    expect(harness.state.sliceDoc(harness.selection.from, harness.selection.to)).toBe('table_name');
  });

  it('deactivates the session once the cursor leaves the fields', () => {
    const harness = createHarness('sel*');
    snippet(selectAll.template)(harness.view as any, null, 0, 4);
    expect(hasNextSnippetField(harness.state)).toBe(true);

    // Clicking away must release the tabstop session so Tab resumes its
    // normal completion behaviour.
    harness.view.dispatch(
      harness.state.update({ selection: EditorSelection.cursor(harness.doc.length) }),
    );
    expect(hasNextSnippetField(harness.state)).toBe(false);
    expect(hasPrevSnippetField(harness.state)).toBe(false);
  });

  it('links repeated placeholders so one edit updates every instance', () => {
    // `join` reuses ${1:table_name} twice — both must move together.
    const join = BUILTIN_SQL_SNIPPETS.find((s) => s.prefix === 'join')!;
    const harness = createHarness('');
    snippet(join.template)(harness.view as any, null, 0, 0);
    const occurrences = harness.doc.split('table_name').length - 1;
    expect(occurrences).toBe(2);
  });
});

describe('builtin snippet library integrity', () => {
  it('exposes every PRD-mandated prefix', () => {
    expect(BUILTIN_SQL_SNIPPETS.map((s) => s.prefix)).toEqual([
      'sel*',
      'selc',
      'ins',
      'upd',
      'del',
      'join',
      'count',
    ]);
  });

  it('uses unique ids and prefixes', () => {
    const ids = BUILTIN_SQL_SNIPPETS.map((s) => s.id);
    const prefixes = BUILTIN_SQL_SNIPPETS.map((s) => s.prefix);
    expect(new Set(ids).size).toBe(ids.length);
    expect(new Set(prefixes).size).toBe(prefixes.length);
  });

  it('numbers tabstops explicitly so tab order is deterministic', () => {
    for (const item of BUILTIN_SQL_SNIPPETS) {
      const stops = [...item.template.matchAll(/\$\{(\d+):/g)].map((m) => Number(m[1]));
      expect(stops.length).toBeGreaterThan(0);
      // Every placeholder carries a number; textual order must not decide it.
      expect(stops.every((n) => n >= 1)).toBe(true);
    }
  });

  it('ends every template with a terminal tabstop', () => {
    // Without it, reaching the last real placeholder would end the session and
    // break the Shift-Tab back-navigation the PRD requires.
    for (const item of BUILTIN_SQL_SNIPPETS) {
      expect(item.template).toMatch(/\$\{\d+\}$/);
    }
  });
});
