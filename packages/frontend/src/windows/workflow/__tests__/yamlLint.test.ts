import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { paramRefDiagnostics } from '../yamlLint';

function diag(text: string) {
  const state = EditorState.create({ doc: text });
  return paramRefDiagnostics({ state } as never);
}

describe('yamlParamLint', () => {
  it('flags an unclosed parameter reference', () => {
    const d = diag('sql: SELECT ${id} FROM t WHERE x=${name');
    expect(d).toHaveLength(1);
    expect(d[0].severity).toBe('error');
    expect(textSlice(d[0].from, d[0].to, 'sql: SELECT ${id} FROM t WHERE x=${name')).toBe('${');
  });

  it('accepts balanced references', () => {
    expect(diag('sql: SELECT ${id} FROM t WHERE x=${name} AND y=${other}')).toHaveLength(0);
  });

  it('reports each unclosed opener', () => {
    expect(diag('${a} ${b ${c}')).toHaveLength(1);
    expect(diag('${a ${b')).toHaveLength(2);
  });

  it('ignores a stray closing brace (may be legitimate SQL)', () => {
    expect(diag("sql: SELECT '}$not_a_param'")).toHaveLength(0);
  });

  it('ignores parameter tokens in comment lines', () => {
    expect(diag('# ${hint\nsql: SELECT 1')).toHaveLength(0);
  });
});

function textSlice(from: number, to: number, doc: string) {
  return doc.slice(from, to);
}
