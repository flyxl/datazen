/**
 * The round trip of an error message (DB-07).
 *
 * The `grid.*` codes are only worth anything if the **message** survives the
 * trip from the host to the front end — and the trip is three layers deep:
 *
 *   Rust `CommandError` → (a plain string on the wire) → the desktop
 *   transport → the backend client → the front end.
 *
 * Two layers used to lose it:
 *
 * - `deserializeApiError` replaces a rejection that is not a record with
 *   "Backend rejected the request with an unreadable payload." — and a
 *   `CommandError` arrives as exactly that: a string;
 * - `asApiError` prefixes the message of a failed call with
 *   `"<method> failed: "`, which moves the `grid.*` prefix away from the
 *   start of the text the classifier anchors on.
 *
 * This file is the **end-to-end** proof that neither happens: the message the
 * front end classifies still starts with the host's prefix, and the code it
 * classifies to is the real one — not `unknown`.
 *
 * It is deliberately not a classifier test: the classifier is covered by
 * `src/lib/__tests__/gridErrors.fk.test.ts`, and that test cannot see the
 * wire at all.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

vi.mock('@datazen/frontend/src/lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

import { createBackendClient, selectBackend, setBackendClient } from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../desktop/tauriBackendTransport';
import { classifyGridError } from '@datazen/frontend/src/lib/gridErrors';
import { hostErrorMessage, openForeignKeyTarget } from '@datazen/frontend/src/lib/fkNavigationActions';
import { usePanelStore } from '@datazen/frontend/src/stores/panelStore';
import { useFkNavigationStore, resetFkNavigationEntryIds } from '@datazen/frontend/src/stores/fkNavigationStore';
import { useTableDataStore } from '@datazen/frontend/src/stores/tableDataStore';
import type { ForeignKeyInfo } from '@datazen/frontend/src/types/index';

/** The message the host sends: the prefix is the contract (07 §3.2). */
const HOST_MESSAGE =
  'grid.fk.targetUnresolved: the referenced table "customers" could not be resolved';

const FOREIGN_KEY: ForeignKeyInfo = {
  name: 'orders_customer_fk',
  columns: ['customer_id'],
  referencedTable: 'customers',
  referencedColumns: ['id'],
};

describe('the round trip of a grid error message', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetFkNavigationEntryIds();
    usePanelStore.setState({ panels: [], activePanelId: null });
    useTableDataStore.getState().reset();
    useFkNavigationStore.getState().reset();
    usePanelStore.getState().addPanel({
      id: 'panel-orders',
      type: 'table',
      tableName: 'orders',
      database: 'app',
      tableSchema: null,
      subTab: 'data',
      connectionId: 'conn-1',
      dbSessionId: 'session-1',
      connectionName: 'Local',
      databaseType: 'postgresql',
    });
    const client = createBackendClient(DESKTOP_BACKEND_ID, createDesktopBackendTransport());
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
    mockInvoke.mockRejectedValue(HOST_MESSAGE);
  });

  it('keeps the host message in the cause chain of the rejection', async () => {
    const { useBackendClient } = await import('@datazen/backend-client');
    let caught: unknown = null;
    try {
      await useBackendClient().resolveForeignKeyTargets({
        dbSessionId: 'session-1',
        table: 'orders',
        columns: ['customer_id'],
        values: [7],
      });
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(Error);
    // The cause is the host's own text, verbatim.
    const cause = (caught as { cause?: unknown }).cause;
    expect(cause).toBeInstanceOf(Error);
    expect((cause as Error).message).toBe(HOST_MESSAGE);
  });

  it('classifies the message the front end gets, not "unknown"', async () => {
    const { useBackendClient } = await import('@datazen/backend-client');
    let caught: unknown = null;
    try {
      await useBackendClient().resolveForeignKeyTargets({
        dbSessionId: 'session-1',
        table: 'orders',
        columns: ['customer_id'],
        values: [7],
      });
    } catch (error) {
      caught = error;
    }

    // What the front end works with: the host's text, without the wrapper's
    // prefix — that prefix would move the code away from the start of the
    // text the classifier anchors on.
    const message = hostErrorMessage(caught);
    expect(message).toBe(HOST_MESSAGE);
    expect(message.startsWith('grid.fk.targetUnresolved')).toBe(true);
    expect(classifyGridError(message)).toBe('grid.fk.targetUnresolved');
    // The wrapper's prefix would have broken it — that is why the cause
    // chain is walked.
    expect(classifyGridError((caught as Error).message)).toBe('unknown');
  });

  it('keeps a non-foreign-key grid code readable in the error banner', async () => {
    // The **other** grid codes were just as unreachable as `grid.fk.*`: the
    // host's message was dropped on the way to the front end. This is the
    // proof that the change reaches them too — through a path that has
    // nothing to do with foreign keys (the staged-change commit).
    const panelId = 'panel-orders';
    mockInvoke.mockResolvedValueOnce({
      columns: [
        { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
        { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
      ],
      rows: [[1, 'Alice']],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await useTableDataStore.getState().loadTableData({
      panelId,
      dbSessionId: 'session-1',
      table: 'orders',
      connectionId: 'conn-1',
      driverType: 'postgresql',
      database: 'app',
      schema: null,
    });
    useTableDataStore.getState().stageCellChange(panelId, 0, 'name', 'Alicia');

    mockInvoke.mockRejectedValue('grid.commit.stalePlan: Row change plan fingerprint is stale');
    await useTableDataStore.getState().commitPendingChanges(panelId);

    // The error banner of the grid carries the host's code.
    const error = useTableDataStore.getState().byPanel.get(panelId)?.error ?? '';
    expect(error).toContain('grid.commit.stalePlan');
  });

  it('reports the classified code of a refused jump, end to end', async () => {
    const outcome = await openForeignKeyTarget({
      sourcePanelId: 'panel-orders',
      sourceTable: 'orders',
      dbSessionId: 'session-1',
      database: 'app',
      schema: null,
      connectionId: 'conn-1',
      driverType: 'postgresql',
      foreignKey: FOREIGN_KEY,
      values: [7],
    });

    expect(outcome.ok).toBe(false);
    expect(outcome.error?.code).toBe('grid.fk.targetUnresolved');
    expect(outcome.error?.raw).toBe(HOST_MESSAGE);
    expect(classifyGridError(outcome.error?.raw ?? '')).toBe('grid.fk.targetUnresolved');
    // The refusal opened no tab.
    expect(usePanelStore.getState().panels).toHaveLength(1);
  });
});
