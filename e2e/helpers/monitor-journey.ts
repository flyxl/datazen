import { browser } from '@wdio/globals';

/** Classify in the webview so raw driver/IPC errors never enter WDIO responses. */
export async function invokeMonitorBackend<T>(
  command: string,
  args: Record<string, unknown>,
): Promise<T> {
  type Reply = { ok: true; value: T } | { ok: false; code: string };
  let reply: Reply;
  try {
    reply = (await browser.executeAsync((command: string, args: Record<string, unknown>, done: (reply: unknown) => void) => {
      const api = window as unknown as { __TAURI_INTERNALS__: { invoke: <R>(command: string, args: Record<string, unknown>) => Promise<R> } };
      void api.__TAURI_INTERNALS__.invoke<T>(command, args).then(value => done({ ok: true, value })).catch((error: unknown) => {
        const text = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
        const code = text.includes('missing field `description`') ? 'MISSING_WORKFLOW_DESCRIPTION'
          : text.includes('invalid args') ? 'INVALID_IPC_ARGUMENTS' : 'IPC_REJECTED';
        done({ ok: false, code });
      });
    }, command, args)) as Reply;
  } catch { throw new Error(`Monitor journey command failed: ${command} [WEBDRIVER_TRANSPORT]`); }
  if (!reply.ok) throw new Error(`Monitor journey command failed: ${command} [${reply.code}]`);
  return reply.value;
}

/** delete_dashboard returns NotFound for absence; missing fixture is already clean. */
export async function deleteMonitorDashboard(id: string): Promise<void> {
  const dashboards = await invokeMonitorBackend<Array<{ id: string }>>('list_dashboards', {});
  if (dashboards.some(dashboard => dashboard.id === id)) {
    await invokeMonitorBackend('delete_dashboard', { id });
  }
}
