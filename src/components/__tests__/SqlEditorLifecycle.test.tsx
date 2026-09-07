import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render } from '@testing-library/react';
import React, { createRef } from 'react';
import { EditorView } from '@codemirror/view';
import { SqlEditor, type SqlEditorHandle } from '../SqlEditor';

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
