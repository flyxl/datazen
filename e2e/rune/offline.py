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

    def test_settings_language_changes_other_ui_labels_and_survives_restart(self):
        settings = self.root / "settings.json"
        trigger = "settings-language-select-trigger"
        appearance = "settings-nav-appearance"
        opened = [SETTINGS, tap(1.5, trigger)]
        choices = self.replay(opened, until=2.5)
        self.expect_rendered(choices, text="外观", within=appearance)
        self.expect_rendered(choices, text="简体中文", within=trigger)
        self.expect_rendered(choices, key="option0")
        self.expect_rendered(choices, text="English", within="option0")
        self.expect_rendered(choices, key="option1")
        self.assertFalse(visible(choices, key="option2"))
        self.assertFalse(settings.exists(), "opening the selector must not save")

        changed = self.replay([*opened, tap(2.8, "option0")], until=4.2)
        self.expect_rendered(changed, text="Appearance", within=appearance)
        self.assertFalse(visible(changed, text="外观", within=appearance))
        self.assertFalse(visible(changed, key="settings-error"))
        self.assertEqual(json.loads(settings.read_text())["ui"]["language"], "en")

        restarted = self.replay([SETTINGS, tap(1.5, appearance)], until=2.8)
        self.expect_rendered(restarted, key="appearance-color-scheme-select-trigger")
        self.expect_rendered(restarted, text="Appearance", within=appearance)

        back = self.replay([*opened, tap(2.8, "option1")], until=4.2)
        self.expect_rendered(back, text="外观", within=appearance)
        self.assertEqual(json.loads(settings.read_text())["ui"]["language"], "zh-CN")
        restored = self.replay([SETTINGS], until=2.2)
        self.expect_rendered(restored, text="外观", within=appearance)
        self.expect_rendered(restored, key="settings-favorites")

    def test_settings_unsupported_saved_locale_falls_back_without_rewriting_file(self):
        settings = self.root / "settings.json"
        previous = '{"ui":{"language":"fr","queryMaxRows":23},"keep":"sentinel"}'
        settings.write_text(previous, encoding="utf-8")
        tree = self.replay([SETTINGS], until=2.3)
        self.expect_rendered(tree, text="外观", within="settings-nav-appearance")
        self.expect_rendered(tree, text="简体中文", within="settings-language-select-trigger")
        self.assertEqual(settings.read_text(encoding="utf-8"), previous)

        choice = self.replay([
            SETTINGS, tap(1.5, "settings-language-select-trigger"), tap(2.8, "option0"),
        ], until=4.2)
        self.expect_rendered(choice, text="Appearance", within="settings-nav-appearance")
        self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), {
            "ui": {"language": "en", "queryMaxRows": 23}, "keep": "sentinel",
        })

    def test_settings_language_failed_save_does_not_switch_rendered_language(self):
        blocked = self.root / "settings.json"
        blocked.mkdir()
        sentinel = blocked / "keep"
        sentinel.write_text("unchanged", encoding="utf-8")
        tree = self.replay([
            SETTINGS, tap(1.5, "settings-language-select-trigger"), tap(2.8, "option0"),
        ], until=4.4)
        self.expect_rendered(tree, key="settings-error")
        self.expect_rendered(tree, text="外观", within="settings-nav-appearance")
        self.expect_rendered(tree, text="简体中文", within="settings-language-select-trigger")
        self.assertFalse(visible(tree, key="option0"))
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "unchanged")
        self.assertEqual(list(blocked.iterdir()), [sentinel])

    def test_settings_theme_persists_after_relaunch(self):
        appearance = tap(1.4, "settings-nav-appearance")
        scheme = "appearance-color-scheme-select-trigger"
        opened = [SETTINGS, appearance, tap(2.3, scheme)]
        choices = self.replay(opened, until=3.1)
        self.expect_rendered(choices, key="option0")
        self.expect_rendered(choices, key="option1")
        self.assertFalse(visible(choices, key="option2"))
        self.assertFalse(visible(choices, key="appearance-theme-select-trigger"))
        self.assertFalse((self.root / "settings.json").exists(), "opening does not save")

        tree = self.replay([*opened, tap(3.4, "option0")], until=4.6)
        self.assertFalse(visible(tree, key="option0"))
        self.expect_rendered(tree, key=scheme)
        settings = json.loads((self.root / "settings.json").read_text())
        self.assertIs(settings["ui"]["themeDark"], False)

        relaunched = self.replay([SETTINGS, appearance], until=2.4)
        self.expect_rendered(relaunched, key=scheme)
        # Native defaults to dark; persist the explicit opposite choice again
        # after confirming the new process restored light.
        self.assertTrue(any(visible(relaunched, text=label, within=scheme)
                            for label in ("Light", "浅色")))
        dark = self.replay([*opened, tap(3.4, "option1")], until=4.6)
        self.assertFalse(visible(dark, key="option1"))
        settings = json.loads((self.root / "settings.json").read_text())
        self.assertIs(settings["ui"]["themeDark"], True)

    def test_settings_theme_failed_save_keeps_previous_choice_and_reports_error(self):
        blocked = self.root / "settings.json"
        blocked.mkdir()
        sentinel = blocked / "keep"
        sentinel.write_text("unchanged", encoding="utf-8")
        scheme = "appearance-color-scheme-select-trigger"
        tree = self.replay([
            SETTINGS, tap(1.4, "settings-nav-appearance"), tap(2.3, scheme),
            tap(3.4, "option0"),
        ], until=4.8)
        self.expect_rendered(tree, key="settings-error")
        self.assertTrue(any(visible(tree, text=label, within=scheme)
                            for label in ("Dark", "深色")))
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "unchanged")
        self.assertEqual(list(blocked.iterdir()), [sentinel])

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

    def test_settings_navigation_changes_real_grid_page_boundary_after_restart(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.execute("DELETE FROM customers")
            connection.executemany("INSERT INTO customers VALUES (?, ?)",
                                   [(i, f"row-{i:02}") for i in range(1, 53)])
            expected = {
                size: connection.execute(
                    "SELECT customer_name FROM customers ORDER BY id LIMIT 1 OFFSET ?",
                    (size,),
                ).fetchone()[0] for size in (17, 25)
            }
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"tablePageSize": 17, "queryMaxRows": 23}}), encoding="utf-8")
        next_page = [*OPEN, tap(5.0, "page-next")]
        first = self.replay(next_page, until=6.9)
        self.expect_rendered(first, key="grid-page-label", text="Page 2 / 4")
        self.expect_rendered(first, text=expected[17], within="cell-0-1")

        changed = self.replay([SETTINGS, DATA_SETTINGS,
                               *fill(2.2, "settings-page-size-input", "25"),
                               tap(3.3, "settings-page-size-set")], until=4.7)
        self.expect_rendered(changed, text="Rows per page: 25", within="settings-data")
        self.assertFalse(visible(changed, key="settings-error"))
        settings = json.loads(self.root.joinpath("settings.json").read_text())["ui"]
        self.assertEqual((settings["tablePageSize"], settings["queryMaxRows"]), (25, 23))

        second = self.replay(next_page, until=6.9)
        self.expect_rendered(second, key="grid-page-label", text="Page 2 / 3")
        self.expect_rendered(second, text=expected[25], within="cell-0-1")

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

    def test_sqlite_grid_rejects_atomic_batch_then_commits_corrected_values(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.executescript("""
                CREATE TABLE edit_audit (id INTEGER, name TEXT);
                CREATE TRIGGER reject_second_edit BEFORE UPDATE ON customers
                WHEN OLD.id=2 AND NEW.customer_name='reject-this'
                BEGIN SELECT RAISE(ABORT, 'fixture rejected row two'); END;
                CREATE TRIGGER log_edits AFTER UPDATE ON customers
                BEGIN INSERT INTO edit_audit VALUES (NEW.id, NEW.customer_name); END;
            """)

        def stored():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as connection:
                return (
                    connection.execute("SELECT * FROM customers ORDER BY id").fetchall(),
                    connection.execute("SELECT * FROM edit_audit ORDER BY rowid").fetchall(),
                )

        def edit(t, cell, value):
            return [tap(t, cell), key(t + .35, "Enter"), tap(t + .6, "grid-editor"),
                    key(t + .95, "a", cmd=True), type_text(t + 1.3, value),
                    key(t + 2.15, "Enter")]

        before = ([(1, "fixture-ada"), (2, "fixture-lin")], [])
        invalid = [*OPEN, *edit(4.8, "cell-0-1", "first-valid"),
                   *edit(7.4, "cell-1-1", "reject-this"), tap(10.2, "commit-staged")]
        rejected = self.replay(invalid, until=12.6)
        self.assertFalse(visible(rejected, key="commit-loading-banner"))
        self.expect_rendered(rejected, text="2 pending changes — use Commit/Rollback",
                             within="pending-changes-bar")
        self.expect_rendered(rejected, text="first-valid", within="cell-0-1")
        self.expect_rendered(rejected, text="reject-this", within="cell-1-1")
        self.assertEqual(stored(), before, "failed row must roll back the valid edit and its audit entry")

        discarded = self.replay([*invalid, tap(13.0, "rollback-staged")], until=14.8)
        self.assertFalse(visible(discarded, key="pending-changes-bar"))
        self.expect_rendered(discarded, text="fixture-ada", within="cell-0-1")
        self.expect_rendered(discarded, text="fixture-lin", within="cell-1-1")
        self.assertEqual(stored(), before)

        committed = self.replay([*invalid, *edit(13.2, "cell-1-1", "second-valid"),
                                 tap(16.0, "commit-staged")], until=18.6)
        self.assertFalse(visible(committed, key="commit-loading-banner"))
        self.assertFalse(visible(committed, key="pending-changes-bar"))
        self.expect_rendered(committed, text="first-valid", within="cell-0-1")
        self.expect_rendered(committed, text="second-valid", within="cell-1-1")
        expected = [(1, "first-valid"), (2, "second-valid")]
        self.assertEqual(stored(), (expected, expected))

    def test_sqlite_grid_without_primary_key_refuses_to_persist_staged_edit(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.executescript("""
                CREATE TABLE keyless (label TEXT);
                INSERT INTO keyless VALUES ('first'), ('first');
            """)
        actions = [OPEN[0], tap(3, "tree-table-keyless"), tap(5, "cell-0-0"),
                   key(5.4, "Enter"), *fill(5.75, "grid-editor", "not-persisted"),
                   key(6.9, "Enter"), tap(7.6, "commit-staged")]
        tree = self.replay(actions, until=9.5)
        self.expect_rendered(tree, text="not-persisted", within="cell-0-0")
        self.expect_rendered(tree, text="first", within="cell-1-0")
        self.expect_rendered(tree, text="1 pending change — use Commit/Rollback",
                             within="pending-changes-bar")
        self.assertFalse(visible(tree, key="commit-loading-banner"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as connection:
            self.assertEqual(connection.execute("SELECT label FROM keyless ORDER BY rowid").fetchall(),
                             [("first",), ("first",)])

    def test_legacy_connection_group_expansion_and_search_clear_restore_connections(self):
        """Migrate the expand/collapse and clear cases in connection-search-group.ts."""
        fixture(self.store, self.db)
        saved = json.loads(self.store.read_text(encoding="utf-8"))
        saved["connections"].append({
            **saved["connections"][0], "id": "other-local", "name": "Other local SQLite",
        })
        self.store.write_text(json.dumps(saved), encoding="utf-8")
        group = "nav-group-all"
        search = "navigator-filter-input"
        first, second = "nav-conn-e2e-sqlite", "nav-conn-other-local"
        collapsed = self.replay([tap(.6, group)], until=1.9)
        self.expect_rendered(collapsed, key=group)
        self.assertFalse(visible(collapsed, key=first))
        self.assertFalse(visible(collapsed, key=second))

        filtered = [tap(.6, group), *fill(1.8, search, "Other local")]
        visible_match = self.replay(filtered, until=3.2)
        self.expect_rendered(visible_match, key=second)
        self.assertFalse(visible(visible_match, key=first))

        cleared = [*filtered, tap(3.4, search), key(3.6, "a", cmd=True), key(3.9, "Backspace")]
        closed_again = self.replay(cleared, until=4.9)
        self.assertFalse(visible(closed_again, key=first))
        self.assertFalse(visible(closed_again, key=second))

        reopened = self.replay([*cleared, tap(5.0, group)], until=6.1)
        self.expect_rendered(reopened, key=first)
        self.expect_rendered(reopened, key=second)
        self.assertEqual(json.loads(self.store.read_text(encoding="utf-8")), saved)

    def test_legacy_settings_navigation_switches_all_six_sections(self):
        """Migrate settings.ts SS-NAV-001 (navigation, not feature availability)."""
        sections = ("behavior", "logging", "ai", "mcpServer", "mcpClient", "extensions")
        for section in sections:
            with self.subTest(section=section):
                tree = self.replay([SETTINGS, tap(1.4, f"settings-nav-{section}")], until=2.3)
                self.expect_rendered(tree, key=f"settings-section-{section}")
                self.assertFalse(visible(tree, key="settings-section-general"))
                for other in sections:
                    if other != section:
                        self.assertFalse(visible(tree, key=f"settings-section-{other}"))
        self.assertFalse((self.root / "settings.json").exists(), "navigation must not save settings")

    def test_legacy_sqlite_published_view_opens_and_shows_only_its_result(self):
        """Migrate the display-view-data journey from sqlite.ts."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE posts(id INTEGER PRIMARY KEY, author TEXT, published INTEGER);
                INSERT INTO posts VALUES (1, 'published-author', 1), (2, 'draft-author', 0);
                CREATE VIEW published_posts AS
                    SELECT author FROM posts WHERE published = 1;
            """)
            expected = db.execute("SELECT author FROM published_posts").fetchall()
        self.assertEqual(expected, [("published-author",)])
        with sqlite3.connect(self.db) as db:
            original = tuple(db.iterdump())
        opened = self.replay([OPEN[0], tap(3, "tree-table-published_posts")], until=5.8)
        self.expect_rendered(opened, key="tree-table-published_posts", within="datazen-navigator")
        self.expect_rendered(opened, text=expected[0][0], within="cell-0-0")
        self.assertFalse(visible(opened, text="draft-author", within="table-workspace"))
        # The live SQLite driver may initialize WAL on read. Compare all
        # logical contents and schema rather than the physical journal bytes.
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(tuple(db.iterdump()), original)

    def test_legacy_sqlite_sidebar_lists_views_under_separate_category(self):
        """Migrate the show-views-in-sidebar case from sqlite.ts."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE posts(id INTEGER PRIMARY KEY, author TEXT, published INTEGER);
                CREATE VIEW published_posts AS SELECT author FROM posts WHERE published = 1;
                CREATE VIEW draft_posts AS SELECT author FROM posts WHERE published = 0;
            """)
            expected = [name for (name,) in db.execute(
                "SELECT name FROM sqlite_schema WHERE type = 'view' ORDER BY name"
            )]
        category = "tree-views-main-main"
        collapsed = [OPEN[0], tap(3, category)]
        initial = self.replay(collapsed, until=4.2)
        self.expect_rendered(initial, text="视图 (2)", within=category)
        self.expect_rendered(initial, key="tree-table-posts")
        for name in expected:
            self.assertFalse(visible(initial, key=f"tree-table-{name}"))

        reopened = self.replay([*collapsed, tap(4.5, category)], until=5.8)
        for name in expected:
            self.expect_rendered(reopened, key=f"tree-table-{name}", within="datazen-navigator")
        self.expect_rendered(reopened, key="tree-table-posts")

        filtered = [*collapsed, *fill(4.5, "navigator-filter-input", "published")]
        narrowed = self.replay(filtered, until=6.1)
        self.expect_rendered(narrowed, text="视图 (1)", within=category)
        self.expect_rendered(narrowed, key="tree-table-published_posts")
        self.assertFalse(visible(narrowed, key="tree-table-draft_posts"))
        self.assertFalse(visible(narrowed, key="tree-table-posts"))

    def test_legacy_workspace_switches_and_closes_tabs_then_returns_to_empty(self):
        """Migrate unified-tab-bar.ts UTB-001 through UTB-006."""
        fixture(self.store, self.db)
        sql = "SELECT customer_name FROM customers WHERE id = 2;"
        with sqlite3.connect(self.db) as db:
            expected = db.execute(sql).fetchone()[0]
            original = db.execute("SELECT * FROM customers ORDER BY id").fetchall()
        query = [*OPEN, tap(4.4, "conn-toolbar-new-query"), tap(5.5, "sql-editor-scroll"),
                 key(5.8, "a", cmd=True), type_text(6.0, sql), tap(8.9, "sql-run")]
        before = self.replay(query, until=10.5)
        self.expect_rendered(before, key="conn-toolbar-new-query")
        self.expect_rendered(before, text="customers", within="tab-1")
        self.expect_rendered(before, text="Query", within="tab-2")
        self.expect_rendered(before, text=expected, within="result-workspace")

        table = [*query, tap(10.7, "tab-1")]
        active_table = self.replay(table, until=12.0)
        self.expect_rendered(active_table, text="fixture-ada", within="table-workspace")
        self.assertFalse(visible(active_table, key="result-workspace"))
        self.expect_rendered(active_table, key="tab-2")

        switch_back = [*table, tap(12.2, "tab-2")]
        active_query = self.replay(switch_back, until=13.3)
        self.expect_rendered(active_query, text=expected, within="result-workspace")
        self.assertFalse(visible(active_query, key="table-workspace"))

        close_query = [*switch_back, tap(13.5, "tab-close-2")]
        closed = self.replay(close_query, until=14.9)
        self.assertFalse(visible(closed, key="tab-2"))
        self.expect_rendered(closed, key="tab-1")
        self.expect_rendered(closed, text="fixture-ada", within="table-workspace")

        empty = self.replay([*close_query, tap(15.1, "tab-close-1")], until=16.3)
        self.expect_rendered(empty, key="workspace-empty")
        self.expect_rendered(empty, key="conn-toolbar-new-query")
        self.assertFalse(visible(empty, key="tab-1"))
        self.assertFalse(visible(empty, key="tab-2"))
        self.assertFalse(visible(empty, key="table-workspace"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), original)

    def test_legacy_table_double_click_edits_current_value_and_escape_discards_draft(self):
        """Migrate table-edit.ts DE-002 and DE-005 on the real SQLite grid."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            initial = db.execute("SELECT * FROM customers ORDER BY id").fetchall()
        open_editor = [*OPEN, {**tap(5, "cell-0-1"), "type": "double-click"}]
        shown = self.replay(open_editor, until=6.3)
        self.expect_rendered(shown, key="grid-editor", within="cell-0-1")

        def find_editor(node, within_editor=False):
            within_editor = within_editor or node.get("key") == "grid-editor"
            if within_editor and node.get("role") == "text-input":
                return node
            for child in node.get("children", ()):
                editor = find_editor(child, within_editor)
                if editor is not None:
                    return editor
            return None

        editor = find_editor(shown["accessibility"])
        self.assertIsNotNone(editor)
        self.assertEqual(editor.get("value"), "fixture-ada")
        draft_actions = [*open_editor, key(6.4, "a", cmd=True),
                         type_text(6.7, "discard-edited-draft")]
        staged_input = find_editor(self.replay(draft_actions, until=7.9)["accessibility"])
        self.assertIsNotNone(staged_input)
        self.assertEqual(staged_input.get("value"), "discard-edited-draft")
        cancelled = self.replay([
            *draft_actions, key(8.1, "Escape"), tap(9.1, "commit-staged"),
        ], until=10.6)
        self.assertFalse(visible(cancelled, key="grid-editor"))
        self.expect_rendered(cancelled, text="fixture-ada", within="cell-0-1")
        self.assertFalse(visible(cancelled, text="discard-edited-draft"))
        status = next(node["text"] for node in cancelled["elements"]
                      if node.get("key") == "data-grid-status")
        self.assertTrue(status.endswith(" · 0 dirty"), status)
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), initial)

    def test_legacy_table_numeric_sql_update_survives_reload(self):
        """Migrate the typed update and readback in table-edit.ts DE-003b."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                ALTER TABLE customers ADD COLUMN score INTEGER DEFAULT 0;
                UPDATE customers SET score = id * 10;
            """)
        changed = "UPDATE customers SET score = 999 WHERE id = 2;"
        read = "SELECT score, typeof(score) FROM customers WHERE id = 2;"
        result = self.replay([
            *query_steps(changed), *fill(10.4, "sql-editor-scroll", read),
            tap(13.0, "sql-run"),
        ], until=15.4)
        self.expect_rendered(result, text="999", within="result-workspace")
        self.expect_rendered(result, text="integer", within="result-workspace")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(
                db.execute("SELECT id, score, typeof(score) FROM customers ORDER BY id").fetchall(),
                [(1, 10, "integer"), (2, 999, "integer")],
            )
        reopened = self.replay(OPEN, until=5.5)
        self.expect_rendered(reopened, text="10", within="cell-0-2")
        self.expect_rendered(reopened, text="999", within="cell-1-2")

    def test_legacy_er_opens_real_sqlite_tables_with_nonoverlapping_nodes(self):
        """Migrate ER-002, ER-003 and the node-layout part of ER-010."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE locations (location_id INTEGER PRIMARY KEY, label TEXT);
                CREATE TABLE visits (visit_id INTEGER PRIMARY KEY, locale TEXT,
                    location_id INTEGER REFERENCES locations(location_id));
            """)
            column_names = {
                table: [entry[1] for entry in db.execute(f"PRAGMA table_info({table})")]
                for table in ("customers", "locations", "visits")
            }
        connect = [OPEN[0]]
        connected = self.replay(connect, until=2.5)
        self.expect_rendered(connected, key="er-button")
        self.assertFalse(visible(connected, key="er-canvas"))

        show = [*connect, tap(3.0, "er-button")]
        diagram = self.replay(show, until=6.5)
        self.expect_rendered(diagram, key="er-page")
        self.expect_rendered(diagram, key="er-canvas")
        boxes = []
        for table, columns in column_names.items():
            node = f"er-box-{table}"
            self.expect_rendered(diagram, key=node, within="er-canvas")
            self.expect_rendered(diagram, key=f"er-title-{table}", text=table, within=node)
            for index, column in enumerate(columns):
                self.expect_rendered(diagram, key=f"er-col-{table}-{index}", text=column, within=node)
            self.assertFalse(visible(diagram, key=f"er-pending-{table}"))
            boxes.append(next(e["rect"] for e in diagram["elements"] if e.get("key") == node))

        from itertools import combinations
        for (ax, ay, aw, ah), (bx, by, bw, bh) in combinations(boxes, 2):
            self.assertTrue(ax + aw <= bx or bx + bw <= ax or ay + ah <= by or by + bh <= ay,
                            "SQLite ER table nodes overlap")

    def test_legacy_er_statistics_count_one_declared_relationship_for_composite_fk(self):
        """Migrate er-diagram.ts ER-006 with real SQLite catalog metadata."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE warehouse (region TEXT, code INTEGER, PRIMARY KEY(region, code));
                CREATE TABLE inventory (
                    row_id INTEGER PRIMARY KEY, region TEXT, code INTEGER,
                    FOREIGN KEY(region, code) REFERENCES warehouse(region, code)
                );
                INSERT INTO warehouse VALUES ('north', 1);
                INSERT INTO inventory VALUES (10, 'north', 1);
            """)
            tables = [name for (name,) in db.execute(
                "SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name"
            )]
            parts = db.execute("PRAGMA foreign_key_list(inventory)").fetchall()
            links = len({part[0] for part in parts})
            original = tuple(db.iterdump())
        self.assertEqual((len(tables), len(parts), links), (3, 2, 1))
        self.assertEqual([(part[2], part[3], part[4]) for part in parts],
                         [("warehouse", "region", "region"), ("warehouse", "code", "code")])

        opened = [OPEN[0], tap(3, "er-button")]
        stats = self.replay(opened, until=6)
        self.expect_rendered(stats, key="er-table-count", text=f"{len(tables)} 张表",
                             within="er-diagram-stats")
        self.expect_rendered(stats, key="er-relationship-count", text=f"{links} 个已声明外键",
                             within="er-diagram-stats")

        def states(node, expected_key):
            if node.get("key") == expected_key:
                return node.get("states", [])
            for child in node.get("children", ()):
                hit = states(child, expected_key)
                if hit is not None:
                    return hit
            return None

        self.assertIn("unchecked", states(stats["accessibility"], "er-box-inventory"))
        selected = self.replay([*opened, tap(6.2, "er-title-inventory")], until=7.7)
        self.expect_rendered(selected, key="er-box-inventory", within="er-canvas")
        self.assertIn("checked", states(selected["accessibility"], "er-box-inventory"))
        self.assertIn("unchecked", states(selected["accessibility"], "er-box-warehouse"))
        self.expect_rendered(selected, key="er-relationship-count", text="1 个已声明外键")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(tuple(db.iterdump()), original)

    def test_legacy_er_search_filters_visible_catalog_nodes_and_can_be_cleared(self):
        """Migrate er-diagram.ts ER-007 with non-vacuous matching assertions."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE pg_inventory (id INTEGER PRIMARY KEY);
                CREATE TABLE pg_receipts (id INTEGER PRIMARY KEY);
            """)
            matches = [name for (name,) in db.execute("""
                SELECT name FROM sqlite_schema
                WHERE type = 'table' AND substr(lower(name), 1, 3) = 'pg_' ORDER BY name
            """)]
        self.assertEqual(matches, ["pg_inventory", "pg_receipts"])
        search = "er-diagram-search"
        opened = [OPEN[0], tap(3, "er-button")]
        filtered = [*opened, *fill(5, search, "PG_")]
        view = self.replay(filtered, until=6.7)
        for name in matches:
            self.expect_rendered(view, key=f"er-box-{name}", within="er-canvas")
        self.assertFalse(visible(view, key="er-box-customers"))
        self.assertFalse(visible(view, key="er-search-no-matches"))

        no_match = [*filtered, *fill(7, search, "missing-table")]
        empty = self.replay(no_match, until=9.0)
        self.expect_rendered(empty, key="er-search-no-matches", within="er-canvas")
        for name in [*matches, "customers"]:
            self.assertFalse(visible(empty, key=f"er-box-{name}"))

        full = self.replay([*no_match, tap(9.2, search), key(9.4, "a", cmd=True),
                            key(9.7, "Backspace")], until=11)
        for name in [*matches, "customers"]:
            self.expect_rendered(full, key=f"er-box-{name}", within="er-canvas")
        self.assertFalse(visible(full, key="er-search-no-matches"))

    def test_legacy_er_relayout_restores_nonoverlapping_table_locations(self):
        """Migrate er-diagram.ts ER-013 and show that re-layout really moves a node."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE east_nodes (id INTEGER PRIMARY KEY);
                CREATE TABLE west_nodes (id INTEGER PRIMARY KEY);
            """)
            names = [name for (name,) in db.execute(
                "SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name"
            )]
        opened = [OPEN[0], tap(3, "er-button")]

        def bounds(tree):
            result = {}
            for name in names:
                box = f"er-box-{name}"
                self.expect_rendered(tree, key=box, within="er-canvas")
                result[name] = next(n["rect"] for n in tree["elements"] if n.get("key") == box)
            return result

        baseline = bounds(self.replay(opened, until=6))
        left, nearby = names[:2]
        horizontal = baseline[nearby][0] - baseline[left][0]
        self.assertGreater(horizontal, 0)
        drag = {**tap(6.2, f"er-title-{left}"), "type": "drag", "dx": horizontal, "dy": 0, "dur": .7}
        moved = bounds(self.replay([*opened, drag], until=7.7))
        self.assertGreater(moved[left][0] - baseline[left][0], 30,
                           "drag must actually move a real table node")
        self.assertEqual(moved[nearby], baseline[nearby])

        reset = bounds(self.replay([*opened, drag, tap(8.0, "er-diagram-relayout")], until=9.6))
        self.assertEqual(reset, baseline)
        from itertools import combinations
        for (ax, ay, aw, ah), (bx, by, bw, bh) in combinations(reset.values(), 2):
            self.assertTrue(ax + aw <= bx or bx + bw <= ax or ay + ah <= by or by + bh <= ay,
                            "re-layout left two rendered table nodes overlapping")
