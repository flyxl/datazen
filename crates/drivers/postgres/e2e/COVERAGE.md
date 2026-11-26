# PostgreSQL driver live E2E coverage

This gate measures line coverage in `crates/drivers/postgres/src` against
a **new, disposable PostgreSQL server**. It does not use a DSN, a saved
connection, ambient PostgreSQL credentials, a mock, driver unit tests, or
other engines' profiles. The runner requires a directory with compatible
`initdb`, `pg_ctl` and `psql` from a TLS-enabled build, OpenSSL, and a non-root
user. It mints a temporary CA for real TLS coverage and binds to IPv4 loopback
on a temporary port, seeds marked test databases, and stops and removes
the cluster when it exits normally. Run only from a trusted checkout.

```sh
rustup component add llvm-tools-preview
python3 crates/drivers/postgres/e2e/coverage.py \
  --pg-bin "$(pg_config --bindir)" \
  --output target/postgres-e2e-report
```

Choose a new output directory each time and do not share `--target-dir`
across different checkouts; LLVM records the source paths in binaries. Keep the Cargo target directory
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

## PG-wire aliases

QuestDB and Apache Cloudberry are aliases registered in this same crate, not
standalone crates with their own production line denominators. Test each product
with its own server. The PostgreSQL report is not evidence that an alias can
execute a PostgreSQL-only feature.

### QuestDB

Use the [official Linux QuestDB runtime](https://questdb.com/docs/getting-started/quick-start/)
(verified with 10.2.0):

```sh
python3 crates/drivers/postgres/e2e/questdb_coverage.py \
  --questdb-script /path/to/questdb/bin/questdb.sh \
  --output target/questdb-e2e-report
```

The runner creates a fresh data directory and random PGWire credentials, starts
QuestDB HTTP and PGWire on loopback, verifies the native `build()` response and
a fresh fixture marker, and records only QuestDB integration profiles. The
PostgreSQL test binaries are linked into the coverage analysis to retain the
full shared source denominator, but none is executed. The gate defaults to a
strict 90% threshold. Use `--report-only` to measure progress below it. This
does not waive a failed or skipped live case.

QuestDB's PGWire compatibility does not provide all PostgreSQL administration
or session features. For example, QuestDB 10.2.0 does not expose
`pg_backend_pid()` or `pg_catalog.pg_stat_activity`, which the shared retained
resource path currently uses. Those paths are not credited by these tests.
