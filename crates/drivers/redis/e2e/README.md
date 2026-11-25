# Native Rune Redis E2E

Run both suites with a native Rune renderer and locally installed Redis
executables. Each suite creates private loopback Redis processes and isolated
connection files, replays interactions against the native UI, and checks the
server independently through `redis-cli`. No existing server or credentials are
used.

```bash
cargo build --locked -p datazen-rune-host --bin datazen-native
python3 crates/drivers/redis/e2e/rune_standalone.py target/debug/datazen-native
python3 crates/drivers/redis/e2e/rune_tools.py target/debug/datazen-native
```

The standalone suite covers incremental scanning, String/Hash previews, binary
visibility, writable vs. read-only profiles, optimistic String/Hash edits, TTL
changes, conflicts, draft navigation, and disconnect. It accepts
`--redis-server` and `--redis-cli` to override executable paths. The tools suite
accepts `--server` and `--cli`, and individual journeys can be selected with
`--only`. Both accept `--artifacts DIR` for the render trees.

Optional real-service fixtures:

- `rune_tools.py BINARY --only probe` tests a live and unreachable endpoint
  without saving either draft.
- `rune_tools.py BINARY --only form` tests how the native dialog saves a
  multi-node, read-only Sentinel profile.
- `rune_tools.py BINARY --only tls` requires OpenSSL and a TLS-capable Redis.
  It uses a disposable CA and verifies both connection and CA rejection.
- `rune_tools.py BINARY --only redis-json` requires a server with JSON commands.
  Use `--redis-json-module PATH` if the Redis build provides a separate module.
  It verifies a root edit without extending or removing the key expiry.

## Rune workbench limits

- The key browser remains visible alongside native Details, Console, Monitor
  and Pub/Sub tabs. The server overview is available as a quick action.
- Logical DBs are explicit; Cluster stays on DB 0. The native connection form
  accepts up to eight explicit Sentinel/Cluster nodes and supports a custom
  TLS CA plus paired mTLS certificate and key paths. Cluster pattern browsing
  follows the legacy single-shard scan scope; exact names route to their owners.
- Key and value scans are bounded and paged. The browser combines server-side
  type and no-expiry filters. Local tree grouping only covers loaded keys.
  Server folder mode pages through namespace levels without relying on that list. Writes through the value editor are single-line UTF-8, at most 4 KiB.
  Guarded saves require Redis scripting and the underlying command permissions;
  expiry-preserving String saves require Redis 6+. Removing a Hash field, List
  head or Set/ZSet member is guarded and cannot remove the last entry implicitly.
- The console accepts explicitly allowlisted read-only commands, individually
  or in a batch of up to eight, with Enter submit, Tab completion for
  commands or loaded keys, and in-session history. The native value viewer preserves binary previews and offers
  bounded parse-only decoding, including MessagePack. On writable profiles,
  a simple terminal SET can be reviewed and create a new String, never overwrite
  one; arbitrary write commands are not dispatched.
  Read-only profiles also reject edits, publishing, import and destructive key
  actions in the host, not just the view. The Monitor and Pub/Sub panels read
  bounded buffers on demand.
- Import reads a trusted ZIP v1 archive without extracting files or overwriting
  existing keys. Export writes the selected key or up to 128 explicitly marked
  keys in the active database. Deleting marked keys requires reviewing their
  exact names and entering the displayed count; no key pattern is sent.
- Stream tools show bounded entries, group and consumer information. Writable
  users can create a group from the first entry and acknowledge a typed pending
  ID. Group destruction is not available.

These journeys require Redis and are intentionally separate from the service-free
Rune E2E suite. Their passing results do not imply a whole-repository coverage
percentage.
