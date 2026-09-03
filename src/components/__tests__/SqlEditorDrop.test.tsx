import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/react';
import React, { createRef } from 'react';
import { SqlEditor, type SqlEditorHandle } from '../SqlEditor';

describe('SqlEditor Drag & Drop', () => {
  it('exposes insertAt on ref to insert text into empty document', () => {
    const ref = createRef<SqlEditorHandle>();
    const onChange = vi.fn();
    render(<SqlEditor ref={ref} value="" onChange={onChange} />);

    expect(ref.current).toBeDefined();
    ref.current?.insertAt('SELECT id FROM users;');
    expect(onChange).toHaveBeenCalledWith('SELECT id FROM users;');
  });

  it('exposes insertAt on ref to append at end when pos is null', () => {
    const ref = createRef<SqlEditorHandle>();
    const onChange = vi.fn();
    render(<SqlEditor ref={ref} value="SELECT 1;" onChange={onChange} />);

    ref.current?.insertAt('SELECT id FROM users;');
    expect(onChange).toHaveBeenCalledWith('SELECT 1;\n\nSELECT id FROM users;');
  });

  it('calls onDropTable when drop event with application/datazen-table occurs', () => {
    const ref = createRef<SqlEditorHandle>();
    const onChange = vi.fn();
    const onDropTable = vi.fn();
    const { container } = render(
      <SqlEditor ref={ref} value="" onChange={onChange} onDropTable={onDropTable} />,
    );

    const payload = {
      tables: [{ tableName: 'users', schema: 'public' }],
    };

    const dropEvent = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        types: ['application/datazen-table'],
        getData: (type: string) => (type === 'application/datazen-table' ? JSON.stringify(payload) : ''),
      },
    });

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();
    cmContent?.dispatchEvent(dropEvent);

    expect(onDropTable).toHaveBeenCalledTimes(1);
    expect(onDropTable).toHaveBeenCalledWith(payload, expect.anything());
  });
});
