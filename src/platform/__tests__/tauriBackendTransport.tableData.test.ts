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

    // `get_table_data` also exists on the Rust side, but the grid reads
    // through the target-bound one; the port has to name the command the
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

  it('refuses the commands the host does not register, before any IPC leaves', async () => {
    // The desktop declares the 08 commands unsupported, so a call gets an
    // explicit failure that names the method and the backend instead of
    // Tauri's bare "command not found" — and the pipe is never touched.
    mock.invoke.mockResolvedValue(undefined);
    const client = createBackendClient('desktop', createDesktopBackendTransport());

    await expect(client.listNamedViews({})).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
      unsupportedKind: 'notImplemented',
    });
    await expect(client.saveNamedView({ view: {} as never })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
    });
    await expect(client.deleteNamedView({ id: 'view-1' })).rejects.toMatchObject({
      code: 'CapabilityUnsupported',
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

    // And the three it does not are not found — which is what makes the
    // declaration above necessary rather than redundant.
    for (const command of ['list_named_views', 'save_named_view', 'delete_named_view']) {
      expect(registered.has(command)).toBe(false);
    }
  });

  it('declares the gaps as "not implemented", not as a missing capability', () => {
    // The desktop *can* serve these methods; it has not implemented them
    // yet. Calling that a capability gap would tell a caller to hide a
    // feature that is about to arrive.
    for (const kind of Object.values(declared)) {
      expect(kind).toBe('notImplemented');
    }
  });
});
