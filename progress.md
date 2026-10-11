# P346 driver track — READY_FOR_TEST

## Implemented code facts

- PostgreSQL and MySQL InteractiveQuery/AgentSession resources retain a real physical PoolConnection until close. The descriptor claims Fixed and reserves exactly two connections (one data, one independent control). MetadataInspection remains leased with its configured pool budget.
- Connection maps hold Arc<Mutex<PoolConnection>>; registry locks are released before SQL awaits. Independent resources run independently. Cancel uses the immutable backend PID/MySQL thread id and an independent control connection.
- Acquire consumes target.database/schema. Explicit PostgreSQL schema table listing no longer includes public implicitly. Pool removal uses short write guards before asynchronous close.
- PostgreSQL drains protocol, refreshes control statistics and observes the exact backend's transaction state. Aborted transactions retain prior namespace with Partial confidence and do not query the aborted target for facts. COMMIT of an observed aborted transaction reports RolledBack.
- MySQL/MariaDB state uses server evidence when available. MySQL requires enabled thread/transaction instrumentation and consumers; otherwise arbitrary SQL gives Unknown. Successful builtin BEGIN/COMMIT/ROLLBACK supplies exact state. Active/Aborted and conservative MySQL Unknown observations register cleanup transaction handles; Idle clears them. Handles are resource/epoch scoped and rollback the original checkout.
- Fixed get_table_schema introspects on the original physical session, including temporary tables. relationSql is parsed by the driver's dialect; PostgreSQL folds only unquoted identifiers and MySQL resolves two parts as database.table. Both return data:{schema,resolvedTarget}; resolvedTarget includes actual namespace, table and sessionScoped. Quoted identifier parts containing dots remain exact in this path.
- Query/query_stream uses SQLx row streams with awaitable sink backpressure, immediate first-row delivery, at most 256 rows/1 MiB encoded row payload per batch. Statement completions contain columns and affected counts but no duplicate row buffer. Typed boundParameters uses prepared fetch_many on the same pin and requires one statement; nonparameter SQL uses raw SQL protocol.
- Sink rejection requests exact cancellation, stops further publication, and drains for up to five seconds. Control cancellation itself has a two-second bound; final protocol ping has a five-second bound. Unconfirmed drain removes the pinned checkout and data pool, closes the checkout on drop, and leaves immutable fixed identity as a tombstone so the old resource fails SessionLost rather than reconnecting.
- Catalog command output uses ResultSink.write_output({kind:'commandResult',data:...}). Execute retains existing typed boundParameters behavior.
- Existing capability/provider test expectations and mocks were adjusted; no tests were added or executed.

## Checks actually performed

Command: CARGO_TARGET_DIR=/tmp/datazen-p346-drivers-target cargo check -p datazen-driver-postgres -p datazen-driver-mysql --tests

Final log: /tmp/datazen-p346-drivers-check-commit.log

CHECK_EXIT=0

Finished `dev` profile [unoptimized + debuginfo] target(s) in 3.13s

One existing integration-test compile warning: postgres/tests/data_transfer_identity_sequences.rs:136 unused variable database. No production warning.

Before and after HEAD: e4a9eb008f6a527a5033cedfc867baa363ea5571

Before and after scoped tracked+untracked source SHA: b3f32270cfc57226756e825a9a7b1c85f01ef8e7e6f31566f1fdc499136d1926

`git diff --check` exit 0. Dedicated target uses 672 MiB; /tmp volume has 194 GiB free at delivery.

## Remaining validation / declared limitations

- WDIO and live PostgreSQL/MySQL acceptance are pending independent root验收. Compile success does not prove SQLx protocol behavior, cancellation timing, transaction cleanup, permissions or P4 writeback journeys on real servers.
- Fixed arbitrary admin/catalog commands without a same-checkout implementation are explicitly Unsupported; get_table_schema is supported. Host typed schema producers remain Host-owned and must use their resource path.
- MySQL without instrumentation cannot confirm arbitrary SQL transaction state; Unknown is intentional, builtin rollback resolves it. Such contexts cannot qualify for Confirmed writable result mapping.
- A single decoded row larger than 1 MiB is rejected. SQLx may allocate that one row before the driver rejects it; the implementation bounds published batches and avoids whole-result materialization, but does not claim a wire-level single-field allocation bound.
- Cancellation/drain failure returns SessionLost and tombstones the data handle; control pool and budget are released by resource close. Server or socket shutdown timing still needs WDIO/live verification.
- API prerequisite commits: fec6c7fe2b3b82cfe8f63b56e3de9c37b201c597, 87ee7bdcca7b00115b8a399acfdf4efd2c6e1e90, e4a9eb008f6a527a5033cedfc867baa363ea5571.
- This branch-only progress ledger must be removed during accepted integration; it is not a permanent deliverable.
