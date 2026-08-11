# datazen-driver-api

The stable Rust API used by [DataZen](https://github.com/flyxl/datazen) database driver plugins.

The crate is maintained in the DataZen monorepo under `packages/driver-api` and is published independently to [crates.io](https://crates.io/). There is intentionally no separate source repository for the API crate.

## License

`datazen-driver-api` is licensed under the **MIT License**. The DataZen application itself is licensed separately under GPLv3.

## Usage

A third-party driver normally depends on the published crate:

```toml
[dependencies]
datazen-driver-api = "0.1"
```

Then implement the public driver traits:

```rust
use datazen_driver_api::{DatabaseDriver, DriverError};

// Implement DatabaseDriver for your driver type.
```

For local integration development, a driver can instead use a path dependency to a local DataZen checkout:

```toml
[dependencies]
datazen-driver-api = { path = "../datazen/packages/driver-api" }
```

This is useful when developing a new API version before it is published.

## API boundary

The Driver API is deliberately independent of database implementation libraries. A driver may use `sqlx`, `tokio-postgres`, `mongodb`, `redis`, or another client internally, including a different version from another driver.

Those implementation types must not appear in the public Driver API. Connection pools, rows, transactions, cursors, and database-specific errors remain private to the driver implementation. DataZen communicates through API-defined types such as `ConnectionHandle`, `QueryResult`, `Value`, and `DriverError`.

See [`docs/public-api-dependency-boundary.md`](../../docs/public-api-dependency-boundary.md) for the complete dependency-boundary policy.

## Versioning

The Cargo crate version and the Driver protocol version are separate concepts:

- **Crate version** follows Cargo/SemVer compatibility rules and controls Rust source compatibility.
- **`PROTOCOL_VERSION`** describes compatibility of the DataZen Driver API protocol and is used by the host/driver integration layer.

A breaking public API change should be evaluated for both versions. Internal dependency changes that do not affect public API types do not require a protocol-version change.

## Development in the DataZen workspace

The crate is a workspace member, so DataZen itself uses it through a local path dependency. Changes to `packages/driver-api` are therefore immediately available to the DataZen application and local driver integration builds.

Useful commands from the repository root:

```bash
cargo check -p datazen-driver-api
cargo test -p datazen-driver-api
cargo doc -p datazen-driver-api --no-deps
cargo package -p datazen-driver-api
cargo publish --dry-run -p datazen-driver-api
```

Publishing is intentionally a release operation; normal development should use the workspace path dependency.

## Independent driver development

A driver developer does not need to clone DataZen merely to obtain the API. The normal flow is:

1. Depend on `datazen-driver-api` from crates.io.
2. Develop and test the driver independently.
3. When full host/UI integration is needed, place the driver and a DataZen checkout in a local integration workspace and use the local Driver Registry path source.
4. For API changes under active development, temporarily use a path dependency to `packages/driver-api`.

The Driver itself remains an independent project; only the API source of truth lives in the DataZen monorepo.
