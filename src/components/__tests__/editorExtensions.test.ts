import { describe, it, expect, vi, afterEach } from 'vitest';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import {
  createBaseEditorExtensions,
  createDomEventHandlers,
  createUpdateListener,
} from '../sql-editor/editorExtensions';

function mountView(
  extensions:
    | ReturnType<typeof createBaseEditorExtensions>[number][]
    | Parameters<typeof EditorState.create>[0]['extensions'],
  opts?: { doc?: string },
) {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  const view = new EditorView({
    state: EditorState.create({
      doc: opts?.doc ?? 'SELECT 1;\nSELECT 2;',
      extensions: extensions ?? [],
    }),
    parent,
  });
  return {
    view,
    parent,
    cleanup: () => {
      view.destroy();
      parent.remove();
    },
  };
}

describe('[tester] editorExtensions factories', () => {
  const cleanups: Array<() => void> = [];

  afterEach(() => {
    cleanups.forEach((fn) => fn());
    cleanups.length = 0;
  });

  it('createBaseEditorExtensions runs Mod-Enter on non-empty selection', () => {
    const onExecuteSelection = vi.fn();
    const onExecute = vi.fn();
    const { view, cleanup } = mountView(
      createBaseEditorExtensions({
        onExecute: { current: onExecute },
        onExecuteSelection: { current: onExecuteSelection },
        onExecuteAll: { current: undefined },
        onSaveQuery: { current: undefined },
      }),
    );
    cleanups.push(cleanup);

    view.dispatch({ selection: { anchor: 0, head: 9 } });
    const isMac = /Mac/i.test(navigator.platform || navigator.userAgent);
    view.contentDOM.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        ...(isMac ? { metaKey: true } : { ctrlKey: true }),
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(onExecuteSelection).toHaveBeenCalledWith('SELECT 1;');
    expect(onExecute).not.toHaveBeenCalled();
  });

  it('createBaseEditorExtensions falls back to onExecute when no statement at cursor', () => {
    const onExecuteSelection = vi.fn();
    const onExecute = vi.fn();
    const parent = document.createElement('div');
    document.body.appendChild(parent);
    const view = new EditorView({
      state: EditorState.create({
        doc: '',
        extensions: createBaseEditorExtensions({
          onExecute: { current: onExecute },
          onExecuteSelection: { current: onExecuteSelection },
          onExecuteAll: { current: undefined },
          onSaveQuery: { current: undefined },
        }),
      }),
      parent,
    });
    cleanups.push(() => {
      view.destroy();
      parent.remove();
    });

    const isMac = /Mac/i.test(navigator.platform || navigator.userAgent);
    view.contentDOM.dispatchEvent(
      new KeyboardEvent('keydown', {
        key: 'Enter',
        ...(isMac ? { metaKey: true } : { ctrlKey: true }),
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(onExecuteSelection).not.toHaveBeenCalled();
    expect(onExecute).toHaveBeenCalledTimes(1);
  });

  it('createDomEventHandlers invokes contextmenu handler with trimmed document sql', () => {
    const onCtxMenu = vi.fn();
    const { view, cleanup } = mountView([
      createDomEventHandlers({
        onCtxMenu: { current: onCtxMenu },
      }),
    ]);
    cleanups.push(cleanup);

    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    view.contentDOM.dispatchEvent(event);

    expect(onCtxMenu).toHaveBeenCalledTimes(1);
    // §S6-D: getStatementAtCursor returns only the current statement
    expect(onCtxMenu.mock.calls[0]?.[1]).toBe('SELECT 1;');
  });

  it('createDomEventHandlers contextmenu falls back to full doc when no statement at cursor', () => {
    const onCtxMenu = vi.fn();
    // Create a view with a doc that has no semicolons
    const { view, cleanup } = mountView(
      [
        createDomEventHandlers({
          onCtxMenu: { current: onCtxMenu },
        }),
      ],
      { doc: 'hello world' },
    );
    cleanups.push(cleanup);

    const event = new MouseEvent('contextmenu', { bubbles: true, cancelable: true });
    view.contentDOM.dispatchEvent(event);

    expect(onCtxMenu).toHaveBeenCalledTimes(1);
    expect(onCtxMenu.mock.calls[0]?.[1]).toBe('hello world');
  });

  it('createDomEventHandlers does not handle drop events (handled by createPasteExtensions)', () => {
    const onDropTable = vi.fn();
    const { view, cleanup } = mountView([
      createDomEventHandlers({
        onCtxMenu: { current: undefined },
      }),
    ]);
    cleanups.push(cleanup);

    const dropEvent = new Event('drop', { bubbles: true, cancelable: true }) as Event & {
      dataTransfer: DataTransfer;
    };
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        getData: (type: string) => (type === 'application/datazen-table' ? '{not-json' : ''),
      },
    });
    view.contentDOM.dispatchEvent(dropEvent);

    // §S6-D: Drop handling moved to createPasteExtensions; createDomEventHandlers ignores drops
    expect(onDropTable).not.toHaveBeenCalled();
  });

  it('createUpdateListener notifies onQualifiedPath when parents change', () => {
    const onChange = vi.fn();
    const onQualifiedPath = vi.fn();
    const lastParents = { current: [] as string[] };
    const { view, cleanup } = mountView([
      createUpdateListener({
        onChange: { current: onChange },
        onQualifiedPath: { current: onQualifiedPath },
        lastParents,
      }),
    ]);
    cleanups.push(cleanup);

    view.dispatch({
      changes: { from: 0, to: 0, insert: 'public.users' },
      selection: { anchor: 'public.users'.length },
    });

    expect(onChange).toHaveBeenCalled();
    expect(onQualifiedPath).toHaveBeenCalled();
    expect(lastParents.current.length).toBeGreaterThan(0);
  });
});
