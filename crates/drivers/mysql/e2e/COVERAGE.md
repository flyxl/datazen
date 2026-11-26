# Live MySQL driver E2E coverage

Run against a verified MySQL Community distribution, not a MariaDB server,
a shared instance, or your application database:

```sh
rustup component add llvm-tools-preview
python3 crates/drivers/mysql/e2e/coverage.py \
  --mysql-bin /path/to/mysql/bin \
  --output target/mysql-live-e2e-report
```

The runner initializes a fresh MySQL data directory, starts the server as the
calling non-root user on an ephemeral loopback TCP port, and seeds randomly
marked disposable catalogs and credentials before accepting connections.
The server is stopped and the data directory deleted on exit. Tests verify
results against independent MySQL queries or native server state.

The only counted files are production sources below `crates/drivers/mysql/src`.
The gate runs only explicitly selected integration cases, not unit tests or
other engines, and independently checks scoped LLVM JSON and LCOV counts.
A missing fixture, failed or skipped test, inconsistent profile, or unrounded
line coverage below 90% fails the gate. Use a fresh output directory each time
and a separate Cargo target directory for each Git worktree. `summary.json`
includes the exact commit, server/build versions and test verdicts;
`mysql-e2e.lcov` lists per-line hits.

`--report-only` measures an intermediate checkpoint below 90% without waiving
test failures. `--legacy-baseline --report-only` runs only the pre-existing
live contracts. The generic CM-26 diagnostic cannot verify reset-for-reuse
because the driver interface has no matching reset entry point, so it
does not count as an acceptance case.
