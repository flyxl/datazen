import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  clickCardConnectButton,
  closeExtraWindows,
  disconnectBackend,
  setEditorContent,
  openQueryTab,
  clickFirstTable,
  waitForNewConnectionDialog,
  waitForNewQueryButton,
  openNewConnectionDialogFromUi,
  expandNewConnectionSshSection,
  connectSeededPgInWorkspace,
} from '../helpers.js';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        ?.invoke(c, JSON.parse(a))
        .then((r) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

async function invokeBackendCatch(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<string> {
  try {
    await invokeBackend(cmd, args);
    return '';
  } catch (e) {
    return String(e);
  }
}

describe('Client parity P0–P2', () => {
  let mainWindow: string;
  let pgConnectionId = '';
  // Live db session ids captured from `connect` (never pass a persisted
  // connection id into a dbSessionId slot).
  let pgSessionId = '';
  let readOnlySessionId = '';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    const conns = await invokeBackend<{ id: string; name?: string }[]>('get_connections');
    const pg =
      conns.find((c) => c.id === 'conn_e2e_pg' || c.name === '本地 PostgreSQL') ?? conns[0];
    if (!pg) {
      throw new Error('No PostgreSQL connection seeded for client-parity E2E');
    }
    pgConnectionId = pg.id;
    pgSessionId = await invokeBackend<string>('connect', { connectionId: pgConnectionId });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    try {
      await invokeBackend('workflow_delete', { workflowId: 'e2e-scheduled-job' });
    } catch {
      /* ok */
    }
    try {
      if (readOnlySessionId) {
        await invokeBackend('disconnect', { dbSessionId: readOnlySessionId });
      }
    } catch {
      /* ok */
    }
    try {
      if (pgSessionId) {
        await disconnectBackend(pgSessionId);
      }
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: 'conn_e2e_readonly' });
    } catch {
      /* ok */
    }
  });

  it('Safe Mode blocks UPDATE without WHERE', async () => {
    const settings = await invokeBackend<{ safeMode: boolean }>('get_settings');
    expect(settings.safeMode).toBe(true);
    const err = await invokeBackendCatch('execute_driver_command', {
      request: {
        dbSessionId: pgSessionId,
        command: 'query',
        input: { sql: 'UPDATE _e2e_missing SET x = 1' },
      },
    });
    expect(err).toMatch(/WHERE/i);
  });

  it('Safe Mode blocks DROP', async () => {
    const err = await invokeBackendCatch('execute_driver_command', {
      request: {
        dbSessionId: pgSessionId,
        command: 'query',
        input: { sql: 'DROP TABLE IF EXISTS _e2e_should_not_drop' },
      },
    });
    expect(err).toMatch(/DROP/i);
  });

  it('substitutes bind params before execution', async () => {
    const result = await invokeBackend<{ data: { results?: { rows?: unknown[] }[] } }>(
      'execute_driver_command',
      {
        request: {
          dbSessionId: pgSessionId,
          command: 'query',
          input: { sql: 'SELECT :n::int AS n', params: { n: 7 } },
        },
      },
    );
    expect(result.data).toBeDefined();
  });

  it('session transaction begin/commit/rollback', async () => {
    await invokeBackend('begin_session_transaction', { dbSessionId: pgSessionId });
    expect(
      await invokeBackend<boolean>('session_transaction_status', { dbSessionId: pgSessionId }),
    ).toBe(true);
    await invokeBackend('rollback_session_transaction', { dbSessionId: pgSessionId });
    expect(
      await invokeBackend<boolean>('session_transaction_status', { dbSessionId: pgSessionId }),
    ).toBe(false);
    await invokeBackend('begin_session_transaction', { dbSessionId: pgSessionId });
    await invokeBackend('commit_session_transaction', { dbSessionId: pgSessionId });
    expect(
      await invokeBackend<boolean>('session_transaction_status', { dbSessionId: pgSessionId }),
    ).toBe(false);
  });

  it('lists routines and privileges on PostgreSQL', async () => {
    for (const kind of ['function', 'procedure', 'trigger'] as const) {
      const rows = await invokeBackend<{ name: string }[]>('get_database_objects', {
        dbSessionId: pgSessionId,
        kind,
      });
      expect(Array.isArray(rows)).toBe(true);
      for (const row of rows) {
        expect(typeof row.name).toBe('string');
      }
    }
    const grants = await invokeBackend<unknown[]>('get_privileges', { dbSessionId: pgSessionId });
    expect(Array.isArray(grants)).toBe(true);
  });

  it('saves a scheduled workflow', async () => {
    await invokeBackend('workflow_save', {
      workflow: {
        id: 'e2e-scheduled-job',
        name: 'E2E scheduled',
        description: 'parity',
        variables: [],
        steps: [{ type: 'query', id: 'q', sql: 'SELECT 1' }],
        schedule: { enabled: true, intervalSecs: 60 },
      },
    });
    const list = await invokeBackend<{ id: string; scheduled?: boolean }[]>('workflow_list');
    const found = list.find((w) => w.id === 'e2e-scheduled-job');
    expect(found?.scheduled).toBe(true);
    const detail = await invokeBackend<{ schedule?: { enabled: boolean } }>('workflow_get', {
      workflowId: 'e2e-scheduled-job',
    });
    expect(detail.schedule?.enabled).toBe(true);
  });

  it('read-only connection rejects writes', async () => {
    const src = await invokeBackend<Record<string, unknown>[]>('get_connections');
    const pg = src.find((c) => c.id === pgConnectionId);
    expect(pg).toBeDefined();
    await invokeBackend('save_connection', {
      config: { ...pg, id: 'conn_e2e_readonly', name: 'E2E ReadOnly', readOnly: true },
    });
    const liveId = await invokeBackend<string>('connect', { connectionId: 'conn_e2e_readonly' });
    readOnlySessionId = liveId;
    const err = await invokeBackendCatch('execute_driver_command', {
      request: {
        dbSessionId: liveId,
        command: 'query',
        input: { sql: 'UPDATE pg_catalog.pg_class SET relname = relname WHERE false' },
      },
    });
    expect(err).toMatch(/read-only/i);
    await invokeBackend('disconnect', { dbSessionId: liveId }).catch(() => undefined);
    await invokeBackend('delete_connection', { id: 'conn_e2e_readonly' });
  });

  it('query toolbar shows format, bind params, and transaction controls', async () => {
    await connectSeededPgInWorkspace();
    await openQueryTab();

    await expect(await $(`button*=${t('query.format')}`)).toBeDisplayed();
    await expect(await $(`button*=${t('query.beginTx')}`)).toBeDisplayed();
    await expect(await $(`button*=${t('query.commitTx')}`)).toBeDisplayed();
    await expect(await $(`button*=${t('query.rollbackTx')}`)).toBeDisplayed();
    await expect(await $(`span*=${t('settings.safeMode')}`)).toBeDisplayed();

    await setEditorContent('select id from t where id = :uid');
    await expect(await $(`button*=${t('query.format')}`)).toBeEnabled();
    const body = await $('body').getText();
    expect(body).toContain(':uid');

    await $(`button*=${t('query.format')}`).click();
    await browser.pause(400);
    const formatted = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content') as HTMLElement | null;
      return el?.textContent || '';
    });
    expect(formatted.toUpperCase()).toContain('SELECT');

    await expect(await $(`button*=${t('objects.title')}`)).toBeDisplayed();
    await expect(await $(`button*=${t('privileges.title')}`)).toBeDisplayed();
    await $(`button*=${t('objects.title')}`).click();
    await expect(await $(`button*=${t('objects.function')}`)).toBeDisplayed();
    const bodyAfterObjects = await $('body').getText();
    expect(bodyAfterObjects).not.toContain('Object list query missing name column');
    expect(
      bodyAfterObjects.includes(t('objects.empty')) ||
        bodyAfterObjects.includes(t('objects.function')) ||
        bodyAfterObjects.includes(t('objects.procedure')),
    ).toBe(true);
  });

  it('table filter editor opens AND/OR controls', async () => {
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(15000);
    const table = await clickFirstTable();
    if (!table) return;
    await browser.pause(800);
    const toggle = await $('[data-testid="table-filter-toggle"]');
    if (await toggle.isExisting()) {
      await toggle.click();
      await browser.pause(400);
    }
    const body = await $('body').getText();
    expect(
      body.includes(t('filter.and')) ||
        body.includes(t('filter.or')) ||
        body.includes(t('filter.add')) ||
        body.includes(t('filter.filter')),
    ).toBe(true);
  });

  it('new connection form shows SSH agent and jump host', async () => {
    await closeExtraWindows(mainWindow);
    await openNewConnectionDialogFromUi();
    await expandNewConnectionSshSection();
    const sshCheckbox = await $('[data-testid="new-conn-ssh-tunnel-checkbox"]');
    await sshCheckbox.waitForDisplayed({ timeout: 10000 });
    await sshCheckbox.click();
    await expect(await $(`button*=${t('newConn.authAgent')}`)).toBeDisplayed();
    await $(`button*=${t('newConn.authAgent')}`).click();
    await expect(await $(`div*=${t('newConn.authAgentHint')}`)).toBeDisplayed();
    await $(`label*=${t('newConn.sshJump')}`).click();
    await expect(await $(`div*=${t('newConn.sshJumpHost')}`)).toBeDisplayed();
  });
});
