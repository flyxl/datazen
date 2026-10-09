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
 * The frontend type is `number` in all three cases; that is a statement about
 * what the UI works with, not about what travels. `Id` is already a string on
 * the wire, and a `Counter` is a **decimal string**, because the Rust kernel
 * counts in `u64` and `2^53 + 1` is not representable as a JS number — a
 * counter encoded as a JSON number would silently lose precision, so
 * `platform-api`'s `Counter` serializes with `collect_str`. A backend that
 * wanted to round a row count is exactly the bug this contract exists to stop.
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
 *
 * Reach it on the wire as a decimal string and narrow it with {@link toCounter}.
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

/**
 * Non-negative integer `Counter`; `undefined` for anything else.
 *
 * Accepts both wire encodings. The decimal string is the one `platform-api`
 * actually emits (`CM-01`: a `u64` serialized with `collect_str`, so that a
 * count above `2^53` survives the trip), and a bare JSON number is accepted for
 * the hand-written payloads and the mock drivers that have no `u64` to encode.
 *
 * A decimal string is refused above `Number.MAX_SAFE_INTEGER` instead of being
 * rounded. Rounding here would be the same silent precision loss `CM-01` was
 * written to prevent, one layer further from the cause; `undefined` leaves the
 * decision visible to whoever decodes it, and a `JobProgress` reading is a
 * display value, not a total that anyone reconciles against a ledger.
 */
export function toCounter(value: unknown): Counter | undefined {
  if (typeof value === 'number') {
    return Number.isInteger(value) && value >= 0 ? (value as Counter) : undefined;
  }
  if (typeof value !== 'string' || !/^\d+$/.test(value)) {
    return undefined;
  }
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) ? (parsed as Counter) : undefined;
}

/** Finite epoch-millisecond `Timestamp`; `undefined` for anything else. */
export function toTimestamp(value: unknown): Timestamp | undefined {
  return typeof value === 'number' && Number.isFinite(value) ? (value as Timestamp) : undefined;
}
