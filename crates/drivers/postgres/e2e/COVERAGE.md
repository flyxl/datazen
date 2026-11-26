# PostgreSQL driver live E2E coverage

This gate measures line coverage in `crates/drivers/postgres/src` against
a **new, disposable PostgreSQL server**. It does not use a DSN, a saved
connection, ambient PostgreSQL credentials, a mock, driver unit tests, or
other engines' profiles. The runner requires a directory with compatible
`initdb`, `pg_ctl` and `psql` and a non-root user. It binds to IPv4 loopback
on a temporary port, seeds marked test databases, and stops and removes
the cluster when it exits normally. Run only from a trusted checkout.

```sh
rustup component add llvm-tools-preview
python3 crates/drivers/postgres/e2e/coverage.py \
  --pg-bin "$(pg_config --bindir)" \
  --output target/postgres-e2e-report
```

Choose a new output directory each time. Keep the Cargo target directory
separate for each checkout so cached coverage paths identify its own sources.
The runner builds all eligible
integration test targets in one locked Cargo invocation, stages exact
instrumented binaries, and runs only named live tests. It treats failures,
skips, missing profiles, or inconsistent LLVM reports as failures. It uses
the unrounded threshold of **at least 90%** and records both numerator and
denominator in `summary.json`; a matching repository-relative line report
is in `postgres-e2e.lcov`. Build/listing and unit profiles are excluded.
The report also records source commit, instrumented binary hashes,
PostgreSQL and Rust versions, and counts of passed, failed and skipped cases.

Use `--report-only` while measuring progress below 90%; it never waives
real test failures or skips. `--legacy-baseline --report-only` omits the
new driver journeys and is not an acceptance run. The separate Rune
native UI and Redis E2E gates have different source scopes.
