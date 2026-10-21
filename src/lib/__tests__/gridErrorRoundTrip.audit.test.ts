/**
 * Independent verification of the host-error round trip (Wave 5 · track A).
 *
 * The deliverable's own round-trip file proves one thing: that a `grid.fk.*`
 * code survives the trip from the host to the front end. This file checks the
 * **broader** claim, because the transport change it relies on is not
 * foreign-key specific:
 *
 * - the change is in `src/platform/tauriBackendTransport.ts` — a plain-string
 *   rejection is wrapped into an `Error` so the message survives
 *   `deserializeApiError`. That affects **every** command of the desktop
 *   backend, not only the foreign-key one;
 * - the eight `grid.commit.*` / `grid.cell.*` / `grid.insert.*` codes were
 *   produced by the host since Wave 2 and were **unreachable** in the desktop
 *   UI, because `CommandError::Serialize` emits a plain string and
 *   `deserializeApiError` replaces a non-record payload with a sentence that
 *   carries no code at all;
 * - so the question is whether *those* codes reach the front end now, and
 *   whether they reach it as **classifiable** text.
 *
 * Every case here goes through the **real** desktop transport, the **real**
 * backend-client facade and the **real** `databaseCommands` wrapper — the same
 * chain the grid's commit button uses. Nothing calls the classifier directly on
 * a hand-made string.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';

const { mockInvoke } = vi.hoisted(() => ({ mockInvoke: vi.fn() }));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: mockInvoke,
  Channel: class {
    onmessage = (_message: unknown) => {};
  },
}));

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

import {
  ApiError,
  createBackendClient,
  selectBackend,
  setBackendClient,
  toId,
} from '@datazen/backend-client';
import {
  createDesktopBackendTransport,
  DESKTOP_BACKEND_ID,
} from '../../platform/tauriBackendTransport';
import { classifyGridError } from '../gridErrors';
import { hostErrorMessage } from '../fkNavigationActions';
import { databaseCommands } from '../../commands/database';
import { extractErrorMessage } from '../../stores/tableData/connectionState';
import type { RowChangePlan } from '../../lib/tableChanges';

const PLAN: RowChangePlan = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: {
    connectionId: 'conn-1',
    dbSessionId: 'session-1',
    driverType: 'postgres',
    database: 'app',
    schema: null,
    table: 'users',
  },
  updates: [],
  deletes: [],
  warnings: [],
} as unknown as RowChangePlan;

const CONTEXT = {
  connectionId: 'conn-1',
  dbSessionId: 'session-1',
  driverType: 'postgres',
  database: 'app',
  schema: null,
  table: 'users',
};

/**
 * The codes this file walks through the chain, with the text the host sends.
 *
 * They are the shapes the Rust side really produces:
 * `FeatureError::Validation("grid.<domain>.<reason>: …")` →
 * `CommandError::Validation` → `CommandError::Serialize` → **a plain string**.
 */
const HOST_CASES = [
  {
    code: 'grid.commit.affectedMismatch',
    text: 'grid.commit.affectedMismatch: Row identity affected 2 rows; refusing ambiguous write',
  },
  {
    code: 'grid.cell.readOnly',
    text: "grid.cell.readOnly: update requires the table's primary key, but 'users' reports none",
  },
  {
    code: 'grid.insert.unknownColumn',
    text: "grid.insert.unknownColumn: insert writes column(s) that do not exist on 'users': ghost",
  },
  {
    code: 'grid.commit.conflictingIntents',
    text:
      'grid.commit.conflictingIntents: a row marked for delete cannot also carry column writes',
  },
] as const;

describe('audit: the host error round trip, for codes other than grid.fk.*', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    const client = createBackendClient(
      DESKTOP_BACKEND_ID,
      createDesktopBackendTransport(),
    );
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);
  });

  it.each(HOST_CASES)(
    'reaches the front end as $code, through the commit path',
    async ({ code, text }) => {
      mockInvoke.mockRejectedValue(text);

      let caught: unknown = null;
      try {
        await databaseCommands.commitPendingChanges({
          dbSessionId: 'session-1',
          plan: PLAN,
          fingerprint: PLAN.fingerprint,
        });
      } catch (error) {
        caught = error;
      }

      // The rejection is an ApiError, as it always was.
      expect(caught).toBeInstanceOf(ApiError);
      const apiError = caught as ApiError;
      // The code of the envelope is unchanged by the transport change: a
      // plain string still decodes to "we do not know which code it was".
      expect(apiError.code).toBe('ServiceUnavailable');

      // **The host's own text is in the cause chain**, verbatim.
      const cause = (apiError as { cause?: unknown }).cause;
      expect(cause).toBeInstanceOf(Error);
      expect((cause as Error).message).toBe(text);

      // And it is the text the front end classifies: the code is real, not
      // "unknown".
      expect(hostErrorMessage(caught)).toBe(text);
      expect(classifyGridError(hostErrorMessage(caught))).toBe(code);

      // The wrapper's own message would classify to "unknown" — the prefix
      // "<method> failed: " moves the code away from the anchor. This is why
      // the cause chain has to be walked.
      expect(classifyGridError(apiError.message)).toBe('unknown');
      expect(apiError.message).toContain(text);
    },
  );

  it('reaches the front end as grid.cell.readOnly, through the preview path', async () => {
    const text =
      "grid.cell.readOnly: update requires the table's primary key, but 'users' reports none";
    mockInvoke.mockRejectedValue(text);

    let caught: unknown = null;
    try {
      await databaseCommands.previewPendingChanges({ context: CONTEXT, changes: [] });
    } catch (error) {
      caught = error;
    }

    expect(hostErrorMessage(caught)).toBe(text);
    expect(classifyGridError(hostErrorMessage(caught))).toBe('grid.cell.readOnly');
  });

  it('shows the code in the error the grid renders (the store error text)', async () => {
    const text =
      'grid.commit.affectedMismatch: Row identity affected 2 rows; refusing ambiguous write';
    mockInvoke.mockRejectedValue(text);

    let caught: unknown = null;
    try {
      await databaseCommands.commitPendingChanges({
        dbSessionId: 'session-1',
        plan: PLAN,
        fingerprint: PLAN.fingerprint,
      });
    } catch (error) {
      caught = error;
    }

    // This is the exact function the table-data store uses to turn a failed
    // command into the text of the error bar.
    const shown = extractErrorMessage(caught, 'Commit failed');
    expect(shown).toContain('grid.commit.affectedMismatch');
    expect(shown).not.toContain('unreadable payload');
  });
});

describe('audit: hostErrorMessage walks the cause chain', () => {
  it('takes the raw text out of a wrapper that prefixes it', () => {
    const raw = 'grid.fk.probeFailed: the probe did not answer';
    const wrapped = new ApiError('ServiceUnavailable', `resolve failed: ${raw}`, {
      cause: new Error(raw),
    });

    expect(hostErrorMessage(wrapped)).toBe(raw);
    expect(classifyGridError(hostErrorMessage(wrapped))).toBe('grid.fk.probeFailed');
  });

  it('takes the deepest text of a longer chain', () => {
    const raw = 'grid.commit.stalePlan: the session drifted';
    const middle = new Error(`outer: ${raw}`, { cause: new Error(raw) });
    const outer = new Error(`outermost: ${raw}`, { cause: middle });

    expect(hostErrorMessage(outer)).toBe(raw);
  });

  it('takes a plain string rejection as it is', () => {
    const raw = 'grid.cell.noRowIdentity: the row has no identity';
    expect(hostErrorMessage(raw)).toBe(raw);
  });

  it('falls back to the string of the value when there is no message', () => {
    expect(hostErrorMessage('grid.fk.valueNull: no value')).toBe('grid.fk.valueNull: no value');
    expect(hostErrorMessage(undefined)).toBe('undefined');
  });
});

describe('audit: the transport change does not touch the capability path', () => {
  it('still refuses an event subscription with CapabilityUnsupported', async () => {
    // A backend without stream support: the refusal is produced by the
    // facade, before any transport call, so the transport change cannot
    // reach it.
    const client = createBackendClient('no-streams', {
      async call() {
        throw new Error('the capability check should have refused this call');
      },
    });
    setBackendClient('no-streams', client);
    selectBackend('no-streams');

    const streamId = toId('s-1');
    if (!streamId) throw new Error('the stream id must be a non-empty string');
    const stream = client.subscribeEvents({ streamId, afterSequence: null });
    let caught: unknown = null;
    try {
      for await (const _event of stream) {
        // The stream ends immediately; the rejection happens on the first pull.
      }
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(ApiError);
    expect((caught as ApiError).code).toBe('CapabilityUnsupported');
    expect((caught as ApiError).unsupportedKind).toBeUndefined();
  });

  it('leaves the desktop capability refusals untouched', async () => {
    const client = createBackendClient(
      DESKTOP_BACKEND_ID,
      createDesktopBackendTransport(),
    );
    setBackendClient(DESKTOP_BACKEND_ID, client);
    selectBackend(DESKTOP_BACKEND_ID);

    // `estimateTableRows` is declared "notImplemented" by the desktop.
    let caught: unknown = null;
    try {
      await client.estimateTableRows({
        dbSessionId: 'session-1',
        table: 'users',
        database: 'app',
        schema: null,
      });
    } catch (error) {
      caught = error;
    }

    expect(caught).toBeInstanceOf(ApiError);
    const apiError = caught as ApiError;
    expect(apiError.code).toBe('CapabilityUnsupported');
    expect(apiError.unsupportedKind).toBe('notImplemented');
    // The capability refusal never went through the wire, so it has no cause
    // chain — and the transport change did not give it one.
    expect((apiError as { cause?: unknown }).cause).toBeUndefined();
  });
});
