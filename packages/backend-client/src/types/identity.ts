/**
 * Base scalar mirrors shared by every DataZen backend contract.
 *
 * These are the TypeScript side of the Rust foundation types the platform
 * kernel is specified against (see
 * `docs/architecture/platform/shared-boundaries-and-ports.md` §2.1 and
 * `docs/architecture/platform/system-overview.md` §6.2). They are declared
 * *nominal* on purpose: `Id`, `Counter` and `Timestamp` are three different
 * things in the wire protocol and a frontend that swaps a revision counter for
 * a session id must fail to compile, not silently ship.
 *
 * The wire encoding is deliberately the simplest one that round-trips:
 * ids are opaque strings, counters and timestamps are numbers. JSON carries
 * them without any transform, so an adapter never has to know about it.
 */

/** Brand of a nominal type. Never instantiated; it only marks intent. */
declare const datazenBrand: unique symbol;

/** Opaque, backend-issued identifier. */
export type Id = string & { readonly [datazenBrand]: 'Id' };

/**
 * Monotonic counter: revisions, sequence numbers, chunk indexes.
 *
 * Never used for anything that is not strictly ordered — in particular a
 * `Counter` is not a timestamp and not an id.
 */
export type Counter = number & { readonly [datazenBrand]: 'Counter' };

/**
 * Wall-clock instant, epoch milliseconds since the Unix epoch.
 *
 * The kernel may emit an ISO-8601 string on the wire for readability; adapters
 * normalize on the way in, so frontend code only ever sees this.
 */
export type Timestamp = number & { readonly [datazenBrand]: 'Timestamp' };

/**
 * Narrowing helper for values decoded from untyped payloads.
 *
 * Returns `undefined` rather than throwing: decoding a whole envelope must be
 * able to fail on its own terms, and a malformed id should surface as a
 * contract violation at the boundary, not as a cast that launders it.
 */
export function toId(value: unknown): Id | undefined {
  return typeof value === 'string' && value.length > 0 ? (value as Id) : undefined;
}

/** Non-negative integer `Counter`; `undefined` for anything else. */
export function toCounter(value: unknown): Counter | undefined {
  return typeof value === 'number' && Number.isInteger(value) && value >= 0
    ? (value as Counter)
    : undefined;
}

/** Finite epoch-millisecond `Timestamp`; `undefined` for anything else. */
export function toTimestamp(value: unknown): Timestamp | undefined {
  return typeof value === 'number' && Number.isFinite(value)
    ? (value as Timestamp)
    : undefined;
}