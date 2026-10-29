/**
 * Review audit of the insert-frontend track — the **wire** shapes.
 *
 * Two properties are pinned here, both of them fail-closed: the `@ts-expect-error`
 * directives below turn into TS2578 ("unused '@ts-expect-error' directive") as
 * soon as the type they guard is weakened, so `tsc --noEmit` is the gate.
 *
 * 1. `RowChangePlan.inserts` is **required** in both mirrors (the host
 *    serializes it unconditionally).
 * 2. `insertLocation` is a **front-end only** field: it lives on
 *    `CommitPendingChangesResult` and on neither wire response type — the
 *    back end does not send it, so a mirror that carried it would be lying.
 *
 * The fixtures are typed explicitly (no `as const`): a `readonly []` would
 * make the object literal unassignable for a reason of its own, and the
 * `@ts-expect-error` would then be "used" by that unrelated error — the
 * directive would stop being a fail-closed pin.
 */
import { describe, expect, it } from 'vitest';
import { GRID_ERROR_CODES, classifyGridError } from '../gridErrors';
import type {
  ChangeWarning,
  CommitPendingChangesResponse as DesktopResponse,
  CommitPendingChangesResult as DesktopResult,
  CommitStatementResult as DesktopStatementResult,
  PlannedStatement,
  RowChangePlan as DesktopPlan,
  TableChangeContext,
} from '../tableChanges';
import type {
  CommitPendingChangesResponse as MirrorResponse,
  CommitStatementResult as MirrorStatementResult,
  PlannedStatement as MirrorPlannedStatement,
  RowChangePlan as MirrorPlan,
} from '@datazen/backend-client';

const TABLE: TableChangeContext = {
  connectionId: 'connection-1',
  dbSessionId: 'session-1',
  driverType: 'postgresql',
  database: 'app',
  schema: 'public',
  table: 'users',
};

const PLAN_BASE = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: TABLE,
  updates: [] as PlannedStatement[],
  deletes: [] as PlannedStatement[],
  warnings: [] as ChangeWarning[],
};

const MIRROR_PLAN_BASE = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: TABLE,
  updates: [] as MirrorPlannedStatement[],
  deletes: [] as MirrorPlannedStatement[],
  warnings: [] as ChangeWarning[],
};

const RESPONSE_BASE = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  statements: [] as DesktopStatementResult[],
  affectedRows: 0,
};

const MIRROR_RESPONSE_BASE = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  statements: [] as MirrorStatementResult[],
  affectedRows: 0,
};

describe('F · the insert error codes of the registry', () => {
  it('does not register a code without a producer', () => {
    // `grid.insert.noWritableColumn` has **no producer**: the NOT NULL check of
    // an insert row is not implemented. A registered code without a producer
    // would make the interface branch on a message that can never arrive.
    expect(GRID_ERROR_CODES).not.toContain('grid.insert.noWritableColumn');
    expect(
      classifyGridError('grid.insert.noWritableColumn: column "email" requires a value'),
    ).toBe('unknown');
  });

  it('still classifies the insert codes that do have a producer', () => {
    expect(
      classifyGridError("grid.insert.unknownColumn: INSERT writes column(s) that do not exist on 'users'"),
    ).toBe('grid.insert.unknownColumn');
    expect(
      classifyGridError('grid.insert.returningUnavailable: the new row could not be located'),
    ).toBe('grid.insert.returningUnavailable');
    // An unrelated message stays unknown, and the caller renders it verbatim.
    expect(classifyGridError('permission denied for table users')).toBe('unknown');
  });
});

describe('E · the plan mirror is fail-closed on `inserts`', () => {
  it('requires `inserts` on the desktop mirror', () => {
    // @ts-expect-error `inserts` is required: the host always serializes it.
    const plan: DesktopPlan = { ...PLAN_BASE, inserts: undefined };

    // A plan without inserts carries an empty list, never a missing key.
    const empty: DesktopPlan = { ...plan, inserts: [] };
    expect(JSON.parse(JSON.stringify(empty)).inserts).toEqual([]);
  });

  it('requires `inserts` on the backend-client mirror', () => {
    // @ts-expect-error `inserts` is required: the host always serializes it.
    const plan: MirrorPlan = { ...MIRROR_PLAN_BASE, inserts: undefined };

    const empty: MirrorPlan = { ...plan, inserts: [] };
    expect(JSON.parse(JSON.stringify(empty)).inserts).toEqual([]);
  });
});

describe('B4 · `insertLocation` is a front-end only field', () => {
  it('is on the front-end commit result', () => {
    const result: DesktopResult = {
      ...RESPONSE_BASE,
      status: 'committed',
      refreshed: true,
      refreshRequired: true,
      insertLocation: 'unavailable',
    };
    expect(result.insertLocation).toBe('unavailable');
  });

  it('is not on the desktop wire response', () => {
    // @ts-expect-error `insertLocation` is front-end only: the back end never sends it.
    const response: DesktopResponse = { ...RESPONSE_BASE, insertLocation: 'located' };
    expect(response).toBeDefined();
  });

  it('is not on the backend-client wire response', () => {
    // @ts-expect-error `insertLocation` is front-end only: the back end never sends it.
    const response: MirrorResponse = { ...MIRROR_RESPONSE_BASE, insertLocation: 'located' };
    expect(response).toBeDefined();
  });
});
