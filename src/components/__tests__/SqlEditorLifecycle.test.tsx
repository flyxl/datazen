import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { act, render } from '@testing-library/react';
import React, { createRef } from 'react';
import { highlightingFor } from '@codemirror/language';
import { EditorView } from '@codemirror/view';
import { tags } from '@lezer/highlight';
import { SqlEditor, type SqlEditorHandle } from '../SqlEditor';
import { useSettingsStore } from '../../stores/settingsStore';

function getEditorView(container: HTMLElement): EditorView {
  const editor = container.querySelector('.cm-editor') as
    | (HTMLElement & {
        cmView?: { view: EditorView };
      })
    | null;
  const view = editor?.cmView?.view;
  if (!view) throw new Error('CodeMirror view was not mounted');
  return view;
}

describe('SqlEditor lifecycle', () => {
  let destroySpy: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    destroySpy = vi.spyOn(EditorView.prototype, 'destroy');
  });

  afterEach(() => {
    destroySpy.mockRestore();
  });

  it('destroys EditorView on unmount', () => {
    const { unmount, container } = render(<SqlEditor value="SELECT 1;" onChange={vi.fn()} />);

    expect(container.querySelector('.cm-editor')).not.toBeNull();
    unmount();
    expect(destroySpy).toHaveBeenCalledTimes(1);
    expect(container.querySelector('.cm-editor')).toBeNull();
  });

  it('registers theme-pack listener only once across prop reconfigures', () => {
    const addListenerSpy = vi.spyOn(document, 'addEventListener');
    const removeListenerSpy = vi.spyOn(document, 'removeEventListener');

    const { rerender, unmount } = render(
      <SqlEditor value="SELECT 1;" onChange={vi.fn()} schema={{ public: { users: ['id'] } }} />,
    );

    const themePackCalls = addListenerSpy.mock.calls.filter(
      ([event]) => event === 'datazen:theme-pack-changed',
    );
    expect(themePackCalls).toHaveLength(1);

    rerender(
      <SqlEditor
        value="SELECT 1;"
        onChange={vi.fn()}
        schema={{ public: { users: ['id', 'name'] } }}
        databaseType="postgresql"
        namespaceLoading
      />,
    );

    const themePackCallsAfterRerender = addListenerSpy.mock.calls.filter(
      ([event]) => event === 'datazen:theme-pack-changed',
    );
    expect(themePackCallsAfterRerender).toHaveLength(1);

    unmount();
    const themePackRemovals = removeListenerSpy.mock.calls.filter(
      ([event]) => event === 'datazen:theme-pack-changed',
    );
    expect(themePackRemovals).toHaveLength(1);

    addListenerSpy.mockRestore();
    removeListenerSpy.mockRestore();
  });

  it('reconfigures the editor syntax highlighting when the setting changes', () => {
    const previousSettings = useSettingsStore.getState().settings;
    useSettingsStore.setState({
      settings: { ...previousSettings, sqlSyntaxTheme: 'default' },
    });

    const { container, unmount } = render(<SqlEditor value="SELECT 'hello';" onChange={vi.fn()} />);

    try {
      const view = getEditorView(container);
      const initialKeywordHighlight = highlightingFor(view.state, [tags.keyword]);

      act(() => {
        useSettingsStore.setState((state) => ({
          settings: { ...state.settings, sqlSyntaxTheme: 'monokai' },
        }));
      });

      const nextKeywordHighlight = highlightingFor(view.state, [tags.keyword]);
      // The active editor should have one syntax highlighter. If the initial
      // theme is mounted outside the compartment, the old and new styles are
      // both active after the setting changes.
      expect(initialKeywordHighlight?.trim().split(/\s+/)).toHaveLength(1);
      expect(nextKeywordHighlight?.trim().split(/\s+/)).toHaveLength(1);
      expect(nextKeywordHighlight).not.toBe(initialKeywordHighlight);
    } finally {
      unmount();
      useSettingsStore.setState({ settings: previousSettings });
    }
  });

  it('uses the property-name color for identifiers after a dot', () => {
    const previousSettings = useSettingsStore.getState().settings;
    const wasDark = document.documentElement.classList.contains('dark');
    document.documentElement.classList.add('dark');
    useSettingsStore.setState({
      settings: { ...previousSettings, sqlSyntaxTheme: 'dracula' },
    });

    const { container, unmount } = render(
      <SqlEditor
        value="SELECT p.category FROM products p;"
        onChange={vi.fn()}
        databaseType="sqlite"
      />,
    );

    try {
      const property = Array.from(container.querySelectorAll('.cm-line span')).find(
        (span) => span.textContent === 'category',
      );
      const qualifier = Array.from(container.querySelectorAll('.cm-line span')).find(
        (span) => span.textContent === 'p',
      );

      expect(property?.classList.contains('cm-sql-property-name')).toBe(true);
      expect(qualifier?.classList.contains('cm-sql-property-name')).toBe(false);
      expect(property ? getComputedStyle(property).color : null).toBe('rgb(80, 250, 123)');
    } finally {
      unmount();
      useSettingsStore.setState({ settings: previousSettings });
      document.documentElement.classList.toggle('dark', wasDark);
    }
  });

  it('[tester] exposes getSelection and toggleLineComment on ref handle', () => {
    const ref = createRef<SqlEditorHandle>();
    const onChange = vi.fn();
    render(<SqlEditor ref={ref} value="SELECT 1;" onChange={onChange} />);

    expect(ref.current?.getSelection()).toBe('');
    ref.current?.toggleLineComment();
    expect(onChange).toHaveBeenCalledWith('-- SELECT 1;');
  });

  it('[tester] syncs external value prop into the editor document', () => {
    const onChange = vi.fn();
    const { rerender, container } = render(<SqlEditor value="SELECT 1;" onChange={onChange} />);

    rerender(<SqlEditor value="SELECT 2;" onChange={onChange} />);
    expect(container.querySelector('.cm-content')?.textContent).toContain('SELECT 2;');
  });

  it('[tester] renders placeholder extension when placeholder prop is set', () => {
    const { container } = render(
      <SqlEditor value="" onChange={vi.fn()} placeholder="Type SQL here..." />,
    );
    expect(container.querySelector('.cm-placeholder')).not.toBeNull();
  });

  it('[tester] calls onQualifiedPath on mount with initial qualified path', () => {
    const onQualifiedPath = vi.fn();
    render(
      <SqlEditor
        value="SELECT * FROM public.users"
        onChange={vi.fn()}
        onQualifiedPath={onQualifiedPath}
      />,
    );

    expect(onQualifiedPath).toHaveBeenCalledTimes(1);
    expect(Array.isArray(onQualifiedPath.mock.calls[0]?.[0])).toBe(true);
  });

  it('fires onChange once per document edit after schema reconfigure', () => {
    const ref = createRef<SqlEditorHandle>();
    const onChange = vi.fn();

    const { rerender, container } = render(
      <SqlEditor
        ref={ref}
        value="SELECT 1;"
        onChange={onChange}
        schema={{ public: { users: ['id'] } }}
      />,
    );

    rerender(
      <SqlEditor
        ref={ref}
        value="SELECT 1;"
        onChange={onChange}
        schema={{ public: { users: ['id', 'name'] } }}
        databaseType="mysql"
      />,
    );

    onChange.mockClear();

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();
    cmContent?.dispatchEvent(
      new InputEvent('beforeinput', {
        inputType: 'insertText',
        data: ' ',
        bubbles: true,
        cancelable: true,
      }),
    );

    ref.current?.insertAt('SELECT 2;');
    expect(onChange).toHaveBeenCalledTimes(1);
  });
});
