import { describe, expect, it } from 'vitest';

import type {
  ChangeWarning,
  CommitPendingChangesResponse,
  CommitStatementResult,
  PendingRowChange,
  PlannedStatement,
  RowChangePlan,
  TableChangeContext,
  TableDataResult,
} from '@datazen/backend-client';
import type {
  ChangeWarning as MirrorChangeWarning,
  CommitPendingChangesResponse as MirrorCommitPendingChangesResponse,
  CommitStatementResult as MirrorCommitStatementResult,
  PendingRowChange as MirrorPendingRowChange,
  PlannedStatement as MirrorPlannedStatement,
  RowChangePlan as MirrorRowChangePlan,
  TableChangeContext as MirrorTableChangeContext,
} from '../tableChanges';
import type { TableDataResult as MirrorTableDataResult } from '../../types';

/* ------------------------------------------------------------------ */
/* Compile-time parity with the backend-client wire shapes.           */
/*                                                                     */
/* form-split F6-c3: these assertions lived in                          */
/* `packages/backend-client/__tests__/tableDataClient.test.ts` and      */
/* reached the desktop mirror by relative climb — exactly the shape     */
/* ruling D1 / gate R5 forbids (`packages/**` may not resolve into      */
/* `packages/frontend/`). Parity belongs to the CONSUMER: the mirror    */
/* is ours, the wire types arrive via the sanctioned bare specifier     */
/* `@datazen/backend-client`, and a drift still fails `tsc --noEmit`    */
/* long before a test could notice.                                     */
/* ------------------------------------------------------------------ */

/** `true` only when `A` and `B` are mutually assignable. */
type MutuallyAssignable<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;

/**
 * `true` only when `A` and `B` declare exactly the same keys.
 *
 * Mutual assignability alone is not enough, and the gap is not academic: an
 * **optional** field that exists on one side and not the other is assignable
 * in *both* directions, so a mirror that silently dropped `insertMarked?`
 * would still pass an assignability check. The key comparison is what makes
 * that failure loud.
 */
type SameKeys<A, B> = [Exclude<keyof A, keyof B>] extends [never]
  ? [Exclude<keyof B, keyof A>] extends [never]
    ? true
    : false
  : false;

/**
 * `true` only when the two shapes are the same shape: same keys, and each
 * assignable to the other (which is what catches a required field that
 * became optional, or a field whose type drifted).
 */
type Parity<A, B> =
  MutuallyAssignable<A, B> extends true ? (SameKeys<A, B> extends true ? true : false) : false;

/** Fails to compile when the checked condition is not `true`. */
type Assert<T extends true> = T;

export type ContextParity = Assert<Parity<MirrorTableChangeContext, TableChangeContext>>;
export type PendingChangeParity = Assert<Parity<MirrorPendingRowChange, PendingRowChange>>;
export type PlannedStatementParity = Assert<Parity<MirrorPlannedStatement, PlannedStatement>>;
export type ChangeWarningParity = Assert<Parity<MirrorChangeWarning, ChangeWarning>>;
export type RowChangePlanParity = Assert<Parity<MirrorRowChangePlan, RowChangePlan>>;
export type CommitStatementParity = Assert<
  Parity<MirrorCommitStatementResult, CommitStatementResult>
>;
export type CommitResponseParity = Assert<
  Parity<MirrorCommitPendingChangesResponse, CommitPendingChangesResponse>
>;

/**
 * The table read result is mirrored too, from the desktop's `src/types`.
 *
 * It matters for the same reason: the same read must mean the same thing on
 * both sides of the port — in particular whether "the count was not taken"
 * arrives as `null` or as a missing key.
 */
export type TableDataResultParity = Assert<
  Parity<MirrorTableDataResult, TableDataResult>
>;

/** `true` only when the key `K` of `T` is **optional**. */
type IsOptional<T, K extends keyof T> = {} extends Pick<T, K> ? true : false;

/**
 * The insert flag stays optional on the wire.
 *
 * Fail-closed: making the flag required turns this into `false` and the
 * program red (the type assertion, not a runtime one — the backend-client
 * builds no payload of its own, so the runtime half of the property belongs
 * to the host's `pendingChangesForWire`, which pins it there).
 */
export type InsertMarkedStaysOptional = Assert<IsOptional<PendingRowChange, 'insertMarked'>>;

describe('tableChanges ↔ backend-client type parity', () => {
  it('holds (compile-time assertions above; this run only proves the harness loads)', () => {
    // The parity machinery is type-level: `tsc --noEmit` is the judge, and a
    // shape that stops matching is a typecheck failure long before a test
    // could notice. This runtime case exists so the file is a real suite
    // member rather than an orphan in the include list.
    expect(true).toBe(true);
  });
});
