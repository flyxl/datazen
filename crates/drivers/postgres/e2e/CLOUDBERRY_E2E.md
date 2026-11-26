# Apache Cloudberry live E2E

The `cloudberry` factory shares the `datazen-driver-postgres` crate, so
this gate measures the same full `crates/drivers/postgres/src` source scope.
It runs **only** tests through the Cloudberry factory against genuine Apache
Cloudberry. No PostgreSQL, QuestDB, mock or unit-test execution profiles
contribute to the result. The other instrumented binaries keep the source
denominator intact and are not run.

Use a supported non-root Linux environment and signed [Apache Cloudberry
release binaries](https://cloudberry.apache.org/releases/). On Ubuntu 24.04
amd64, the [2.2.0-incubating
release](https://github.com/apache/cloudberry/releases/tag/2.2.0-incubating)
provides a convenience tarball containing a `.deb`, its SHA-512 checksum,
and PGP signature. Check the binary package before extracting/installing
it, and ensure that the runtime libraries required by that package are
present. PostgreSQL binaries will be rejected.

```sh
rustup component add llvm-tools-preview
python3 crates/drivers/postgres/e2e/cloudberry_coverage.py \
  --cloudberry-bin /path/to/cloudberry/bin \
  --output target/cloudberry-live-e2e-report \
  --target-dir target/cloudberry-instrumented
```

Use a new output directory for each run and separate build targets across
worktrees. The driver harness creates a temporary local cluster with one
coordinator and two real segments using TCP loopback, verifies actual rows
on both segments, and stops and deletes the cluster on exit. It generates an
ephemeral password and does not connect to an existing database. No Docker,
SSH, system service, or root changes are needed. The Python runner expects
the supplied Cloudberry distribution to support the native coordinator,
segment role, and catalog bootstrap commands shipped with 2.2.

The coverage report fails below 90% by default. Use `--report-only` when
measuring an incomplete implementation; this never permits failed or
skipped tests. `summary.json` and `postgres-e2e.lcov` report the actual
coverage of the Cloudberry-only replay, separately from PostgreSQL, QuestDB,
and Rune UI results.
