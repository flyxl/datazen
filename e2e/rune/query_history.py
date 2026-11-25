"""SQLite migration of the native query-history journey."""
import json

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
