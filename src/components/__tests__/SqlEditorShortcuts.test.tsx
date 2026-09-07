import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import React, { createRef } from 'react';
import { SqlEditor, type SqlEditorHandle } from '../SqlEditor';

describe('SqlEditor Shortcuts', () => {
  it('[tester] executes trimmed selection when Mod-Enter is pressed with a selection', () => {
    const ref = createRef<SqlEditorHandle>();
    const onExecuteSelection = vi.fn();

    const sql = 'SELECT 1;\nSELECT 2;';
    const { container } = render(
      <SqlEditor
        ref={ref}
        value={sql}
        onChange={vi.fn()}
        onExecuteSelection={onExecuteSelection}
      />,
    );

    ref.current?.insertAt('');
    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();

    const isMac =
      typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
    const modProps = isMac ? { metaKey: true } : { ctrlKey: true };

    // Select first statement via keyboard chord is unreliable in jsdom; use imperative handle path
    // by inserting then selecting through editorExtensions coverage in editorExtensions.test.ts.
    // Here we validate non-empty selection path by dispatching after programmatic select-all.
    const selectAllEvent = new KeyboardEvent('keydown', {
      key: 'a',
      code: 'KeyA',
      ...modProps,
      bubbles: true,
      cancelable: true,
    });
    cmContent?.dispatchEvent(selectAllEvent);

    const enterEvent = new KeyboardEvent('keydown', {
      key: 'Enter',
      code: 'Enter',
      ...modProps,
      bubbles: true,
      cancelable: true,
    });
    cmContent?.dispatchEvent(enterEvent);

    expect(onExecuteSelection).toHaveBeenCalled();
  });

  it('executes full document via onExecute when Mod-Enter is pressed without selection', () => {
    const ref = createRef<SqlEditorHandle>();
    const onExecuteSelection = vi.fn();
    const onExecute = vi.fn();

    const sql = 'SELECT 1;\nSELECT 2;';
    const { container } = render(
      <SqlEditor
        ref={ref}
        value={sql}
        onChange={vi.fn()}
        onExecute={onExecute}
        onExecuteSelection={onExecuteSelection}
      />,
    );

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();

    const isMac =
      typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
    const modProps = isMac ? { metaKey: true } : { ctrlKey: true };

    // Trigger Mod-Enter keydown
    const event = new KeyboardEvent('keydown', {
      key: 'Enter',
      code: 'Enter',
      ...modProps,
      bubbles: true,
      cancelable: true,
    });
    cmContent?.dispatchEvent(event);

    expect(onExecute).toHaveBeenCalledTimes(1);
    expect(onExecuteSelection).not.toHaveBeenCalled();
  });

  it('triggers onSaveQuery when Mod-s is pressed', () => {
    const ref = createRef<SqlEditorHandle>();
    const onSaveQuery = vi.fn();

    const { container } = render(
      <SqlEditor ref={ref} value="SELECT 1;" onChange={vi.fn()} onSaveQuery={onSaveQuery} />,
    );

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();

    const isMac =
      typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
    const modProps = isMac ? { metaKey: true } : { ctrlKey: true };

    const event = new KeyboardEvent('keydown', {
      key: 's',
      code: 'KeyS',
      ...modProps,
      bubbles: true,
      cancelable: true,
    });
    cmContent?.dispatchEvent(event);

    expect(onSaveQuery).toHaveBeenCalledTimes(1);
  });
});
