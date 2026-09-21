/**
 * Redis driver UI-owned IPC payload types.
 *
 * These mirror the Redis-crate-side serde shapes (packages/drivers/redis/src/types.rs),
 * not the generic `driver-api` KV contract. They live with the driver that
 * produces them so the host's global `src/types` stays free of driver-specific
 * value shapes.
 */

/** Parsed value detail returned by the `get_key` command. */
export interface KeyDetail {
  key: string;
  keyType: string;
  ttl: number;
  value: unknown;
}

/** Binary-safe value frame from `get_key_raw`. */
export interface ValueFrame {
  key: string;
  keyType: string;
  /** TTL in seconds: -1 = no expiry, -2 = key missing. */
  ttl: number;
  /** Logical length (STRLEN / HLEN / LLEN / SCARD / ZCARD / XLEN). */
  logicalLen: number;
  /** MEMORY USAGE bytes (null if unsupported). */
  memBytes: number | null;
  /** Base64-encoded raw bytes of the string value (null when truncated or non-string). */
  rawB64: string | null;
  /** True when the value exceeded the size budget. */
  truncated: boolean;
}
