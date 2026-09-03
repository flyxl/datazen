import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import React, { createRef } from 'react';
import { SqlEditor, type SqlEditorHandle } from '../SqlEditor';

describe('SqlEditor Shortcuts', () => {
  it('executes statement at cursor when Mod-Enter is pressed without selection', () => {
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

    const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
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

    // It should execute statement at cursor rather than full document
    expect(onExecuteSelection).toHaveBeenCalledTimes(1);
    expect(onExecuteSelection).toHaveBeenCalledWith('SELECT 1;');
  });

  it('triggers onExecuteAll when Mod-Shift-Enter is pressed', () => {
    const ref = createRef<SqlEditorHandle>();
    const onExecuteAll = vi.fn();

    const sql = 'SELECT 1;\nSELECT 2;';
    const { container } = render(
      <SqlEditor
        ref={ref}
        value={sql}
        onChange={vi.fn()}
        onExecuteAll={onExecuteAll}
      />,
    );

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();

    const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
    const modProps = isMac ? { metaKey: true } : { ctrlKey: true };

    const event = new KeyboardEvent('keydown', {
      key: 'Enter',
      code: 'Enter',
      ...modProps,
      shiftKey: true,
      bubbles: true,
      cancelable: true,
    });
    cmContent?.dispatchEvent(event);

    expect(onExecuteAll).toHaveBeenCalledTimes(1);
  });

  it('triggers onSaveQuery when Mod-s is pressed', () => {
    const ref = createRef<SqlEditorHandle>();
    const onSaveQuery = vi.fn();

    const { container } = render(
      <SqlEditor
        ref={ref}
        value="SELECT 1;"
        onChange={vi.fn()}
        onSaveQuery={onSaveQuery}
      />,
    );

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();

    const isMac = typeof navigator !== 'undefined' && /Mac/i.test(navigator.platform || navigator.userAgent);
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
