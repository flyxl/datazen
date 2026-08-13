import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { sql, StandardSQL } from '@codemirror/lang-sql';
import {
  namespaceLoadingCompletionResult,
  namespaceLoadingCompletionSource,
} from '../namespaceLoadingCompletion';

describe('namespaceLoadingCompletionResult', () => {
  it('returns null when not loading', () => {
    expect(namespaceLoadingCompletionResult(false, 0, 'Loading objects…')).toBeNull();
  });

  it('returns a non-inserting option while loading', () => {
    const result = namespaceLoadingCompletionResult(true, 12, 'Loading objects…');
    expect(result).not.toBeNull();
    expect(result!.from).toBe(12);
    expect(result!.filter).toBe(false);
    expect(result!.options).toHaveLength(1);
    expect(result!.options[0]!.label).toBe('Loading objects…');
    expect(typeof result!.options[0]!.apply).toBe('function');
  });
});

describe('namespaceLoadingCompletionSource', () => {
  it('is not a CodeMirror Extension and must be registered via language data', () => {
    const source = namespaceLoadingCompletionSource(true, 'Loading objects…');
    expect(() => EditorState.create({ extensions: [source] })).toThrow(
      /Unrecognized extension value/,
    );
    expect(() =>
      EditorState.create({
        extensions: [
          sql({ dialect: StandardSQL }),
          StandardSQL.language.data.of({ autocomplete: source }),
        ],
      }),
    ).not.toThrow();
  });
});

describe('namespaceLoadingCompletionResult', () => {
  it('returns null when not loading', () => {
    expect(namespaceLoadingCompletionResult(false, 0, 'Loading objects…')).toBeNull();
  });

  it('returns a non-inserting option while loading', () => {
    const result = namespaceLoadingCompletionResult(true, 12, 'Loading objects…');
    expect(result).not.toBeNull();
    expect(result!.from).toBe(12);
    expect(result!.filter).toBe(false);
    expect(result!.options).toHaveLength(1);
    expect(result!.options[0]!.label).toBe('Loading objects…');
    expect(typeof result!.options[0]!.apply).toBe('function');
  });
});
