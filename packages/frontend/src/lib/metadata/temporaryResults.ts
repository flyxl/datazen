/**
 * Temporary-result invalidation rules.
 *
 * Temporary objects (redacted preview buffers, ad-hoc batch snapshots,
 * per-execution temp tables surfaced to the UI) are always bound to the
 * *original runtime handle* — the `SessionHandle` (`dbSessionId` +
 * `runtimeEpoch`) of the session they came from. They are never rebindable:
 * once the runtime epoch changes, the entry is invalid.
 *
 * Old *result* provenance (`ResultProvenance`: connectionId, configRevision,
 * requested target, capability snapshot) is retained for audit but is marked
 * stale once any of its identity/config/target dimensions no longer matches
 * the caller's current context. A stale provenance entry may be displayed
 * ("result from ...") but must not be treated as writable or current, and any
 * temporary result derived from it is dropped.
 */

import type { ExecutionTarget, ResultProvenance, SessionHandle } from '@datazen/backend-client';

export interface TemporaryResult<T> {
  /** The runtime handle this temporary object is bound to (origin). */
  readonly originalHandle: SessionHandle;
  /** Provenance of the call that produced the value, when known. */
  readonly provenance: ResultProvenance | null;
  readonly value: T;
}

export type InvalidationReason =
  | 'runtimeEpochChanged'
  | 'connectionChanged'
  | 'configRevisionChanged'
  | 'targetChanged';

export interface InvalidationDecision {
  valid: boolean;
  reasons: InvalidationReason[];
}

/**
 * Decide whether a temporary result is still valid for the *current*
 * runtime handle and current call context. Returns the list of violated
 * rules; `valid` is true only when nothing changed.
 */
export function checkTemporaryResult(
  entry: TemporaryResult<unknown>,
  current: { handle: SessionHandle; connectionId: string; configRevision: number; target: ExecutionTarget },
): InvalidationDecision {
  const reasons: InvalidationReason[] = [];
  if (entry.originalHandle.runtimeEpoch !== current.handle.runtimeEpoch ||
      entry.originalHandle.dbSessionId !== current.handle.dbSessionId) {
    reasons.push('runtimeEpochChanged');
  }
  if (entry.provenance) {
    if (entry.provenance.connectionId !== current.connectionId) reasons.push('connectionChanged');
    if (entry.provenance.configRevision !== current.configRevision) reasons.push('configRevisionChanged');
    if (
      entry.provenance.requestedTarget.connectionId !== current.target.connectionId ||
      entry.provenance.requestedTarget.namespace.database !== current.target.namespace.database ||
      entry.provenance.requestedTarget.namespace.schema !== current.target.namespace.schema ||
      entry.provenance.requestedTarget.namespace.catalog !== current.target.namespace.catalog
    ) {
      reasons.push('targetChanged');
    }
  }
  return { valid: reasons.length === 0, reasons };
}

/** Read a temporary result, or null when any invalidation rule fires. */
export function readTemporaryResult<T>(
  entry: TemporaryResult<T> | undefined,
  current: { handle: SessionHandle; connectionId: string; configRevision: number; target: ExecutionTarget },
): T | null {
  if (!entry) return null;
  return checkTemporaryResult(entry, current).valid ? entry.value : null;
}

/** Classify a provenance record as current or stale against the live context. */
export function isProvenanceCurrent(
  provenance: ResultProvenance,
  current: { connectionId: string; configRevision: number; target: ExecutionTarget },
): boolean {
  if (provenance.connectionId !== current.connectionId) return false;
  if (provenance.configRevision !== current.configRevision) return false;
  const a = provenance.requestedTarget.namespace;
  const b = current.target.namespace;
  return a.database === b.database && a.schema === b.schema && a.catalog === b.catalog;
}
