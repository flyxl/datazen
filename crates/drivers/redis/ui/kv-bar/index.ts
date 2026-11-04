/**
 * Redis contributions to the host's KV workspace slots (PRD §3.4).
 *
 * This module is the *only* surface `scripts/resolve-drivers.mjs` imports from
 * `ui/kv-bar/**`: the codegen emits one merged import for every slot declared
 * against this path, so anything exported here is effectively public API of the
 * driver toward the host. Internal helpers (`keyObjectInfo`, the two hooks)
 * stay un-exported on purpose.
 *
 * Props shapes are frozen in `@datazen/driver-sdk` (`KvStatusBarProps`,
 * `KeyPropsSidebarProps`) — consume them verbatim, never widen them here.
 */
export { RedisKvStatusBar } from './KvStatusBar';
export { RedisKeyPropsSidebar } from './KeyPropsSidebar';
