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

`--postgres-config FILE` adds three named journeys and `--mysql-config FILE`
adds two. An unconfigured engine is not included in the test counts. The
config is a private JSON file with `host` (`127.0.0.1`), `port` (integer), `database` (`datazen_e2e`), `user`, `password`, and
`client` (path to `psql` or `mysql`). Do not commit the config. Use disposable
servers only; create a `rune_e2e_fixture(marker VARCHAR(40) NOT NULL)` marker
table containing exactly one row, `datazen-rune-disposable`. The runner rejects
any other database or marker before mutating anything. Each journey creates
and removes only its own randomized tables or schema. This verifies connection,
catalog/browse, SQL result, and committed inline edit against the selected
server and compares persisted rows using an independent CLI process.
PostgreSQL verifies a non-default schema and safe refusal of same-name
relations when the UI only carries a bare table name. Explicitly selecting one
homonymous relation across namespaces remains separate work.
Both engines verify ordered columns/types/nullability, all indexes with
column order and uniqueness, and an ordered composite FK from actual server
metadata. MySQL also compares the complete rendered table DDL against
`SHOW CREATE TABLE`; PostgreSQL ordinary-table DDL is covered separately.

When both fixtures are enabled, native Data Transfer exercises insert-only
copying between PostgreSQL and MySQL in both directions. These journeys
independently verify exact decimal precision, NULL versus empty text, defaults
and sentinel rows, and the selected PostgreSQL namespace (the profile schema
or `public`). They also verify cancellation, rollback on a duplicate target key, and
preflight refusal of lossy decimal mappings or nontransactional targets. MySQL-to-PostgreSQL initially
supports only mappings that can prove their type capacity from the catalog:
signed integers, bounded DECIMAL and bounded VARCHAR.

## Migration inventory

These are behavioral mappings, not an assertion that every original test in a
legacy group was replaced. Keep legacy specs until their missing behavior and
driver-specific assertions are added.

| Legacy group | Rune coverage | Remaining examples |
| --- | --- | --- |
| Connection, settings, SQLite, grid, SQL, chart, exports, backups | Offline native smoke and focused journeys; see development E2E documentation | Connection editing/group drag, session recovery, clips, platform window operations, broader import/restore flow, per-locale validation |
| `table-dirty-cells.ts` staged changes | Three cells staged in a real SQLite grid, restoring one to its original value clears its dirty count; a batch commit lands only the two remaining edits, independently verified | Dirty state across page/navigation and per-theme visual parity |
| `export-import.ts` CSV import | Review exact CSV header mapping, then explicitly append UTF-8 rows to a real existing SQLite table; independent readback for quotes, commas, NULL vs empty, defaults and preexisting rows; cancel, malformed CSV, duplicate headers, read-only, triggers and later-key-conflict rollback | Arbitrary column remapping, additional formats/engines, schema creation and larger imports |
| `sql-query.ts` selected/caret execution | Real SQLite executes the exact selection or statement at the caret, preserves quoted UTF-8 semicolons and ignores neighboring unselected writes; rejects a multi-statement selection or incompletely quoted input before dispatch | Entire-script execution UI and dialect-exact selection for other engines |
| `sqlite.ts` query result batches | Real single SQLite statement returns all 600 rows; result find locates the unique last-row marker from the next batch | Multiple statements in one native run and retained artifact export |
| `sql-query.ts` favorites | Save the current SQL to a real host file, restart, load without executing, explicitly run and check against SQLite, rename in place, delete to trash | React-specific menu/dialog parity and per-connection favorites policies |
| `query-history.ts` | Real successful and failed SQL, reload, connection isolation, load a saved statement and explicitly rerun | Database-grouped all-scope, search, history lifecycle/delete |
| `workflow.ts`, `workflow-window.ts` | Offline editor error, YAML save/restart, actual SQLite write and independent readback | Typed step output, variables, conditional and linked steps, execution history, workflow-to-dashboard |
| `data-dashboard-sql-add.ts` | Real SQLite SQL widget creation, persisted chart and hidden workflow, restart + manual refresh after an independent database change | Multi-board import/export and scheduled refresh behavior |
| Host SQL contract and driver-specific journeys | Real PostgreSQL/MySQL browse/query/edit and catalog structure, composite foreign keys and ordered indexes; MySQL table DDL and PostgreSQL ordinary-table DDL with quoted schema/table, constraints and secondary index; duplicate bare names fail closed and unsupported identity columns produce no misleading DDL | Index create/drop UI, PostgreSQL partitions/inheritance/identity/generated columns, full dump/restore parity, complete schema tree and pool rotation, maintenance commands |
| Redis | Separate checked-in native real-server fixture and driver production line coverage | Verify any additional legacy-only behavior separately |
| Data Transfer | Native insert-only transfer into an existing table in a second real SQLite file; explicit mappings, review/cancel, read-only and file-alias refusal, NULL/empty distinction and conflict rollback; picks non-default tables, repairs a duplicate mapping, and verifies the decoy stays unchanged. Opt-in PostgreSQL ↔ MySQL verifies read-only type review before confirmation and exact values after insert | Other pairs, destructive or structure modes, recordset ranges and resume/recovery |
| Schema Diff | Read-only comparison between two real SQLite files: column changes, named CHECK definitions, and explicit simple secondary indexes (ordered columns and uniqueness); unsupported metadata is shown as unresolved and neither catalog, rows, nor file is written | Broader table/constraint semantics including unnamed CHECKs, FK and implicit UNIQUE/PK indexes, advanced index definitions, plan/apply, heterogeneous pairs |
| Data Sync | Read-only preview of two real SQLite tables with matching columns and primary keys; independent counts for inserts, updates, unchanged and kept target-only rows; checks composite text/integer keys at the 1,000-row boundary and rejects missing keys, larger tables and file aliases without changing either file; switching tools clears stale counts | Reviewed execution, larger or heterogeneous tables, filters and ranges, recovery |
| AI, wapps, packaged desktop | Not counted by this harness | Real provider/bridge checks, native dialogs/tray/window behavior, platform packages |
