import { describe, expect, it } from 'vitest';
import { EditorState } from '@codemirror/state';
import { sql, StandardSQL } from '@codemirror/lang-sql';
import { CompletionContext } from '@codemirror/autocomplete';
import {
  namespaceLoadingCompletionResult,
  namespaceLoadingCompletionSource,
  shouldShowNamespaceLoadingHint,
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

describe('shouldShowNamespaceLoadingHint', () => {
  it('hides the loading hint until the user types or explicitly completes', () => {
    expect(shouldShowNamespaceLoadingHint(false, false, false)).toBe(false);
    expect(shouldShowNamespaceLoadingHint(true, false, false)).toBe(false);
    expect(shouldShowNamespaceLoadingHint(true, true, false)).toBe(true);
    expect(shouldShowNamespaceLoadingHint(true, false, true)).toBe(true);
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

  it('does not offer a loading hint on an empty editor', () => {
    const source = namespaceLoadingCompletionSource(true, 'Loading objects…');
    const state = EditorState.create({
      doc: '',
      extensions: [sql({ dialect: StandardSQL })],
    });
    expect(source(new CompletionContext(state, 0, false))).toBeNull();
  });

  it('offers a loading hint after the user types a prefix', () => {
    const source = namespaceLoadingCompletionSource(true, 'Loading objects…');
    const state = EditorState.create({
      doc: 'sel',
      extensions: [sql({ dialect: StandardSQL })],
    });
    const result = source(new CompletionContext(state, 3, false));
    expect(result?.options[0]?.label).toBe('Loading objects…');
  });
});
