import type { Counter, SessionHandle } from '@datazen/backend-client';

export function counterValue(value: Counter | string | number): bigint {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) {
    throw new Error('Unsafe counter encoding');
  }
  if (!/^\d+$/.test(String(value))) throw new Error('Invalid counter encoding');
  return BigInt(value);
}

export function sameHandle(a: SessionHandle | null, b: SessionHandle | null): boolean {
  return !!a && !!b && a.dbSessionId === b.dbSessionId && a.runtimeEpoch === b.runtimeEpoch;
}

export function wireCounter(value: bigint): Counter {
  // Rust encodes counters as decimal strings. The existing nominal facade
  // still declares numeric Counter, so preserve the wire value at this boundary.
  return value.toString() as unknown as Counter;
}
