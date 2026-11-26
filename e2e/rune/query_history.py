"""SQLite migration of the native query-history journey."""
import json
import sqlite3

from offline import fill
from scenarios import fixture, query_steps, tap, visible


class QueryHistoryJourney:
    def test_sql_history_tracks_success_failure_and_connection_across_restart(self):
        fixture(self.store, self.db)
        other = self.root / "other.sqlite"
        other_store = self.root / "other-connections.json"
        fixture(other_store, other)
        profile = json.loads(self.store.read_text())
        profile["connections"].append({
            **json.loads(other_store.read_text())["connections"][0],
            "id": "e2e-other",
            "name": "Other SQLite fixture",
        })
        self.store.write_text(json.dumps(profile), encoding="utf-8")

        successful = "SELECT 'history-success' AS note;"
        failing = "SELECT * FROM missing_history_table;"
        to_history = tap(6.4, "query-sidebar-tab-history")
        run_twice = [
            *query_steps(successful)[:-1], to_history, tap(8.5, "sql-run"),
            *fill(10.2, "sql-editor-scroll", failing), tap(13.7, "sql-run"),
        ]
        first = self.replay(run_twice, until=15.5)
        self.expect_rendered(first, key="query-history-list")
        self.expect_rendered(first, text=failing.rstrip(";"), within="query-history-entry-0")
        self.expect_rendered(first, key="query-history-failure", within="query-history-entry-0")
        self.expect_rendered(first, key="query-history-entry-error", within="query-history-entry-0")
        self.expect_rendered(first, text=successful.rstrip(";"), within="query-history-entry-1")
        self.expect_rendered(first, key="query-history-success", within="query-history-entry-1")

        history_path = self.root / "query-history.json"
        saved_history = json.loads(history_path.read_text())
        self.assertEqual(
            [(entry["connectionId"], entry["sql"].rstrip(";"), entry["success"])
             for entry in saved_history],
            [("e2e-sqlite", successful.rstrip(";"), True),
             ("e2e-sqlite", failing.rstrip(";"), False)],
        )
        self.assertTrue(saved_history[1]["errorMessage"])

        inspect_history = [
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "conn-toolbar-new-query"),
            tap(4, "query-sidebar-tab-history"),
        ]
        reopened = self.replay(inspect_history, until=6)
        self.expect_rendered(reopened, text=failing.rstrip(";"), within="query-history-entry-0")
        self.expect_rendered(reopened, text=successful.rstrip(";"), within="query-history-entry-1")

        restore = [*inspect_history, tap(6.2, "query-history-entry-1")]
        restored = self.replay(restore, until=8)
        self.assertFalse(visible(restored, text="history-success", within="result-workspace"))
        self.assertEqual(json.loads(history_path.read_text()), saved_history)
        rerun = self.replay([*restore, tap(8.1, "sql-run")], until=10.5)
        self.expect_rendered(rerun, text="history-success", within="result-workspace")
        self.assertEqual(len(json.loads(history_path.read_text())), 3)

        empty = self.replay([
            tap(.5, "conn-open-e2e-other"), tap(3, "conn-toolbar-new-query"),
            tap(4, "query-sidebar-tab-history"),
        ], until=6)
        self.expect_rendered(empty, key="query-history-empty")
        self.assertFalse(visible(empty, key="query-history-list"))


    def test_legacy_query_history_collapses_consecutive_execution_only(self):
        """Migrate SQ-021 without confusing a collapsed card with a skipped execution."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("CREATE TABLE counter (n INTEGER NOT NULL); INSERT INTO counter VALUES (0);")
        repeated = "UPDATE counter SET n = n + 1;"
        separator = "SELECT n FROM counter;"
        open_history = [
            tap(.5, "conn-open-e2e-sqlite"),
            tap(3, "conn-toolbar-new-query"),
            tap(4.5, "query-sidebar-tab-history"),
        ]
        history_file = self.root / "query-history.json"

        first = self.replay([
            *open_history, *fill(5.2, "sql-editor-scroll", repeated),
            tap(8, "sql-run"), tap(10.5, "sql-run"), tap(13, "sql-run"),
        ], until=15.5)
        self.expect_rendered(first, text=repeated.rstrip(";"), within="query-history-entry-0")
        self.expect_rendered(first, key="query-history-success", within="query-history-entry-0")
        self.assertFalse(visible(first, key="query-history-entry-1"))
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute(separator).fetchone(), (3,))
        saved = json.loads(history_file.read_text(encoding="utf-8"))
        self.assertEqual([(r["sql"].rstrip(";"), r["success"]) for r in saved],
                         [(repeated.rstrip(";"), True)])

        second = self.replay([
            *open_history, *fill(5.2, "sql-editor-scroll", separator), tap(8, "sql-run"),
            *fill(10, "sql-editor-scroll", repeated), tap(13, "sql-run"),
        ], until=15.5)
        for index, sql in enumerate((repeated, separator, repeated)):
            self.expect_rendered(second, text=sql.rstrip(";"), within=f"query-history-entry-{index}")
        self.assertFalse(visible(second, key="query-history-entry-3"))
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute(separator).fetchone(), (4,))
        stored = json.loads(history_file.read_text(encoding="utf-8"))
        self.assertEqual(stored[0]["id"], saved[0]["id"])
        self.assertEqual(len({r["id"] for r in stored}), 3)
