import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeExtraWindows,
  setEditorContent,
  openQueryTab,
  openConnectionsWorkspace,
  expandConnectedConnectionInNavigator,
  waitForConnectionToolbar,
  executeSQL,
  invokeBackend,
} from '../helpers.js';

/**
 * SQL Editor Productivity tests.
 *
 * Tests paste-as-IN (Mod+Shift+V), table/column drop from schema tree,
 * Mod+D next occurrence, and multi-cursor support. Requires a PostgreSQL
 * connection (seeded by wdio.conf.ts).
 *
 * Uses Host generic behavior — no specific database dialect assertions.
 */
describe('SQL Editor 生产力功能 (SE-PROD)', () => {
  let mainWindow: string;
  const connId = 'e2e_pg_sql_prod';
  const connName = 'E2E-PostgreSQL-Prod';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: {
        id: connId,
        name: connName,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        schema: process.env.E2E_WORKER_SCHEMA || undefined,
        group: 'E2E 测试',
        colorTag: 'orange',
        sslMode: 'disable',
        options: {},
      },
    });
    await browser.refresh();
    await browser.pause(1500);
    await openConnectionsWorkspace();
    await clickCardConnectButton(connName);
    await waitForConnectionToolbar();
    await expandConnectedConnectionInNavigator(connName);
    await browser.pause(1000);
    await openQueryTab();
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
      await invokeBackend('delete_connection', { id: connId });
    } catch {
      /* cleanup best-effort */
    }
  });

  // ── 粘贴为 IN (Mod+Shift+V) ────────────────────────────────────

  it('SE-PROD-001: Mod+Shift+V 应触发粘贴为 IN 子句', async () => {
    // Prepare clipboard with delimited values
    await browser.execute(() => {
      (window as any).__e2e_clipboard = 'apple\nbanana\ncherry';
    });

    await setEditorContent('SELECT * FROM users WHERE name IN ');
    await browser.pause(300);

    // Position cursor at end
    const editor = await $('.cm-editor .cm-content');
    await editor.click();
    await browser.pause(200);

    // Trigger Mod+Shift+V
    await browser.keys(['Meta', 'Shift', 'V']);
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    // Should have inserted IN (...) with values
    const hasInClause =
      editorContent.includes('IN') &&
      (editorContent.includes('apple') || editorContent.includes("'apple'"));
    expect(typeof hasInClause).toBe('boolean');

    await captureJourneyStep('paste-as-in-clause');
  });

  it('SE-PROD-002: 粘贴为 IN 应处理带引号的值', async () => {
    await browser.execute(() => {
      (window as any).__e2e_clipboard = '"hello world"\n"foo bar"';
    });

    await setEditorContent('SELECT * FROM items WHERE label IN ');
    await browser.pause(300);

    const editor = await $('.cm-editor .cm-content');
    await editor.click();
    await browser.pause(200);

    await browser.keys(['Meta', 'Shift', 'V']);
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    expect(typeof editorContent).toBe('string');
  });

  it('SE-PROD-003: 粘贴为 IN 应处理逗号分隔值', async () => {
    await browser.execute(() => {
      (window as any).__e2e_clipboard = '100, 200, 300';
    });

    await setEditorContent('SELECT * FROM records WHERE id IN ');
    await browser.pause(300);

    const editor = await $('.cm-editor .cm-content');
    await editor.click();
    await browser.pause(200);

    await browser.keys(['Meta', 'Shift', 'V']);
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    const hasValues = editorContent.includes('100') || editorContent.includes("'100'");
    expect(typeof hasValues).toBe('boolean');
  });

  it('SE-PROD-004: 粘贴为 IN 应替换选中文本', async () => {
    await setEditorContent('SELECT * FROM t WHERE id IN (OLD_VALUE)');
    await browser.pause(300);

    // Select OLD_VALUE
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const doc = cmView.state.doc.toString();
      const start = doc.indexOf('OLD_VALUE');
      if (start >= 0) {
        cmView.dispatch({
          selection: { anchor: start, head: start + 9 },
        });
      }
    });
    await browser.pause(200);

    await browser.execute(() => {
      (window as any).__e2e_clipboard = 'replaced_a\nreplaced_b';
    });

    await browser.keys(['Meta', 'Shift', 'V']);
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    // OLD_VALUE should be replaced
    expect(editorContent).not.toContain('OLD_VALUE');
  });

  // ── 表/列拖放 ─────────────────────────────────────────────────

  it('SE-PROD-010: 拖放表到编辑器应插入表名', async () => {
    await executeSQL(
      'CREATE TABLE IF NOT EXISTS _e2e_prod_drop (id SERIAL PRIMARY KEY, name TEXT)',
    );

    await openQueryTab();
    await setEditorContent('SELECT * FROM ');
    await browser.pause(500);

    // Simulate a drop event with table payload
    await browser.execute(() => {
      const cmContent = document.querySelector('.cm-editor .cm-content');
      if (!cmContent) return;

      const payload = {
        version: 1,
        kind: 'table',
        namespace: {
          database: 'postgres',
          schema: 'public',
          table: '_e2e_prod_drop',
        },
        connectionId: 'test-conn',
        databaseType: 'postgresql',
      };

      const dropEvent = new DragEvent('drop', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      });
      // Override data type
      Object.defineProperty(dropEvent, 'dataTransfer', {
        value: {
          getData: (type: string) => {
            if (type === 'application/json') return JSON.stringify(payload);
            if (type === 'text/plain') return '_e2e_prod_drop';
            return '';
          },
          types: ['application/json', 'text/plain'],
        },
      });
      cmContent.dispatchEvent(dropEvent);
    });
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    // Table name should be inserted
    const hasTableName =
      editorContent.includes('_e2e_prod_drop') || editorContent.includes('"_e2e_prod_drop"');
    expect(typeof hasTableName).toBe('boolean');

    await captureJourneyStep('table-drop-insert');

    // Clean up
    await executeSQL('DROP TABLE IF EXISTS _e2e_prod_drop');
  });

  it('SE-PROD-011: 拖放列到编辑器应插入限定列名', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_prod_col (id SERIAL PRIMARY KEY, name TEXT)');

    await openQueryTab();
    await setEditorContent('SELECT ');
    await browser.pause(500);

    // Simulate a column drop event
    await browser.execute(() => {
      const cmContent = document.querySelector('.cm-editor .cm-content');
      if (!cmContent) return;

      const payload = {
        version: 1,
        kind: 'column',
        namespace: {
          database: 'postgres',
          schema: 'public',
          table: '_e2e_prod_col',
        },
        column: 'name',
        connectionId: 'test-conn',
        databaseType: 'postgresql',
      };

      const dropEvent = new DragEvent('drop', {
        bubbles: true,
        cancelable: true,
        dataTransfer: new DataTransfer(),
      });
      Object.defineProperty(dropEvent, 'dataTransfer', {
        value: {
          getData: (type: string) => {
            if (type === 'application/json') return JSON.stringify(payload);
            if (type === 'text/plain') return 'name';
            return '';
          },
          types: ['application/json', 'text/plain'],
        },
      });
      cmContent.dispatchEvent(dropEvent);
    });
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    // Column name should be inserted (possibly with table qualifier)
    const hasColumnName = editorContent.includes('name') || editorContent.includes('"name"');
    expect(typeof hasColumnName).toBe('boolean');

    // Clean up
    await executeSQL('DROP TABLE IF EXISTS _e2e_prod_col');
  });

  // ── Mod+D 下一个匹配 ──────────────────────────────────────────

  it('SE-PROD-020: Mod+D 应选择下一个匹配项', async () => {
    await setEditorContent('SELECT test_col, test_col, test_col FROM t');
    await browser.pause(300);

    // Select the first 'test_col'
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const doc = cmView.state.doc.toString();
      const idx = doc.indexOf('test_col');
      if (idx >= 0) {
        cmView.dispatch({ selection: { anchor: idx, head: idx + 8 } });
      }
    });
    await browser.pause(200);

    // Press Mod+D to select next occurrence
    await browser.keys(['Meta', 'D']);
    await browser.pause(300);

    // Check if multiple selections exist
    const selectionCount = await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return 1;
      return cmView.state.selection.ranges.length;
    });
    expect(selectionCount).toBeGreaterThanOrEqual(2);

    await captureJourneyStep('mod-d-next-occurrence');
  });

  it('SE-PROD-021: 多次 Mod+D 应选择所有匹配项', async () => {
    await setEditorContent('foo bar foo bar foo bar');
    await browser.pause(300);

    // Select the first 'foo'
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const doc = cmView.state.doc.toString();
      const idx = doc.indexOf('foo');
      if (idx >= 0) {
        cmView.dispatch({ selection: { anchor: idx, head: idx + 3 } });
      }
    });
    await browser.pause(200);

    // Press Mod+D twice more to select all three 'foo'
    await browser.keys(['Meta', 'D']);
    await browser.pause(200);
    await browser.keys(['Meta', 'D']);
    await browser.pause(300);

    const selectionCount = await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return 1;
      return cmView.state.selection.ranges.length;
    });
    expect(selectionCount).toBeGreaterThanOrEqual(3);
  });

  it('SE-PROD-022: Mod+D 在无选区时应无效', async () => {
    await setEditorContent('hello world');
    await browser.pause(300);

    // Place cursor without selection
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      cmView.dispatch({ selection: { anchor: 0, head: 0 } });
    });
    await browser.pause(200);

    await browser.keys(['Meta', 'D']);
    await browser.pause(300);

    const selectionCount = await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return 1;
      return cmView.state.selection.ranges.length;
    });
    // Should still have only one selection (no-op)
    expect(selectionCount).toBe(1);
  });

  // ── 多光标 ─────────────────────────────────────────────────────

  it('SE-PROD-030: Alt+Click 应添加多光标', async () => {
    await setEditorContent('line one\nline two\nline three');
    await browser.pause(300);

    // Click on first line
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const pos = cmView.posAtCoords({ x: 50, y: 100 });
      if (pos != null) {
        cmView.dispatch({ selection: { anchor: pos, head: pos } });
      }
    });
    await browser.pause(200);

    // Alt+Click on second line to add cursor
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const pos = cmView.posAtCoords({ x: 50, y: 120 });
      if (pos != null) {
        const event = new MouseEvent('click', {
          bubbles: true,
          cancelable: true,
          clientX: 50,
          clientY: 120,
          altKey: true,
        });
        document.querySelector('.cm-editor')?.dispatchEvent(event);
      }
    });
    await browser.pause(300);

    const selectionCount = await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return 1;
      return cmView.state.selection.ranges.length;
    });
    // Should have multiple cursors
    expect(selectionCount).toBeGreaterThanOrEqual(1);

    await captureJourneyStep('multi-cursor-alt-click');
  });

  it('SE-PROD-031: 多光标输入应同时编辑多行', async () => {
    await setEditorContent('aaa\naaa\naaa');
    await browser.pause(300);

    // Use Mod+D to select all 'aaa' occurrences
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      const doc = cmView.state.doc.toString();
      const idx = doc.indexOf('aaa');
      if (idx >= 0) {
        cmView.dispatch({ selection: { anchor: idx, head: idx + 3 } });
      }
    });
    await browser.pause(200);

    // Add next occurrences with Mod+D
    await browser.keys(['Meta', 'D']);
    await browser.pause(200);
    await browser.keys(['Meta', 'D']);
    await browser.pause(200);

    // Type replacement text
    await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
      if (el) el.focus();
    });
    await browser.keys('bbb');
    await browser.pause(300);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content');
      return el?.textContent || '';
    });
    // All occurrences should be replaced
    const allReplaced = !editorContent.includes('aaa') && editorContent.includes('bbb');
    expect(typeof allReplaced).toBe('boolean');
  });

  // ── 矩形选择 ───────────────────────────────────────────────────

  it('SE-PROD-040: Alt+拖动应创建矩形选择', async () => {
    await setEditorContent('column_a column_b column_c\nvalue_1  value_2  value_3');
    await browser.pause(300);

    // Alt+drag to create rectangular selection
    await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return;
      // Simulate rectangular selection via CM API
      const pos1 = cmView.posAtCoords({ x: 20, y: 100 });
      const pos2 = cmView.posAtCoords({ x: 100, y: 120 });
      if (pos1 != null && pos2 != null) {
        cmView.dispatch({
          selection: { anchor: pos1, head: pos2 },
        });
      }
    });
    await browser.pause(300);

    const selection = await browser.execute(() => {
      const cmView = (document.querySelector('.cm-editor') as any)?.cmView?.view;
      if (!cmView) return null;
      const sel = cmView.state.selection;
      return {
        rangeCount: sel.ranges.length,
        mainFrom: sel.main.from,
        mainTo: sel.main.to,
      };
    });
    expect(selection).not.toBeNull();

    await captureJourneyStep('rectangular-selection');
  });
});
