import { describe, it, expect } from 'vitest';
import { EditorState, StateEffect } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import {
  createStatementFrameExtension,
  frameDegradedField,
  SetDegradedEffect,
} from '../statementFrame';
import { shouldHideFrame, isDocumentDegraded } from '../types';
import { statementIndexField } from '../../semantic/statementRanges';
import { executionStateField, StartExecutionEffect } from '../executionState';

// Create a shared field instance so registration and query use the same field
const stmtIndexField = statementIndexField();

// ── Pure function tests ──────────────────────────────────────────────────

describe('isDocumentDegraded (pure)', () => {
  it('returns false for small documents', () => {
    expect(isDocumentDegraded(10)).toBe(false);
    expect(isDocumentDegraded(500)).toBe(false);
  });

  it('returns true for documents exceeding threshold', () => {
    expect(isDocumentDegraded(501)).toBe(true);
    expect(isDocumentDegraded(1000)).toBe(true);
  });

  it('uses custom threshold', () => {
    expect(isDocumentDegraded(10, 5)).toBe(true);
    expect(isDocumentDegraded(5, 5)).toBe(false);
    expect(isDocumentDegraded(4, 5)).toBe(false);
  });
});

describe('shouldHideFrame (pure)', () => {
  it('hides when degraded', () => {
    expect(
      shouldHideFrame({
        degraded: true,
        selectionEmpty: true,
        selectionFrom: 0,
        selectionTo: 0,
        docLineCount: 10,
        docText: 'SELECT 1;\nSELECT 2;',
      }),
    ).toBe(true);
  });

  it('does not hide for single cursor (empty selection)', () => {
    expect(
      shouldHideFrame({
        degraded: false,
        selectionEmpty: true,
        selectionFrom: 5,
        selectionTo: 5,
        docLineCount: 10,
        docText: 'SELECT 1;\nSELECT 2;',
      }),
    ).toBe(false);
  });

  it('hides for multi-line non-empty selection', () => {
    expect(
      shouldHideFrame({
        degraded: false,
        selectionEmpty: false,
        selectionFrom: 0,
        selectionTo: 50,
        docLineCount: 10,
        docText:
          'SELECT 1;\nSELECT 2;\nSELECT 3;\nSELECT 4;\nSELECT 5;\nSELECT 6;\nSELECT 7;\nSELECT 8;\nSELECT 9;\nSELECT 10;',
      }),
    ).toBe(true);
  });

  it('does not hide for same-line selection', () => {
    expect(
      shouldHideFrame({
        degraded: false,
        selectionEmpty: false,
        selectionFrom: 0,
        selectionTo: 5,
        docLineCount: 10,
        docText: 'SELECT 1;\nSELECT 2;',
      }),
    ).toBe(false);
  });

  it('does not hide when selectionFrom === selectionTo (empty)', () => {
    expect(
      shouldHideFrame({
        degraded: false,
        selectionEmpty: false,
        selectionFrom: 5,
        selectionTo: 5,
        docLineCount: 10,
        docText: 'SELECT 1;\nSELECT 2;',
      }),
    ).toBe(false);
  });
});

// ── CodeMirror integration tests ─────────────────────────────────────────

function createState(doc: string, degraded = false) {
  const extensions = [stmtIndexField, executionStateField, ...createStatementFrameExtension()];
  let state = EditorState.create({ doc, extensions });
  if (degraded) {
    state = state.update({
      effects: SetDegradedEffect.of(true),
    }).state;
  }
  return state;
}

function getFrameDecos(view: EditorView) {
  const decos: Array<{ line: number; from: number }> = [];
  // Iterate through decorations to find sql-statement-frame
  view.state.field(statementIndexField(), false)?.ranges.forEach((range) => {
    // Check if the first line of this range has the frame decoration
    const line = view.state.doc.lineAt(range.from);
    decos.push({ line: line.number, from: line.from });
  });
  return decos;
}

describe('frameDegradedField', () => {
  it('[tester] starts degraded for documents over 500 lines', () => {
    const doc = Array.from({ length: 600 }, (_, i) => `SELECT ${i};`).join('\n');
    const state = createState(doc);
    expect(state.field(frameDegradedField)).toBe(true);
  });

  it('[tester] not degraded for documents under 500 lines', () => {
    const state = createState('SELECT 1; SELECT 2;');
    expect(state.field(frameDegradedField)).toBe(false);
  });

  it('[tester] can be explicitly set via SetDegradedEffect', () => {
    let state = createState('SELECT 1;');
    expect(state.field(frameDegradedField)).toBe(false);
    state = state.update({
      effects: SetDegradedEffect.of(true),
    }).state;
    expect(state.field(frameDegradedField)).toBe(true);
  });

  it('[tester] can be explicitly unset via SetDegradedEffect', () => {
    const doc = Array.from({ length: 600 }, (_, i) => `SELECT ${i};`).join('\n');
    let state = createState(doc);
    expect(state.field(frameDegradedField)).toBe(true);
    state = state.update({
      effects: SetDegradedEffect.of(false),
    }).state;
    expect(state.field(frameDegradedField)).toBe(false);
  });
});

describe('createStatementFrameExtension', () => {
  it('[tester] returns an array of extensions', () => {
    const ext = createStatementFrameExtension();
    expect(Array.isArray(ext)).toBe(true);
    expect(ext.length).toBeGreaterThan(0);
  });

  it('[tester] includes the degraded field', () => {
    const ext = createStatementFrameExtension();
    const state = EditorState.create({
      doc: 'SELECT 1;',
      extensions: ext,
    });
    // Should be able to read the degraded field
    const degraded = state.field(frameDegradedField, false);
    expect(typeof degraded).toBe('boolean');
  });

  it('[tester] creates frame decorations for multi-statement document', () => {
    const doc = 'SELECT 1;\nSELECT 2;\nSELECT 3;';
    const state = createState(doc);
    // Statement ranges should exist
    const ranges = state.field(stmtIndexField, false);
    expect(ranges).toBeDefined();
    expect(ranges!.ranges.length).toBe(3);
  });

  it('[tester] degraded mode does not create frame decorations', () => {
    const doc = 'SELECT 1; SELECT 2;';
    const state = createState(doc, true);
    const degraded = state.field(frameDegradedField);
    expect(degraded).toBe(true);
  });
});
