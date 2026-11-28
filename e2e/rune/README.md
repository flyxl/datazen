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

`--postgres-config FILE` adds PostgreSQL-only journeys and
`--mysql-config FILE` adds MySQL-only journeys. Passing both also enables
cross-engine journeys. Unconfigured engines are not counted. Use
`run.py --list-tests` with the same fixture options to see the exact plan. The
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
or `public`). They also verify cancellation, duplicate-key rollback, a
deferred constraint that fails at COMMIT, and refusal of lossy decimal
mappings or nontransactional targets before writes. MySQL-to-PostgreSQL
supports only mappings that can prove their type capacity from the catalog:
signed integers, bounded DECIMAL and bounded VARCHAR.

## SQL formatter: real Native editor

The formatter has its own narrow, opt-in acceptance suite. Drive the stock Native
binary with isolated settings, using the same disposable SQLite or marked
loopback TCP fixtures as above. Use separate real PostgreSQL, Oracle MySQL
Community and MariaDB servers; a MariaDB server is not a MySQL test. A
SELECT-only fixture account suffices. The Native credential-import CLI is not
a UI replay and its LLVM profile is excluded.

```sh
BIN=target/debug/datazen-native
for mode in "" --definitions --statements --size-bound; do
  python3 e2e/rune/sql_formatter.py "$BIN" $mode || exit 1
done
python3 e2e/rune/sql_formatter.py "$BIN" --dialect sqlite
python3 e2e/rune/sql_formatter.py "$BIN" --dialect postgresql --fixture "$PG_FIXTURE"
python3 e2e/rune/sql_formatter.py "$BIN" --dialect mysql --fixture "$MYSQL_FIXTURE"
python3 e2e/rune/sql_formatter.py "$BIN" --dialect mariadb --fixture "$MARIA_FIXTURE"
```

The expected transformations in `sql-formatter-dialects-15.8.2.json` were
verified with the pinned 15.8.2 CJS and original browser distributions. No
JavaScript is loaded by these journeys or the Native app. Each test opens a
real session, pastes its draft, clicks Format, compares every rendered document
line, and checks one physical Undo in a replay of the same route. It checks no
query result/history, no settings change, and an unchanged SQLite file or TCP
fixture marker. These cases cover neither every upstream statement nor a
stock SQL Server session.

### Formatter-only line coverage on Linux
Use the Rust toolchain which built the binary (including `llvm-tools-preview`).
The ordinary host coverage runner at the top of this page measures **different
crates**. For the formatter gate, the wrapper below instruments only the
production formatter library; the final Native executable only links the Rust
profiling runtime. Use a new Cargo target or an exact, previously verified
instrumented binary. Do not set global instrumentation in `RUSTFLAGS`.

```bash
export CARGO_TARGET_DIR="$PWD/target/formatter-ui-cov"
export RUSTC_WORKSPACE_WRAPPER="$PWD/e2e/rune/formatter_coverage_rustc.sh"
mkdir -p "$CARGO_TARGET_DIR"
export DZ_FORMATTER_PROFILE_RUSTC_LOG="$CARGO_TARGET_DIR/compiler-events.tsv"
profiler=("$(rustc --print target-libdir)"/libprofiler_builtins-*.rlib)
test "${#profiler[@]}" = 1 && test -f "${profiler[0]}" || exit 1
export DZ_FORMATTER_PROFILE_RLIB="${profiler[0]}"
cargo build --locked -p datazen-rune-host --bin datazen-native --message-format=json \
  > "$CARGO_TARGET_DIR/native-build.jsonl" || exit 1
# Read the actual dependency artifact from this successful Cargo invocation.
rlib=$(python3 - "$CARGO_TARGET_DIR/native-build.jsonl" <<'PYTHON'
import json, sys
rows = (json.loads(line) for line in open(sys.argv[1]))
[rlib] = [f for row in rows if row.get("reason") == "compiler-artifact"
          and row["target"]["name"].replace("-", "_") == "datazen_sql_formatter"
          for f in row["filenames"] if f.endswith(".rlib")]
print(rlib)
PYTHON
) || exit 1
python3 e2e/rune/formatter_coverage.py "$CARGO_TARGET_DIR/debug/datazen-native" \
  --formatter-rlib "$rlib" --reference "$REFERENCE" \
  --output target/formatter-e2e-report \
  --postgresql-fixture "$PG_FIXTURE" --mysql-fixture "$MYSQL_FIXTURE" \
  --mariadb-fixture "$MARIA_FIXTURE" --mysql-log "$MYSQL_GENERAL_LOG" \
  --mariadb-log "$MARIA_GENERAL_LOG"
```

The three marked disposable servers must be running throughout the gate.
`REFERENCE` is the installed project-locked sql-formatter 15.8.2 package directory,
used only as an outside oracle. For the MySQL-family refusal checks, enable their general query logs and
provide read-only paths to those logs, not credentials or logs from a real
service. These checks compare only the fresh log suffix to verify a positive
metadata read and absence of the authored SQL. The output directory must
**not exist**. The gate runs the mounted suites, requires every unittest group
to pass with no skips and one fresh nonempty profile per captured tree,
checks that the binary did not change and that **all** LLVM mappings belong to
the formatter's exact local production sources. It writes the per-file report,
line counts, annotated source, and exact binary digest. The requested 99%
threshold applies **only to the combined, deduplicated result** on all mapped
production lines. It is not a 99% threshold for stock Native alone. The gate
builds an external public API test consumer and an isolated three-test T-SQL
unit binary against the same instrumented rlib. No general-purpose unit suite
or T-SQL E2E is included. It reports Native-only, public-process-only, E2E union,
T-SQL-unit-only, and final union; validates the exact function/region mapping and
counter sums; and hashes the input binaries and each counted raw profile.
The mixed total is not an E2E-only percentage. Use `--native-only` without
`--formatter-rlib` and `--reference` for the Native diagnostic, or
`--report-only` to report a below-threshold mixed measurement without accepting
a failed case. See [public consumer acceptance](../../crates/datazen-sql-formatter/e2e/README.md).
`llvm-cov`'s file-summary lines and the distinct physical line keys in exported
LCOV `DA` records are different metrics; never add their counts.

## Opt-in real Redis UI

Pass both `--redis-server PATH` and `--redis-cli PATH` to either native
runner to include eight UI journeys using private loopback Redis instances.
They check archive/lifecycle and key inspection, collection removal,
Stream groups, Pub/Sub, console completion, and server-side key filters and
folders. These are UI replays in the same ordinary native binary, not runs
of the separate driver-contract or Redis crate coverage gate. The combined
report records enabled engines so compare like-for-like runs.

## Migration inventory

These are behavioral mappings, not an assertion that every original test in a
legacy group was replaced. Keep legacy specs until their missing behavior and
driver-specific assertions are added.

| Legacy group | Rune coverage | Remaining examples |
| --- | --- | --- |
| Connection, settings, SQLite, grid, SQL, chart, exports, backups | Offline native smoke and focused journeys. The shared settings navigation and appearance selector verify actual light/dark persistence, unsupported options stay hidden, and a failed write preserves the previous appearance. English and Simplified Chinese are verified through actual translated labels and persisted settings, including write-failure and unsupported-locale fallback. See development E2E documentation for other smoke cases | Connection editing/group drag, session recovery, clips, platform window operations, broader import/restore flow, external language packs and broader per-locale validation |
| `connection-empty-state.ts` EMPTY-001–004 | With a real saved SQLite profile, the inactive landing only shows connection actions. New and Import open actual native dialogs, cancel and Escape return without saving, and the New Query action appears only after connecting and yields the independently checked SQLite row | Native toolbar replaces the React-specific quick action; the import file chooser itself is not exercised |
| `connection-validation.ts` TC-CONN-005–007 | Validate missing address before IO, refuse invalid port, report unreachable reserved loopback endpoint, and verify real MySQL rejects an incorrect password and accepts the valid one. No draft persisted and neither password is rendered | Advanced SSL and group settings, and empty-password parity |
| `connection-search-group.ts` name and address filters | Match a saved host substring case-insensitively without connecting; reject unrelated hosts and profiles without hosts, then clear to restore all rows. Also verifies name/no-match filtering and group expand/collapse | Named-group context menus and other interactions |
| `new-connection.ts` default driver and engine draft switching | Native dialog defaults to linked PostgreSQL and uses real factory ports for PostgreSQL/MySQL/MariaDB. Per-engine names and addresses survive switching; cancelling/reopening resets the form without persisting it; SQLite uses the linked driver's file-location metadata: only a file path is required and the saved file connection opens after relaunch | Advanced fields, file chooser and SQL connection-test validation |
| `er-diagram.ts` ER-002/003/005/006/007/013 and ER-010 node layout | Open a real SQLite catalog, verify columns and non-overlapping boxes, count one declared FK for a composite key, select a table, filter and clear visible nodes, and restore layout after a real drag, and zoom actual rendered nodes without scaling the toolbar | Graphically rendered relationship edges, prediction, hover and tab container parity |
| `table-cell-selection.ts` TC-SEL-001–006 | Real SQLite cell clicks, Shift-click and row gutters choose exact visible cells; dragging paints precisely the rectangle. Escape during a drag and scrolling its virtualized origin out of view restore the previous logical selection; Escape after a click and paging clear selection. Independent SQLite reads check values and unchanged rows | Dragging across pages and autoscroll are not supported by this gesture |
| `table-data.ts` TC-TABLE-009 | Enter a real empty SQLite table, verify it retains column headers and exactly zero records instead of showing the app's missing-workspace/error state; repeat after a genuine INSERT and DELETE, with independent SQLite row and schema readback | Empty query results have separate tests |
| `table-edit.ts` DE-002, DE-003b, DE-005–008 | Double-click the real SQLite grid editor, assert the bound value and typed draft, and cancel with no staged/persisted write. Verify an integer UPDATE after reopen. Preview the live bound SQL and separate quoted parameter without writing, dismiss and roll back, then explicitly confirm a separately reviewed batch and verify the exact row after reopen | Additional editing modes and cross-driver preview parity |
| `table-dirty-cells.ts` staged changes | Three cells staged in a real SQLite grid, restoring one to its original value clears its dirty count; a batch commit lands only the two remaining edits. A trigger rejection rolls back the entire batch (including audit writes), keeps staged edits until discard or correction, and keyless rows cannot be committed | Dirty state across page/navigation and per-theme visual parity |
| `export-import.ts` CSV import | Review exact CSV header mapping, then explicitly append UTF-8 rows to a real existing SQLite table; independent readback for quotes, commas, NULL vs empty, defaults and preexisting rows; cancel, malformed CSV, duplicate headers, read-only, triggers and later-key-conflict rollback | Arbitrary column remapping, additional formats/engines, schema creation and larger imports |
| `sql-query.ts` selected/caret execution | Real SQLite executes the exact selection or statement at the caret, preserves quoted UTF-8 semicolons and ignores neighboring unselected writes; rejects a multi-statement selection or incompletely quoted input before dispatch | Dialect-exact selected/caret behavior for other engines |
| `sqlite.ts` view catalog and browsing | A distinct Views category in the real SQLite catalog opens, collapses, and filters; opening a view checks its projected rows against an independent SQLite query | Legacy view metadata and other engines need separate verification |
| `sql-query.ts` SQ-002/SQ-004 and streaming batches | A real delayed PostgreSQL SELECT produces two named columns and 600 rows (including a unique independently read marker found at row 600) and displays the measured query duration | Duration for an entire multi-statement script |
| `sql-query.ts` SQ-002 error branch | Real PostgreSQL rejects a statement targeting a missing table; an accessible per-result alert shows a classified reason, copy feedback appears and expires, and explicitly running corrected SQL in the same editor produces the independently checked result | Granular server diagnostic text, AI explain/fix, and retry of an unchanged query |
| `sql-query.ts` SQ-001 Stop | A real PostgreSQL observer sees the active keyed long-running statement and verifies it ends after the native Stop button, before window teardown and the natural deadline. The same editor runs a fresh query; the keyboard hint is rendered | Other engines and runtime lifecycle switching |
| `sql-query.ts` SQ-EXPLAIN-001 | Native PostgreSQL Explain renders a real plan on the selected database for an UPDATE on a relation unique to that database. The test verifies the operation and plan node with independent PostgreSQL EXPLAIN and confirms the rows did not change. Closing the plan leaves the prior grid intact. MySQL independently verifies its classic plan for an UPDATE and unchanged rows. | Plan-only SQL, not EXPLAIN ANALYZE; PostgreSQL case requires the alternate marker |
| `sql-query.ts` SQ-CTX-001/002 | With an optional `alternateDatabase` of `datazen_e2e_alt`, both actual PostgreSQL databases must contain the disposable fixture marker. The native editor selection changes the actual database and sentinel rows returned; qualified catalog SQL does not reset it. A pinned profile has no selector | Only profiles without a pinned database show the picker |
| `sql-query.ts` SQ-LIFECYCLE-001 editor isolation extension | Two editors on the same connection execute against distinct verified databases. Switching back restores the original choice; opening a new editor after closing the first starts at the default | Connection switch and non-PostgreSQL aliases are covered separately |
| `sql-query.ts` SQ-LIFECYCLE-001 | Six real context switches in one Rune editor prove their actual PostgreSQL target from the server and show that each execution-owned physical backend disappears while the editor remains open | Rune uses execution-scoped sessions rather than mirroring the legacy editor's resource budget |
| `sql-query.ts` SQ-013/014 | Against real PostgreSQL, Run and Command+Enter execute exactly the selected fragment containing a UTF-8 quoted semicolon. Neither an unselected UNION sibling nor an adjacent write executes; the server is independently checked | Other PostgreSQL syntax and aliases |
| `sql-query.ts` SQ-021 | Three real SQLite updates run independently but yield one history card; a different intervening statement prevents a later repeat from replacing the earlier card | Legacy history deduplication is scoped to a connection, database and schema |
| `sql-query.ts` SQ-012 | Actual PostgreSQL UPDATE result shows server-reported two affected rows, zero for a non-match, and measured duration. A separate database client verifies changed and untouched rows; an empty SELECT retains a column and does not display an affected count | DML with RETURNING and other driver parity are not claimed |
| `sql-query.ts` SQ-011 | PostgreSQL SELECT scripts expose separate ordered result tabs, retain named columns for a zero-row result, keep a dollar-quoted semicolon inside its result, and reject SELECT INTO before dispatch | Mixed scripts, arbitrary SQL and other PostgreSQL-family drivers |
| `sqlite.ts` multiple statements | Explicit SQLite SELECT script publishes separate tabs for each result, with independent counts/headers before and after switching; a mixed SELECT/delete script is rejected before execution | Mixed SQL scripts and SQLite zero-row column metadata |
| `query-row-limit-journey.ts` QLIMIT-003/004 | With a user-configured cap, an ordinary native SQLite or PostgreSQL SELECT shows the exact ordered prefix; the warning appears only if the driver had an extra row (including a true PostgreSQL WITH TIES result). Neither driver rewrites authored SQL for this publication budget. PostgreSQL checks LIMIT/OFFSET/FETCH; SQLite checks a tighter authored LIMIT and adjacent compound SELECT/VALUES results | The default and explicit on/off control are separate legacy cases; this is a result-publication limit, not a server work deadline |
| `sqlite.ts` query result batches | Real single SQLite statement returns all 600 rows; result find locates the unique last-row marker from the next batch | Retained artifact export |
| `sql-query.ts` SQ-005 and SQ-015/017/018 section visibility | Real SQLite query remains visible while the Rune query library opens Favorites, leaves it for Snippets, and reopens History with the exact executed SQL | Native library tabs replace the original toggle controls; Tauri menu events are not part of this replay |
| `sql-query.ts` favorites | Save the current SQL to a real host file, restart, load without executing, explicitly run and check against SQLite, rename in place, delete to trash | React-specific menu/dialog parity and per-connection favorites policies |
| `query-history.ts` QH-001–005 | Real PostgreSQL queries verify current-database default, database grouping in All, case-insensitive search/no-match and returning to Current. SQLite covers success/failure, reload, connection isolation, loading without running and explicit rerun | History lifecycle/delete |
| `unified-tab-bar.ts` UTB-001–008 | Real SQLite table and query results in separate tabs, switching, closing and returning to the empty workspace. Real PostgreSQL observers check that confirming a running tab close stops the exact statement before its deadline and that declining leaves it running | Remaining cross-engine stop and packaged-window behavior |
| `workflow.ts`, `workflow-window.ts` | Offline editor error, YAML save/restart, actual SQLite writes and independent readback; a failing step stops later writes and leaves the earlier committed step intact | Typed step output, variables, conditional and linked steps, execution history, workflow-to-dashboard |
| `workflow.ts` SW-01 save/list/delete slice | Save two actual native workflow records, check the target steps, cancel and confirm deletion independently, and restart with the unrelated definition intact | Legacy variable definitions and stable ID exposure are not asserted by this native list |
| `workflow.ts` SW-03 and workflow-window history | A real SQLite workflow stores its report. After restart the history tab shows the old real result despite a later DB change, reuses an already open detail, and Cancel and confirmed Clear are checked against local storage | History is local; no operator parameter map is persisted |
| `workflow.ts` SW-04b/c row-field references | A native YAML workflow reads a real SQLite row and passes its Unicode text and integer through `steps.s1.rows[0].field` to a second query. An independent read checks the exact downstream row and preserved source and decoy | The connected SQL task still does not cover variable entry, conditional steps and workflow history |
| `workflow.ts` SW-04 runtime parameters | The native registry supplies declared values and required/default metadata. Enter a Unicode string and genuine integer in one real SQLite workflow tab, run another with its distinct own defaults, switch back, and independently verify both rows and an unchanged decoy. Missing required input is refused before data changes | Visual authoring and full typed definition round-trip are separate |
| `workflow.ts` SW-05 / SW-06 | A registry-loaded condition reads live SQLite and alternately executes each real branch, not the unselected branch. Invalid SQL is rendered with its concrete failed step and error, with only prior commits and no later write | Additional condition expression and error-policy variants are not claimed |
| `workflow-window.ts` run tab and step navigation | Execute a persisted two-query SQLite workflow, switch stable step IDs, compare independently read typed result cells and the separately rendered SQL, and close the run tab back to the empty state without deleting the workflow | Native result table is not the React virtualized grid; execution history and per-step parameters are separate |
| `data-dashboard-sql-add.ts` | Real SQLite SQL widget creation, persisted chart and hidden workflow, restart + manual refresh after an independent database change | Multi-board import/export and scheduled refresh behavior |
| Host SQL contract and driver-specific journeys | Real PostgreSQL/MySQL browse/query/edit and catalog structure, composite foreign keys and ordered indexes; MySQL table DDL and PostgreSQL ordinary-table DDL with quoted schema/table, constraints and secondary index; duplicate bare names fail closed and unsupported identity columns produce no misleading DDL | Index create/drop UI, PostgreSQL partitions/inheritance/identity/generated columns, full dump/restore parity, complete schema tree and pool rotation, maintenance commands |
| Redis | Separate checked-in native real-server fixture and driver production line coverage | Verify any additional legacy-only behavior separately |
| Data Transfer | Native insert-only transfer into an existing table in a second real SQLite file; explicit mappings, review/cancel, read-only and file-alias refusal, NULL/empty distinction and conflict rollback; picks non-default tables, repairs a duplicate mapping, and verifies the decoy stays unchanged. The catalog-backed column editor verifies exact-name Auto Match, manual mapping, rejection of duplicate targets, and preservation of defaults for unmapped target columns. Opt-in PostgreSQL ↔ MySQL verifies read-only type review before confirmation and exact values after insert | Other pairs, destructive or structure modes, recordset ranges and resume/recovery |
| Schema Diff | Read-only comparison between two real SQLite files: column changes, named CHECK definitions, and explicit simple secondary indexes (ordered columns and uniqueness); unsupported metadata is shown as unresolved and neither catalog, rows, nor file is written | Broader table/constraint semantics including unnamed CHECKs, FK and implicit UNIQUE/PK indexes, advanced index definitions, plan/apply, heterogeneous pairs |
| Data Sync | Read-only preview of two real SQLite tables with matching columns and primary keys; independent counts for inserts, updates, unchanged and kept target-only rows; checks composite text/integer keys at the 1,000-row boundary and rejects missing keys, larger tables and file aliases without changing either file; switching tools clears stale counts | Reviewed execution, larger or heterogeneous tables, filters and ranges, recovery |
| AI, wapps, packaged desktop | Not counted by this harness | Real provider/bridge checks, native dialogs/tray/window behavior, platform packages |
