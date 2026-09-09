import { describe, it, expect, vi, beforeAll } from 'vitest';
import { render } from '@testing-library/react';
import React, { createRef } from 'react';
import { SqlEditor, type SqlEditorHandle } from '../SqlEditor';
import { extensionRegistry, sqlEditorEnhancedEP } from '@datazen/extension-points';
import { EditorView } from '@codemirror/view';

describe('SqlEditor Drag & Drop', () => {
  beforeAll(() => {
    function resolvePayload(rawObj: any) {
      if (rawObj.version === 1 && rawObj.namespace) {
        return {
          tables: [{ tableName: rawObj.namespace.table, schema: rawObj.namespace.schema }],
          connectionId: rawObj.connectionId,
          databaseType: rawObj.databaseType,
        };
      }
      if (rawObj.tables) return rawObj;
      if (rawObj.tableName) {
        return { tables: [{ tableName: rawObj.tableName, schema: rawObj.schema }] };
      }
      return rawObj;
    }

    extensionRegistry.register(sqlEditorEnhancedEP, {
      createPasteExtensions: (opts) => {
        return [
          EditorView.domEventHandlers({
            dragover(event) {
              const types = event.dataTransfer?.types || [];
              if (
                types.includes('application/datazen-table') ||
                types.includes('application/datazen-schema-object') ||
                types.includes('text/plain')
              ) {
                event.preventDefault();
              }
            },
            drop(event) {
              const dt = event.dataTransfer;
              if (!dt) return;
              const jsonRaw =
                dt.getData('application/datazen-table') ||
                dt.getData('application/datazen-schema-object');
              if (jsonRaw) {
                try {
                  const parsed = JSON.parse(jsonRaw);
                  opts.onDrop?.(resolvePayload(parsed), 0);
                  return;
                } catch {
                  // ignore
                }
              }
              const textRaw = dt.getData('text/plain');
              if (textRaw) {
                try {
                  const parsed = JSON.parse(textRaw);
                  opts.onDrop?.(resolvePayload(parsed), 0);
                } catch {
                  opts.onDrop?.({ tables: [{ tableName: textRaw, schema: undefined }] }, 0);
                }
              }
            },
          }),
        ];
      },
    });
  });
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

  it('[tester] calls onContextMenu when contextmenu event fires', () => {
    const onContextMenu = vi.fn();
    const { container } = render(
      <SqlEditor value="SELECT 1;" onChange={vi.fn()} onContextMenu={onContextMenu} />,
    );

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();
    cmContent?.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));

    expect(onContextMenu).toHaveBeenCalledTimes(1);
    expect(onContextMenu.mock.calls[0]?.[1]).toBe('SELECT 1;');
  });

  it('[tester] accepts dragover for schema tree table payloads', () => {
    const { container } = render(<SqlEditor value="" onChange={vi.fn()} onDropTable={vi.fn()} />);
    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();

    const dragEvent = new Event('dragover', { bubbles: true, cancelable: true }) as Event & {
      preventDefault: () => void;
      dataTransfer: DataTransfer;
    };
    const preventDefault = vi.fn();
    dragEvent.preventDefault = preventDefault;
    Object.defineProperty(dragEvent, 'dataTransfer', {
      value: { types: ['application/datazen-table'], dropEffect: '' },
    });
    cmContent?.dispatchEvent(dragEvent);
    expect(preventDefault).toHaveBeenCalled();
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
        getData: (type: string) =>
          type === 'application/datazen-table' ? JSON.stringify(payload) : '',
      },
    });

    const cmContent = container.querySelector('.cm-content');
    expect(cmContent).not.toBeNull();
    cmContent?.dispatchEvent(dropEvent);

    expect(onDropTable).toHaveBeenCalledTimes(1);
    // §S5-A: createPasteExtensions enriches the payload with connectionId/databaseType
    expect(onDropTable).toHaveBeenCalledWith(
      expect.objectContaining({ tables: payload.tables }),
      expect.anything(),
    );
  });

  it('[tester] wraps legacy single-table drop payloads', () => {
    const ref = createRef<SqlEditorHandle>();
    const onDropTable = vi.fn();
    const { container } = render(
      <SqlEditor ref={ref} value="" onChange={vi.fn()} onDropTable={onDropTable} />,
    );

    // §S5-A: Legacy single-table payloads must use { tables: [...] } format
    // for createDropCaretExtension to parse them. Bare { tableName, schema }
    // is no longer auto-wrapped — the schema tree now sends the v1 MIME type.
    const legacyPayload = {
      tables: [{ tableName: 'orders', schema: 'public' }],
    };
    const dropEvent = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        types: ['application/datazen-table'],
        getData: (type: string) =>
          type === 'application/datazen-table' ? JSON.stringify(legacyPayload) : '',
      },
    });

    container.querySelector('.cm-content')?.dispatchEvent(dropEvent);
    expect(onDropTable).toHaveBeenCalledWith(
      expect.objectContaining({ tables: legacyPayload.tables }),
      expect.anything(),
    );
  });

  it('handles versioned application/datazen-schema-object payload', () => {
    const ref = createRef<SqlEditorHandle>();
    const onDropTable = vi.fn();
    const { container } = render(
      <SqlEditor ref={ref} value="" onChange={vi.fn()} onDropTable={onDropTable} />,
    );

    const v1Payload = {
      version: 1,
      kind: 'table',
      namespace: {
        database: 'testdb',
        schema: 'public',
        table: 'products',
      },
      connectionId: 'conn-1',
      databaseType: 'postgresql',
    };

    const dropEvent = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        types: ['application/datazen-schema-object'],
        getData: (type: string) =>
          type === 'application/datazen-schema-object' ? JSON.stringify(v1Payload) : '',
      },
    });

    container.querySelector('.cm-content')?.dispatchEvent(dropEvent);
    expect(onDropTable).toHaveBeenCalledTimes(1);
    expect(onDropTable).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [{ tableName: 'products', schema: 'public' }],
        connectionId: 'conn-1',
        databaseType: 'postgresql',
      }),
      expect.anything(),
    );
  });

  it('handles lowercase MIME and text/plain fallback', () => {
    const ref = createRef<SqlEditorHandle>();
    const onDropTable = vi.fn();
    const { container } = render(
      <SqlEditor ref={ref} value="" onChange={vi.fn()} onDropTable={onDropTable} />,
    );

    const payload = {
      version: 1,
      kind: 'table',
      namespace: {
        database: 'db',
        table: 'customers',
      },
      connectionId: 'c1',
      databaseType: 'mysql',
    };

    const dropEvent = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        types: ['text/plain'],
        getData: (type: string) => (type === 'text/plain' ? JSON.stringify(payload) : ''),
      },
    });

    container.querySelector('.cm-content')?.dispatchEvent(dropEvent);
    expect(onDropTable).toHaveBeenCalledTimes(1);
    expect(onDropTable).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [{ tableName: 'customers', schema: undefined }],
      }),
      expect.anything(),
    );
  });

  it('handles raw table name in text/plain fallback', () => {
    const ref = createRef<SqlEditorHandle>();
    const onDropTable = vi.fn();
    const { container } = render(
      <SqlEditor ref={ref} value="" onChange={vi.fn()} onDropTable={onDropTable} />,
    );

    const dropEvent = new Event('drop', { bubbles: true, cancelable: true });
    Object.defineProperty(dropEvent, 'dataTransfer', {
      value: {
        types: ['text/plain'],
        getData: (type: string) => (type === 'text/plain' ? 'er_customers' : ''),
      },
    });

    container.querySelector('.cm-content')?.dispatchEvent(dropEvent);
    expect(onDropTable).toHaveBeenCalledTimes(1);
    expect(onDropTable).toHaveBeenCalledWith(
      expect.objectContaining({
        tables: [{ tableName: 'er_customers', schema: undefined }],
      }),
      expect.anything(),
    );
  });
});
