# Rune native end-to-end tests

This suite drives the production `datazen-native` binary with Rune input replay.
It checks visible application state, then opens a second database connection
where a journey claims a persisted write. The default run uses temporary files
and SQLite only; it never needs PostgreSQL, MySQL, Redis, an LLM API key, or a
running compositor. Linux needs Vulkan (Mesa lavapipe works) and
`libxkbcommon-dev`. Initialize the pinned `vendor/rune` submodule before building.

```sh
cargo build --locked -p datazen-rune-host --bin datazen-native
python3 e2e/rune/run.py target/debug/datazen-native
rustup component add llvm-tools-preview
python3 e2e/rune/coverage.py --output target/rune-e2e-report --jobs 3
```

The default coverage invocation enforces the existing 90% minimum. Add
`--report-only` to measure progress while below the target. Tests and LLVM still
must succeed. The report includes source commit, test counts, per-file and
per-crate line totals, and LCOV. Only profiles from the tested application are
merged, never from unit tests, build scripts, or the React/WebdriverIO test
runner. The denominator is LLVM-mapped production lines in
`datazen-ui`, `datazen-ui-api`, and `datazen-rune-host`. Redis has a separate
driver-only gate. Do not average the percentages or claim whole-repository
coverage.

## Opt-in real SQL servers

`--postgres-config FILE` adds two named journeys and `--mysql-config FILE`
adds one. An unconfigured engine is not included in the test counts. The config is a private JSON file with `host` (`127.0.0.1`),
`port` (integer), `database` (`datazen_e2e`), `user`, `password`, and
`client` (path to `psql` or `mysql`). Do not commit the config. Use disposable
servers only; create a `rune_e2e_fixture(marker VARCHAR(40) NOT NULL)` marker
table containing exactly one row, `datazen-rune-disposable`. The runner rejects
any other database or marker before mutating anything. Each journey creates
and removes just its own randomized table. This verifies connection,
catalog/browse, SQL result, and committed inline edit against the selected
server and compares persisted rows using an independent CLI process.
PostgreSQL verifies a non-default schema and safe refusal of same-name
relations when the UI only carries a bare table name. Explicitly selecting one
homonymous relation across namespaces remains separate work.

## Migration inventory

These are behavioral mappings, not an assertion that every original test in a
legacy group was replaced. Keep legacy specs until their missing behavior and
driver-specific assertions are added.

| Legacy group | Rune coverage | Remaining examples |
| --- | --- | --- |
| Connection, settings, SQLite, grid, SQL, chart, exports, backups | Offline native smoke and focused journeys; see development E2E documentation | Connection editing/group drag, session recovery, clips, platform window operations, complete import/restore flow, per-locale validation |
| `sql-query.ts` favorites | Save the current SQL to a real host file, restart, load without executing, explicitly run and check against SQLite, rename in place, delete to trash | React-specific menu/dialog parity and per-connection favorites policies |
| `query-history.ts` | Real successful and failed SQL, reload, connection isolation, load a saved statement and explicitly rerun | Database-grouped all-scope, search, history lifecycle/delete |
| `workflow.ts`, `workflow-window.ts` | Offline editor error, YAML save/restart, actual SQLite write and independent readback | Typed step output, variables, conditional and linked steps, execution history, workflow-to-dashboard |
| `data-dashboard-sql-add.ts` | Real SQLite SQL widget creation, persisted chart and hidden workflow, restart + manual refresh after an independent database change | Multi-board import/export and scheduled refresh behavior |
| Host SQL contract and driver-specific journeys | Opt-in PostgreSQL/MySQL browse/query/edit slice | Both-engine matrix for full contract, indexes/DDL, schema tree and pool rotation, maintenance commands |
| Redis | Separate checked-in native real-server fixture and driver production line coverage | Verify any additional legacy-only behavior separately |
| Migrations and transfer | Existing UI activation/validation paths are not proof of transfer | Real source/target readback, diff/apply, heterogeneous transfers, synchronization, recovery |
| AI, wapps, packaged desktop | Not counted by this harness | Real provider/bridge checks, native dialogs/tray/window behavior, platform packages |
