import { describe, it, expect, vi } from 'vitest';
import { EditorState } from '@codemirror/state';
import {
  createStatementGutterExtension,
  docVersionField,
  IncrementDocVersionEffect,
  type GutterClickCallback,
} from '../statementGutter';
import { statementIndexField } from '../../semantic/statementRanges';
import {
  executionStateField,
  StartExecutionEffect,
  FinishExecutionEffect,
} from '../executionState';
import { frameDegradedField, SetDegradedEffect } from '../statementFrame';

// Create a shared field instance so registration and query use the same field
const stmtIndexField = statementIndexField();

function createState(doc: string, degraded = false) {
  const extensions = [
    stmtIndexField,
    executionStateField,
    frameDegradedField,
    ...createStatementGutterExtension(),
  ];
  let state = EditorState.create({ doc, extensions });
  if (degraded) {
    state = state.update({
      effects: SetDegradedEffect.of(true),
    }).state;
  }
  return state;
}

describe('docVersionField', () => {
  it('[tester] starts at 0', () => {
    const state = createState('SELECT 1;');
    expect(state.field(docVersionField)).toBe(0);
  });

  it('[tester] increments on IncrementDocVersionEffect', () => {
    let state = createState('SELECT 1;');
    state = state.update({
      effects: IncrementDocVersionEffect.of(),
    }).state;
    expect(state.field(docVersionField)).toBe(1);
  });

  it('[tester] increments on document change', () => {
    const state = createState('SELECT 1;');
    const next = state.update({
      changes: { from: 0, to: 8, insert: 'SELECT 2' },
    }).state;
    expect(next.field(docVersionField)).toBe(1);
  });

  it('[tester] increments on both effect and doc change', () => {
    let state = createState('SELECT 1; SELECT 2;');
    state = state.update({
      effects: IncrementDocVersionEffect.of(),
    }).state;
    state = state.update({
      changes: { from: 0, to: 8, insert: 'SELECT 3' },
    }).state;
    expect(state.field(docVersionField)).toBe(2);
  });
});

describe('createStatementGutterExtension', () => {
  it('[tester] returns an array of extensions', () => {
    const ext = createStatementGutterExtension();
    expect(Array.isArray(ext)).toBe(true);
    expect(ext.length).toBeGreaterThan(0);
  });

  it('[tester] includes the docVersionField', () => {
    const ext = createStatementGutterExtension();
    const state = EditorState.create({
      doc: 'SELECT 1;',
      extensions: ext,
    });
    expect(typeof state.field(docVersionField)).toBe('number');
  });

  it('[tester] creates gutter for multi-statement document', () => {
    const doc = 'SELECT 1;\nSELECT 2;\nSELECT 3;';
    const state = createState(doc);
    const ranges = state.field(stmtIndexField, false);
    expect(ranges).toBeDefined();
    expect(ranges!.ranges.length).toBe(3);
  });
});

describe('[tester] gutter execution state integration', () => {
  it('executing a statement updates execution state', () => {
    let state = createState('SELECT 1; SELECT 2;');
    state = state.update({
      effects: StartExecutionEffect.of({
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      }),
    }).state;
    const exec = state.field(executionStateField);
    expect(exec.status).toBe('running');
    expect(exec.targetRange).toEqual({ from: 0, to: 8 });
  });

  it('finishing execution resets state', () => {
    let state = createState('SELECT 1;');
    state = state.update({
      effects: StartExecutionEffect.of({
        targetRange: { from: 0, to: 8 },
        documentVersion: 1,
      }),
    }).state;
    state = state.update({
      effects: FinishExecutionEffect.of(),
    }).state;
    const exec = state.field(executionStateField);
    expect(exec.status).toBe('idle');
  });

  it('degraded mode can be toggled', () => {
    let state = createState('SELECT 1;');
    expect(state.field(frameDegradedField)).toBe(false);

    state = state.update({
      effects: SetDegradedEffect.of(true),
    }).state;
    expect(state.field(frameDegradedField)).toBe(true);

    state = state.update({
      effects: SetDegradedEffect.of(false),
    }).state;
    expect(state.field(frameDegradedField)).toBe(false);
  });
});

describe('[tester] gutter click callback factory', () => {
  it('onExecute option is accepted', () => {
    const onExecute: GutterClickCallback = vi.fn();
    const ext = createStatementGutterExtension({ onExecute });
    expect(ext).toBeDefined();
    expect(Array.isArray(ext)).toBe(true);
  });

  it('maxLineCount option is accepted', () => {
    const ext = createStatementGutterExtension({ maxLineCount: 100 });
    expect(ext).toBeDefined();
  });
});

describe('[tester] statement range gutter markers', () => {
  it('has statement ranges for multi-statement doc', () => {
    const doc = 'SELECT 1;\n\nSELECT 2;\n\nSELECT 3;';
    const state = createState(doc);
    const ranges = state.field(stmtIndexField, false);
    expect(ranges).toBeDefined();
    // Blank lines between statements are filtered out
    expect(ranges!.ranges.length).toBe(3);
  });

  it('has single range for single statement', () => {
    const state = createState('SELECT 1;');
    const ranges = state.field(stmtIndexField, false);
    expect(ranges!.ranges.length).toBe(1);
  });

  it('has no ranges for empty document', () => {
    const state = createState('');
    const ranges = state.field(stmtIndexField, false);
    expect(ranges!.ranges.length).toBe(0);
  });

  it('correctly places play markers on first executable lines only (multiline SQL)', () => {
    const doc = `SELECT
  *
FROM
  er_customers
WHERE
  er_customers."city" = '上海';
SELECT * FROM "test_orders";`;
    const state = createState(doc);
    const ranges = state.field(stmtIndexField, false);
    expect(ranges).toBeDefined();
    expect(ranges!.ranges).toHaveLength(2);
    expect(ranges!.ranges[0]!.firstExecutableLine).toBe(1);
    expect(ranges!.ranges[1]!.firstExecutableLine).toBe(7);

    // Verify after incremental edit (e.g. typing or newline on line 2)
    const line2Pos = doc.indexOf('*');
    const updatedState = state.update({
      changes: { from: line2Pos, to: line2Pos + 1, insert: 'id, name' },
    }).state;
    const updatedRanges = updatedState.field(stmtIndexField, false);
    expect(updatedRanges!.ranges).toHaveLength(2);
    expect(updatedRanges!.ranges[0]!.firstExecutableLine).toBe(1);
    expect(updatedRanges!.ranges[1]!.firstExecutableLine).toBe(7);
  });

  it('triggers onExecute with full statement sql when gutter marker is clicked', () => {
    const onExecute = vi.fn();
    const doc = `SELECT
  *
FROM
  er_customers
WHERE
  er_customers."city" = '上海';
SELECT * FROM "test_orders";`;

    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const { EditorView } = require('@codemirror/view');
    const state = EditorState.create({
      doc,
      extensions: [
        stmtIndexField,
        executionStateField,
        frameDegradedField,
        ...createStatementGutterExtension({ onExecute }),
      ],
    });

    const view = new EditorView({ state, parent });

    try {
      // Find all rendered gutter markers
      const markers = parent.querySelectorAll('.sql-gutter-marker');
      expect(markers.length).toBe(2); // Only line 1 and line 7

      // Click the first marker (statement 1)
      markers[0]?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      expect(onExecute).toHaveBeenCalledTimes(1);
      expect(onExecute.mock.calls[0][0].source).toBe('gutter');
      expect(onExecute.mock.calls[0][0].sql).toContain('SELECT\n  *\nFROM\n  er_customers');
      expect(onExecute.mock.calls[0][0].statementIndex).toBe(0);

      // Click the second marker (statement 2)
      markers[1]?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      expect(onExecute).toHaveBeenCalledTimes(2);
      expect(onExecute.mock.calls[1][0].source).toBe('gutter');
      expect(onExecute.mock.calls[1][0].sql).toBe('SELECT * FROM "test_orders";');
      expect(onExecute.mock.calls[1][0].statementIndex).toBe(1);
    } finally {
      view.destroy();
      parent.remove();
    }
  });

  it('does NOT render gutter run marker for invalid statements like "1. 输入 SELECT ..."', () => {
    const doc = '1. 输入 SELECT order_no FROM er_orders o;';
    const parent = document.createElement('div');
    document.body.appendChild(parent);

    const { EditorView } = require('@codemirror/view');
    const state = EditorState.create({
      doc,
      extensions: [
        stmtIndexField,
        executionStateField,
        frameDegradedField,
        ...createStatementGutterExtension(),
      ],
    });

    const view = new EditorView({ state, parent });

    try {
      const markers = parent.querySelectorAll('.sql-gutter-marker');
      expect(markers.length).toBe(0);
    } finally {
      view.destroy();
      parent.remove();
    }
  });
});
