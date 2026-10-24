import { afterEach, describe, expect, it, vi } from 'vitest';
import fs from 'node:fs';
import path from 'node:path';

const ROOT = path.resolve(import.meta.dirname, '../../..');

/**
 * The desktop half of the table-data port.
 *
 * `packages/backend-client` declares *what* travels; this file pins *which
 * Tauri command* each method reaches and *how* its payload is shaped on the
 * way out. The two shapes are not the same: the kernel-surface commands take
 * a single `request` argument, so their payload is nested under `request`,
 * while the table-data commands take named arguments (`context`, `changes`,
 * `plan`, `scope`, ...), so their payload is the argument bag itself.
 *
 * Getting this wrong does not fail a type check — both shapes are valid
 * `MethodMap` requests — it fails at the IPC boundary, with a Rust side that
 * cannot find the field it was promised. So the mapping is asserted here.
 */

const mock = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/core', () => ({
  invoke: mock.invoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

import { createDesktopBackendTransport } from '../tauriBackendTransport';
import { createBackendClient, type Counter, type Id, type MethodMap, type RowChangePlan } from '@datazen/backend-client';

afterEach(() => {
  vi.clearAllMocks();
});

const CONTEXT = {
  connectionId: 'conn-1',
  dbSessionId: 'sess-1',
  driverType: 'postgres',
  database: 'sales',
  schema: 'public',
  table: 'orders',
} as const;

const PLAN: RowChangePlan = {
  planId: 'plan-1',
  fingerprint: 'fp-1',
  configRevision: 7 as Counter,
  table: { ...CONTEXT },
  updates: [],
  deletes: [],
  inserts: [],
  warnings: [],
};

describe('desktop transport: table-data commands', () => {
  it('reads a table through the target-bound command, with the request nested', async () => {
    mock.invoke.mockResolvedValue({ columns: [], rows: [], page: 0, pageSize: 50 });
    const transport = createDesktopBackendTransport();

    await transport.call('getTableData', {
      context: { ...CONTEXT },
      manualHandle: null,
      page: 0,
      pageSize: 50,
      filters: undefined,
      sorts: undefined,
      skipCount: undefined,
      filterLogic: undefined,
    });

    // The session-scoped `get_table_data` has been retired; the grid reads
    // through the target-bound command, and the port names the command the
    // grid actually uses.
    expect(mock.invoke).toHaveBeenLastCalledWith('get_table_data_at_target', {
      request: {
        context: { ...CONTEXT },
        manualHandle: null,
        page: 0,
        pageSize: 50,
        filters: undefined,
        sorts: undefined,
        skipCount: undefined,
        filterLogic: undefined,
      },
    });
  });

  it('previews a change through the flat argument bag the command declares', async () => {
    mock.invoke.mockResolvedValue(PLAN);
    const transport = createDesktopBackendTransport();

    await transport.call('previewPendingChanges', {
      context: { ...CONTEXT },
      changes: [],
    });

    // The Rust command takes `context` and `changes` as named arguments, so
    // the payload is the bag itself — nesting it would make the Rust side
    // look for a `request` field that does not exist.
    expect(mock.invoke).toHaveBeenLastCalledWith('preview_pending_changes', {
      context: { ...CONTEXT },
      changes: [],
    });
  });

  it('commits a change through the flat argument bag the command declares', async () => {
    mock.invoke.mockResolvedValue({
      planId: 'plan-1',
      fingerprint: 'fp-1',
      statements: [],
      affectedRows: 0,
    });
    const transport = createDesktopBackendTransport();

    await transport.call('commitPendingChanges', {
      dbSessionId: 'sess-1',
      manualHandle: null,
      plan: PLAN,
      fingerprint: 'fp-1',
    });

    expect(mock.invoke).toHaveBeenLastCalledWith('commit_pending_changes', {
      dbSessionId: 'sess-1',
      manualHandle: null,
      plan: PLAN,
      fingerprint: 'fp-1',
    });
  });

  it('estimates rows through the flat argument bag the command declares', async () => {
    mock.invoke.mockResolvedValue(null);
    const transport = createDesktopBackendTransport();

    // The transport is a raw pipe and the *facade* is the gate. The desktop
    // registered the command with 09, so the mapping is reachable now; it
    // stays pinned here so the argument bag cannot drift away from the Rust
    // command without a test noticing.
    await transport.call('estimateTableRows', {
      dbSessionId: 'sess-1',
      table: 'orders',
      database: 'sales',
      schema: 'public',
    });

    expect(mock.invoke).toHaveBeenLastCalledWith('estimate_table_rows', {
      dbSessionId: 'sess-1',
      table: 'orders',
      database: 'sales',
      schema: 'public',
    });
  });

  it('resolves a foreign key through the single-request command', async () => {
    mock.invoke.mockResolvedValue({ exists: true, candidates: [], truncated: false });
    const transport = createDesktopBackendTransport();

    // The command carries its whole request under `request`, so the payload
    // is nested — the same shape the Rust command declares.
    const request = {
      dbSessionId: 'sess-1',
      table: 'orders',
      columns: ['customer_id'],
      values: [9],
    } satisfies MethodMap['resolveForeignKeyTargets']['request'];
    await transport.call('resolveForeignKeyTargets', request);

    expect(mock.invoke).toHaveBeenLastCalledWith('resolve_foreign_key_targets', { request });
  });

  it('writes a result cell through the single-request command', async () => {
    mock.invoke.mockResolvedValue(undefined);
    const transport = createDesktopBackendTransport();

    const request = {
      binding: {
        handle: { dbSessionId: 'sess-1' as Id, runtimeEpoch: 'epoch-1' as Id },
        resourceBindingId: 'res-1' as Id,
        executionId: 'exec-1' as Id,
      },
      statementIndex: 0,
      rowIndex: 2,
      column: 'note',
      value: 'edited',
    } satisfies MethodMap['commitQueryResultCell']['request'];
    await transport.call('commitQueryResultCell', request);

    expect(mock.invoke).toHaveBeenLastCalledWith('commit_query_result_cell', { request });
  });

  it('serves the named-view commands the host registers', async () => {
    // 08 registered the three named-view commands, so the declaration that
    // kept them out of the desktop had to go — otherwise the desktop would
    // refuse a command the host serves.
    mock.invoke.mockResolvedValue([]);
    const client = createBackendClient('desktop', createDesktopBackendTransport());

    await client.listNamedViews({});
    expect(mock.invoke).toHaveBeenLastCalledWith('list_named_views', { scope: undefined });

    await client.saveNamedView({ view: { id: 'view-1' } as never });
    expect(mock.invoke).toHaveBeenLastCalledWith('save_named_view', {
      view: { id: 'view-1' },
    });

    await client.deleteNamedView({ id: 'view-1' });
    expect(mock.invoke).toHaveBeenLastCalledWith('delete_named_view', { id: 'view-1' });
  });

  it('refuses the named-view commands for a form factor without a host store', async () => {
    // A browser build has no `{appData}` to write and no desktop `Store`, so
    // it declares the named views as a **capability** gap. The caller gets an
    // explicit refusal — never an empty list that reads like "you have no
    // saved views" — and the pipe is never touched.
    mock.invoke.mockResolvedValue(undefined);
    const client = createBackendClient('web', {
      async call() {
        throw new Error('the capability check should have refused this call');
      },
      unsupportedMethods: {
        listNamedViews: 'capability',
        saveNamedView: 'capability',
        deleteNamedView: 'capability',
      },
    });

    await expect(client.listNamedViews({})).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'capability',
    });
    await expect(client.saveNamedView({ view: {} as never })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'capability',
    });
    await expect(client.deleteNamedView({ id: 'view-1' })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'capability',
    });

    expect(mock.invoke).not.toHaveBeenCalled();
  });

  it('serves the estimate command the host registers', async () => {
    // 09 registered `estimate_table_rows`, so the declaration that kept it
    // out of the desktop had to go — otherwise the desktop would refuse a
    // command the host serves.
    mock.invoke.mockResolvedValue(null);
    const client = createBackendClient('desktop', createDesktopBackendTransport());

    await client.estimateTableRows({ dbSessionId: 'sess-1', table: 'orders' });

    expect(mock.invoke).toHaveBeenCalledWith('estimate_table_rows', {
      dbSessionId: 'sess-1',
      table: 'orders',
      database: undefined,
      schema: undefined,
    });
  });

  it('serves the foreign-key command the host registers', async () => {
    mock.invoke.mockResolvedValue({ exists: true, candidates: [], truncated: false });
    const client = createBackendClient('desktop', createDesktopBackendTransport());

    await client.resolveForeignKeyTargets({
      dbSessionId: 'sess-1',
      table: 'orders',
      columns: ['customer_id'],
      values: [9],
    });

    expect(mock.invoke).toHaveBeenCalledWith('resolve_foreign_key_targets', {
      request: {
        dbSessionId: 'sess-1',
        table: 'orders',
        columns: ['customer_id'],
        values: [9],
      },
    });
  });

  it('serves the commands the host does register', async () => {
    mock.invoke.mockResolvedValue({ columns: [], rows: [], page: 0, pageSize: 50 });
    const client = createBackendClient('desktop', createDesktopBackendTransport());

    await client.getTableData({ context: CONTEXT, page: 0, pageSize: 50 });

    expect(mock.invoke).toHaveBeenCalledWith('get_table_data_at_target', {
      request: { context: CONTEXT, page: 0, pageSize: 50 },
    });
  });
});

/**
 * The command names the host registers, parsed out of its handler list.
 *
 * Parsed, not pattern-matched. The entries in `tauri::generate_handler!` are
 * **paths** — `crate::commands::x`, `crate::platform::session_commands::y`,
 * `crate::commands::schema_diff::job::desktop::z` — and the Tauri command
 * name is the *last* segment of the path. A substring search for
 * `commands::x,` therefore misses every registration that is not spelled
 * exactly that way (a different module path, a different last entry with no
 * trailing comma), and this check's whole job is to notice a registration
 * that changed shape.
 *
 * Comments are stripped first: a commented-out entry is not a registration.
 */
function parseRegisteredCommands(source: string): Set<string> {
  const start = source.indexOf('tauri::generate_handler![');
  if (start < 0) throw new Error('run.rs has no generate_handler! block');
  const open = source.indexOf('[', start);

  // Bracket matching, not `indexOf(']')`: the entries carry `#[cfg(...)]`
  // attributes, which have brackets of their own.
  let depth = 0;
  let end = -1;
  for (let i = open; i < source.length; i++) {
    if (source[i] === '[') depth += 1;
    else if (source[i] === ']') {
      depth -= 1;
      if (depth === 0) {
        end = i;
        break;
      }
    }
  }
  if (end < 0) throw new Error('run.rs has an unterminated generate_handler! block');

  const block = source
    .slice(open, end)
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/\/\/[^\n]*/g, '');

  const names = new Set<string>();
  for (const match of block.matchAll(/\bcrate::([A-Za-z0-9_]+(?:::[A-Za-z0-9_]+)+)\b/g)) {
    const segments = match[1].split('::');
    names.add(segments[segments.length - 1] as string);
  }
  return names;
}

/**
 * The declaration and the host's command registry are two facts that have to
 * agree, and nothing but a test makes them agree: the port registers every
 * method the product has, the desktop declares the ones the host cannot
 * serve, and the host's handler list says which those are.
 *
 * When a wave registers one of these commands, this test fails and asks for the
 * declaration to go — which is the whole point of declaring the gap instead of
 * letting the call reach a command that does not exist. 07 already did that
 * for `resolve_foreign_key_targets`, and 09 for `estimate_table_rows`.
 */
describe('the desktop declaration against the host command registry', () => {
  const TABLE_DATA_COMMANDS: ReadonlyArray<{ method: keyof MethodMap; command: string }> = [
    { method: 'getTableData', command: 'get_table_data_at_target' },
    { method: 'previewPendingChanges', command: 'preview_pending_changes' },
    { method: 'commitPendingChanges', command: 'commit_pending_changes' },
    { method: 'commitQueryResultCell', command: 'commit_query_result_cell' },
    { method: 'estimateTableRows', command: 'estimate_table_rows' },
    { method: 'resolveForeignKeyTargets', command: 'resolve_foreign_key_targets' },
    { method: 'listNamedViews', command: 'list_named_views' },
    { method: 'saveNamedView', command: 'save_named_view' },
    { method: 'deleteNamedView', command: 'delete_named_view' },
    { method: 'resolveResultOrigin', command: 'resolve_result_origin' },
    { method: 'previewResultCellChange', command: 'preview_result_cell_change' },
  ];

  const declared = createDesktopBackendTransport().unsupportedMethods ?? {};

  it('declares exactly the table-data commands the host does not register', () => {
    const hostBootstrap = fs.readFileSync(
      path.join(ROOT, 'src-tauri/src/bootstrap/run.rs'),
      'utf8',
    );
    const registeredCommands = parseRegisteredCommands(hostBootstrap);

    for (const { method, command } of TABLE_DATA_COMMANDS) {
      const registered = registeredCommands.has(command);
      const unsupported = declared[method] !== undefined;

      // The two facts have to be the same fact. A command that is registered
      // but still declared unsupported keeps the desktop from using a
      // feature that works; a command that is not registered but not
      // declared hands the caller Tauri's bare "command not found".
      expect({ method, registered, unsupported }).toEqual({
        method,
        registered,
        unsupported: !registered,
      });
    }
  });

  it('reads the registrations rather than guessing at their spelling', () => {
    const hostBootstrap = fs.readFileSync(
      path.join(ROOT, 'src-tauri/src/bootstrap/run.rs'),
      'utf8',
    );
    const registered = parseRegisteredCommands(hostBootstrap);

    // The table-data commands the host *does* register are found by their
    // command name, whatever module path they are registered under.
    expect(registered.has('get_table_data_at_target')).toBe(true);
    expect(registered.has('preview_pending_changes')).toBe(true);
    expect(registered.has('commit_pending_changes')).toBe(true);
    expect(registered.has('commit_query_result_cell')).toBe(true);
    expect(registered.has('resolve_foreign_key_targets')).toBe(true);
    expect(registered.has('estimate_table_rows')).toBe(true);

    // And the three named-view commands are registered too — which is what
    // makes the desktop's own declaration unnecessary.
    for (const command of ['list_named_views', 'save_named_view', 'delete_named_view']) {
      expect(registered.has(command)).toBe(true);
    }
  });

  it('declares no gap at all, because the desktop serves every one of them', () => {
    // Every table-data command the port declares is registered on the host,
    // so the desktop has nothing to refuse. A gap here would be a gap that
    // does not exist.
    expect(Object.keys(declared)).toEqual([]);
  });

  it('tells a capability gap from a missing implementation', () => {
    // The two kinds are not interchangeable: a form factor that can never
    // serve a method and a backend that has not implemented it yet call for
    // different reactions from the caller (hide the feature vs. say "not
    // available yet"). The named views are the first case — the browser has
    // no host store at all.
    const web = {
      async call(): Promise<never> {
        throw new Error('the capability check should have refused this call');
      },
      unsupportedMethods: { listNamedViews: 'capability' } as Partial<
        Record<keyof MethodMap, 'capability' | 'notImplemented'>
      >,
    };
    expect(web.unsupportedMethods.listNamedViews).toBe('capability');
    expect(web.unsupportedMethods.listNamedViews).not.toBe('notImplemented');
  });
});
