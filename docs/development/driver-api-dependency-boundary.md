# Driver API Public Dependency Boundary

`packages/driver-api` is the stable compile-time contract between DataZen and independent database driver plugins.

The API is published as the MIT-licensed `datazen-driver-api` crate. Its source of truth is the DataZen monorepo; there is no separate Driver API source repository.

## Public API rule

Public Driver API signatures and public fields may use:

- Rust primitives and standard-library types;
- types defined by `datazen-driver-api` itself;
- `serde` traits/attributes where required;
- `serde_json::Value` and other transport-neutral JSON data types.

They must not expose database implementation types.

## Forbidden public dependencies

The following must not appear in public traits, function signatures, public struct fields, enum variants, or public type aliases:

- `sqlx` types such as `Pool`, `Row`, `Transaction`, or database-specific errors;
- `tokio` runtime or synchronization types;
- database-specific crates such as `mongodb`, `redis`, `clickhouse`, `mysql_async`, `tokio-postgres`, and similar libraries;
- HTTP client implementation types such as `reqwest`;
- database pools, rows, transactions, cursors, or implementation-specific error types;
- any other third-party implementation type that an independent driver may reasonably need at a different version.

For example, this is forbidden:

```rust
pub trait DatabaseDriver {
    fn pool(&self) -> sqlx::Pool<sqlx::Postgres>;
}
```

Instead, the Driver API should expose an opaque handle:

```rust
pub struct ConnectionHandle {
    pub id: String,
    pub pool_id: String,
}
```

The driver owns and manages the real connection pool internally.

## Dependency layering

The intended dependency graph is:

```text
                         DataZen Host
                              │
                    datazen-driver-api
                              │
              ┌───────────────┼───────────────┐
              │               │               │
          Driver A         Driver B        Driver C
              │               │               │
           sqlx 0.7         sqlx 0.8        mongodb
              │               │               │
           private          private         private
```

Different drivers may use different database libraries or different versions of the same library. This is safe because implementation types never cross the API boundary.

## Foundation dependencies

`serde` and `serde_json` are allowed as transport-neutral data dependencies.

`async-trait` is currently used to express asynchronous driver traits. It is part of the Rust API implementation surface, but it is not a database implementation dependency.

`inventory` is intentionally used for compile-time driver registration. DataZen embeds drivers into the application binary rather than loading Rust shared libraries at runtime.

## Versioning

The Cargo crate version and the DataZen Driver protocol version are separate:

- **Crate version** follows Cargo/SemVer compatibility rules for the Rust API.
- **`PROTOCOL_VERSION`** represents DataZen ↔ Driver API protocol compatibility.

Internal dependency changes that do not affect public API types do not require a protocol-version change. Breaking public trait or protocol changes must be evaluated for both versions.

## Review checklist

Before merging a change to `packages/driver-api`:

- [ ] No `sqlx` type appears in public API.
- [ ] No database-specific crate type appears in public API.
- [ ] No `tokio` type appears in public API.
- [ ] Connection pools remain owned by the driver.
- [ ] Rows, cursors, and transactions use API-defined types or opaque handles.
- [ ] Cross-boundary errors use `DriverError` or another API-defined error type.
- [ ] Generic JSON data uses `serde_json` rather than a database-specific document type.
- [ ] New third-party dependencies are checked for accidental public exposure.
- [ ] Crate-version and protocol-version implications are considered.

## Development and publishing

DataZen itself consumes the crate through the workspace path dependency:

```toml
[workspace.dependencies]
datazen-driver-api = { path = "packages/driver-api" }
```

Independent plugins normally consume the published crate:

```toml
[dependencies]
datazen-driver-api = "0.1"
```

When developing an API change before publication, an independent plugin can temporarily use a local path dependency pointing at `packages/driver-api`.
