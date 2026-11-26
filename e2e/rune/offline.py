"""Rune migrations of legacy settings, grid, catalog and local file journeys."""
import csv
import json
import sqlite3

from scenarios import fixture, query_steps, tap, type_text, visible

OPEN = (tap(.5, "conn-open-e2e-sqlite"), tap(3, "tree-table-customers"))
SETTINGS = tap(.5, "rail-settings")
DATA_SETTINGS = tap(1.4, "settings-nav-dataBrowsing")


def key(t, name, *, cmd=False):
    return {"t": t, "type": "key", "name": name, "cmd": cmd}


def fill(t, element, value):
    return [tap(t, element), key(t + .25, "a", cmd=True),
            type_text(t + .45, value)]


class OfflineJourneys:
    """Mix into the native replay harness; no externally hosted service."""

    def test_settings_theme_persists_after_relaunch(self):
        appearance = tap(1.4, "settings-nav-appearance")
        tree = self.replay([SETTINGS, appearance, tap(2.3, "settings-theme")], until=3.5)
        self.expect_rendered(tree, text="Theme: Light", within="settings-theme")
        settings = json.loads((self.root / "settings.json").read_text())
        self.assertIs(settings["ui"]["themeDark"], False)
        self.expect_rendered(self.replay([SETTINGS, appearance], until=2.4),
                             text="Theme: Light", within="settings-theme")

    def test_settings_page_size_and_query_limit_survive_restart(self):
        tree = self.replay([
            SETTINGS, DATA_SETTINGS,
            *fill(2.2, "settings-page-size-input", "17"), tap(3.3, "settings-page-size-set"),
            *fill(4, "settings-max-rows-input", "23"), tap(5.1, "settings-max-rows-set"),
        ], until=6.2)
        self.assertFalse(visible(tree, key="settings-error"))
        ui = json.loads((self.root / "settings.json").read_text())["ui"]
        self.assertEqual((ui["tablePageSize"], ui["queryMaxRows"]), (17, 23))
        tree = self.replay([SETTINGS, DATA_SETTINGS], until=2.5)
        self.expect_rendered(tree, text="Rows per page: 17", within="settings-data")
        self.expect_rendered(tree, text="Query max rows: 23", within="settings-data")

    def test_settings_invalid_page_size_is_not_saved(self):
        tree = self.replay([
            SETTINGS, DATA_SETTINGS, *fill(2.2, "settings-page-size-input", "abc"),
            tap(3.3, "settings-page-size-set"),
        ], until=4.5)
        self.expect_rendered(tree, text="table page size must be a number", within="settings-error")
        self.assertFalse((self.root / "settings.json").exists())

    def test_filter_grid_matches_sqlite_predicate_and_can_be_cleared(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT customer_name FROM customers WHERE customer_name LIKE ?",
                                  ("%lin%",)).fetchone()[0]
        filtered = [*OPEN, tap(4.5, "filter-column"), *fill(5, "filter-input", "lin"),
                    key(6.1, "Enter")]
        tree = self.replay(filtered, until=7.8)
        self.expect_rendered(tree, text=expected, within="data-grid")
        self.expect_rendered(tree, text="1 rows", within="grid-filter")
        self.assertFalse(visible(tree, text="fixture-ada", within="data-grid"))

        tree = self.replay([*filtered, tap(8, "filter-input"), key(8.35, "a", cmd=True),
                            key(8.6, "Backspace"), key(9, "Enter")], until=10.5)
        self.expect_rendered(tree, text="fixture-ada", within="data-grid")
        self.expect_rendered(tree, text=expected, within="data-grid")
        self.expect_rendered(tree, text="2 rows", within="grid-filter")

    def test_grid_header_cycles_sort_order(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            asc, desc = [row[0] for row in db.execute(
                "SELECT customer_name FROM customers ORDER BY customer_name")]
        actions = list(OPEN)
        for count, (direction, first) in enumerate(((" ↑", asc), (" ↓", desc), ("", asc)), 1):
            with self.subTest(direction=direction or "unsorted"):
                at = 3.8 + count * 1.35
                actions.append(tap(at, "head-1"))
                tree = self.replay(actions, until=at + 1.2)
                self.expect_rendered(tree, text=f"customer_name{direction}", within="head-1")
                self.expect_rendered(tree, text=first, within="cell-0-1")

    def test_grid_next_and_previous_page_use_saved_page_size(self):
        fixture(self.store, self.db)
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"tablePageSize": 1}}), encoding="utf-8")
        nxt = [*OPEN, tap(5, "page-next")]
        tree = self.replay(nxt, until=6.8)
        self.expect_rendered(tree, key="grid-page-label", text="Page 2 / 2")
        self.expect_rendered(tree, text="fixture-lin", within="cell-0-1")
        self.assertFalse(visible(tree, text="fixture-ada", within="data-grid"))
        tree = self.replay([*nxt, tap(7, "page-prev")], until=8.8)
        self.expect_rendered(tree, key="grid-page-label", text="Page 1 / 2")
        self.expect_rendered(tree, text="fixture-ada", within="cell-0-1")

    def test_sqlite_columns_and_indexes_match_catalog(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE INDEX idx_e2e_customer_name ON customers(customer_name)")
            column = db.execute("PRAGMA table_info(customers)").fetchall()[1][1]
            index = db.execute("PRAGMA index_list(customers)").fetchone()[1]
        for view, within, expected in (("columns", "columns-list", column),
                                       ("indexes", "indexes-list", index)):
            with self.subTest(view=view):
                tree = self.replay([*OPEN, tap(5, f"view-{view}")], until=6.8)
                self.expect_rendered(tree, text=expected, within=within)

    def test_sqlite_foreign_key_matches_catalog(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER "
                       "REFERENCES customers(id))")
            target = db.execute("PRAGMA foreign_key_list(orders)").fetchone()[2]
        tree = self.replay([OPEN[0], tap(3, "tree-table-orders"), tap(5, "view-fks")], until=6.8)
        self.expect_rendered(tree, text=f"{target} (id)", within="fks-list")
        self.expect_rendered(tree, text="customer_id", within="fks-list")

    def test_sqlite_query_view_returns_real_rows(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE VIEW e2e_customer_view AS SELECT customer_name FROM customers WHERE id=2")
            expected = db.execute("SELECT * FROM e2e_customer_view").fetchone()[0]
        tree = self.replay(query_steps("SELECT * FROM e2e_customer_view;"), until=11)
        self.expect_rendered(tree, text=expected, within="result-workspace")
        self.assertFalse(visible(tree, text="fixture-ada", within="result-workspace"))

    def test_csv_export_matches_sqlite_and_cancel_writes_nothing(self):
        fixture(self.store, self.db)
        destination = self.root / "customers.csv"
        actions = [*OPEN, tap(4.4, "rail-settings"), tap(5.2, "settings-nav-dataBrowsing"),
                   tap(6.1, "export-start")]
        tree = self.replay([*actions, {"t": 7, "type": "file-dialog-cancel"}], until=8.4)
        self.expect_rendered(tree, text="the export was cancelled; nothing was written",
                             within="export-error")
        self.assertFalse(destination.exists())
        self.assertFalse(list(self.root.rglob("*.csv")))
        self.replay([*actions, {"t": 7, "type": "file-dialog", "path": str(destination)}], until=10)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT CAST(id AS TEXT), customer_name FROM customers").fetchall()
        with destination.open(newline="", encoding="utf-8") as stream:
            rows = list(csv.reader(stream))
        self.assertEqual(rows[0], ["id", "customer_name"])
        self.assertCountEqual([tuple(row) for row in rows[1:]], expected)

    def test_json_export_preserves_sqlite_types_and_escaped_text(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("INSERT INTO customers VALUES (?, ?)", (3, 'line 1\nline 2, "quoted"'))
            expected = [dict(zip(("id", "customer_name"), row))
                        for row in db.execute("SELECT id, customer_name FROM customers")]
        destination = self.root / "customers.json"
        self.replay([
            *OPEN, tap(4.4, "rail-settings"), tap(5.2, "settings-nav-dataBrowsing"),
            tap(6, "export-format"), tap(6.7, "export-start"),
            {"t": 7.4, "type": "file-dialog", "path": str(destination)},
        ], until=10.5)
        self.assertCountEqual(json.loads(destination.read_text(encoding="utf-8")), expected)

    def test_sqlite_backup_restores_in_an_independent_database(self):
        fixture(self.store, self.db)
        destination = self.root / "customers.sql"
        self.replay([
            *OPEN, tap(4.4, "rail-settings"), tap(5.2, "settings-nav-dataBrowsing"),
            tap(6.4, "backup-start"),
            {"t": 7.3, "type": "file-dialog", "path": str(destination)},
        ], until=10.5)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT id, customer_name FROM customers ORDER BY id").fetchall()
        with sqlite3.connect(":memory:") as restored:
            restored.executescript(destination.read_text(encoding="utf-8"))
            self.assertEqual(restored.execute(
                "SELECT id, customer_name FROM customers ORDER BY id").fetchall(), expected)

    def test_query_result_find_moves_between_matches(self):
        fixture(self.store, self.db)
        sql = "SELECT customer_name FROM customers ORDER BY id;"
        search = [*query_steps(sql), *fill(10.7, "result-find-input", "fixture-"),
                  tap(12.0, "result-find-apply")]
        tree = self.replay(search, until=13)
        self.expect_rendered(tree, key="result-find-count", text="1 of 2")
        tree = self.replay([*search, tap(13, "result-find-next")], until=14.2)
        self.expect_rendered(tree, key="result-find-count", text="2 of 2")

    def test_settings_snippet_survives_restart_then_can_be_deleted(self):
        editor = tap(1.4, "settings-nav-editor")
        self.replay([
            SETTINGS, editor, *fill(2.3, "snippet-name-input", "Rune local snippet"),
            *fill(3.8, "snippet-sql-input", "SELECT 42;"), tap(5.1, "snippet-save"),
        ], until=6.3)
        target = self.root / "snippets.json"
        self.assertIn("SELECT 42;", target.read_text(encoding="utf-8"))
        tree = self.replay([SETTINGS, editor], until=3)
        self.expect_rendered(tree, text="Rune local snippet", within="settings-snippets")
        tree = self.replay([SETTINGS, editor, tap(2.5, "snippet-delete-0")], until=3.8)
        self.assertFalse(visible(tree, text="Rune local snippet", within="settings-snippets"))
        self.assertNotIn("SELECT 42;", target.read_text(encoding="utf-8"))

    def test_settings_import_validates_missing_source_without_writing(self):
        tree = self.replay([SETTINGS, DATA_SETTINGS, tap(2.6, "import-next")], until=4.1)
        self.expect_rendered(tree, text="a source is required", within="import-error")
        self.expect_rendered(tree, key="import-step-1")
        self.assertFalse(visible(tree, key="import-step-2"))
        self.assertFalse(self.db.exists())

    def test_connection_search_filters_by_name_case_insensitively(self):
        fixture(self.store, self.db)
        store = json.loads(self.store.read_text())
        store["connections"].append({**store["connections"][0], "id": "sandbox-local",
                                     "name": "Sandbox Local"})
        self.store.write_text(json.dumps(store), encoding="utf-8")
        search = fill(.5, "navigator-filter-input", "sAnDbOx")
        tree = self.replay(search, until=2)
        self.expect_rendered(tree, key="nav-conn-sandbox-local")
        self.assertFalse(visible(tree, key="nav-conn-e2e-sqlite"))
        tree = self.replay([*search, *fill(2.2, "navigator-filter-input", "no-match")], until=4)
        self.expect_rendered(tree, key="nav-group-all-empty")
        self.assertFalse(visible(tree, key="nav-conn-sandbox-local"))
        self.assertEqual(json.loads(self.store.read_text()), store)

    def test_connection_dialog_cancel_does_not_save_draft(self):
        actions = [tap(.5, "connections-create-first"),
                   *fill(1.4, "connection-name-input", "Unsaved draft")]
        tree = self.replay(actions, until=2.7)
        self.expect_rendered(tree, key="connection-dialog")
        tree = self.replay([*actions, tap(2.9, "dialog-cancel")], until=4)
        self.expect_rendered(tree, key="connections-empty")
        self.assertFalse(visible(tree, key="connection-dialog"))
        self.assertFalse(self.store.exists())

    def test_sqlite_chart_view_uses_numeric_results_and_returns_to_grid(self):
        fixture(self.store, self.db)
        sql = "SELECT 'Ada' AS label, id AS metric FROM customers ORDER BY id;"
        chart = [*query_steps(sql), tap(11, "result-view-chart")]
        tree = self.replay(chart, until=12.4)
        for index in (0, 1):
            self.expect_rendered(tree, key=f"result-block-{index}", within="result-chart")
        self.assertFalse(visible(tree, key="result-grid"))
        tree = self.replay([*chart, tap(12.8, "result-view-table")], until=14.2)
        self.expect_rendered(tree, text="Ada", within="result-cell-0-0")
        self.expect_rendered(tree, text="2", within="result-cell-1-1")

    def test_sqlite_read_only_profile_rejects_query_writes(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text())
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.replay(query_steps("UPDATE customers SET customer_name='must-not-write' WHERE id=1;"),
                    until=11)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute(
                "SELECT customer_name FROM customers WHERE id=1").fetchone(), ("fixture-ada",))

    def test_sqlite_query_retains_rows_across_result_batches(self):
        fixture(self.store, self.db)
        sql = ("WITH RECURSIVE n(i) AS(SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<600) "
               "SELECT i, 'batch-'||i AS marker FROM n;")
        with sqlite3.connect(self.db) as connection:
            expected = connection.execute(sql).fetchall()
        self.assertEqual((len(expected), expected[-1]), (600, (600, "batch-600")))
        tree = self.replay([
            *query_steps(sql), *fill(11.0, "result-find-input", expected[-1][1]),
            tap(12.5, "result-find-apply"),
        ], until=14.0)
        self.expect_rendered(tree, key="result-grid-status", text="result · 600 rows × 2 cols")
        self.expect_rendered(tree, key="result-find-count", text="1 of 1")
        self.expect_rendered(tree, key="result-status-text")
        # The status prefix is localized; check the exact global cell address.
        self.assertTrue(any(
            node.get("key") == "result-status-text"
            and (node.get("text") or "").endswith(" · 600 rows × 2 cols · selection r599c1..r599c1")
            for node in tree["elements"]
        ))

    def test_sqlite_dirty_count_reverts_one_cell_and_commits_only_remaining_edits(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.execute("ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0")
            connection.execute("UPDATE customers SET score = id * 10")

        def rows():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as connection:
                return connection.execute(
                    "SELECT id, customer_name, score FROM customers ORDER BY id").fetchall()

        def edit(t, cell, value):
            return [tap(t, cell), key(t + .35, "Enter"), tap(t + .6, "grid-editor"),
                    key(t + .95, "a", cmd=True), type_text(t + 1.3, value),
                    key(t + 2.15, "Enter")]

        before = [(1, "fixture-ada", 10), (2, "fixture-lin", 20)]
        self.assertEqual(rows(), before)
        staged_steps = [
            *OPEN, *edit(4.8, "cell-0-1", "Ada, staged"),
            *edit(7.4, "cell-0-2", "17"), *edit(10.0, "cell-1-1", "temporary"),
        ]
        staged = self.replay(staged_steps, until=12.8)
        self.expect_rendered(staged, text="3 pending changes — use Commit/Rollback",
                             within="pending-changes-bar")
        self.expect_rendered(staged, text="temporary", within="cell-1-1")
        self.assertEqual(rows(), before)

        reverted_steps = [*staged_steps, *edit(13.1, "cell-1-1", "fixture-lin")]
        reverted = self.replay(reverted_steps, until=15.8)
        self.expect_rendered(reverted, text="2 pending changes — use Commit/Rollback",
                             within="pending-changes-bar")
        self.expect_rendered(reverted, text="fixture-lin", within="cell-1-1")
        self.assertEqual(rows(), before)

        committed = self.replay([*reverted_steps, tap(16.2, "commit-staged")], until=19.0)
        self.assertFalse(visible(committed, text="2 pending changes — use Commit/Rollback"))
        self.expect_rendered(committed, text="Ada, staged", within="cell-0-1")
        self.expect_rendered(committed, text="17", within="cell-0-2")
        self.assertEqual(rows(), [(1, "Ada, staged", 17), before[1]])
