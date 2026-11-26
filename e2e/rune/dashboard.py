"""Migrate the SQL-backed dashboard journey using an isolated SQLite file."""
import json
import sqlite3

from offline import fill
from scenarios import fixture, tap, visible


class DashboardJourney:
    def test_dashboard_sql_widget_persists_and_refreshes_live_sqlite(self):
        fixture(self.store, self.db)
        title = "SQLite E2E Bars"
        sql = "SELECT customer_name, id FROM customers ORDER BY id"
        start = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-dashboards")]
        invalid = self.replay([*start, tap(4.2, "widget-save")], until=5.4)
        self.expect_rendered(invalid, key="widget-form-error")
        self.expect_rendered(invalid, key="dashboard-empty")
        self.assertFalse((self.root / "dashboards.json").exists())
        self.assertFalse((self.root / "workflows.json").exists())

        created = self.replay([
            *start, *fill(4.1, "widget-title-input", title),
            *fill(5.5, "widget-query-input", sql), tap(8, "widget-save"),
        ], until=10)
        widgets = json.loads((self.root / "dashboards.json").read_text())["widgets"]
        self.assertEqual(len(widgets), 1)
        self.assertEqual(widgets[0]["title"], title)
        workflow_file = json.loads((self.root / "workflows.json").read_text())
        self.assertEqual(len(workflow_file), 1)
        self.assertIn(sql, json.dumps(workflow_file))
        self.expect_rendered(created, text="fixture-ada 1", within="dashboards-page")
        self.expect_rendered(created, text="fixture-lin 2", within="dashboards-page")
        self.assertFalse(visible(created, key="widget-form-error"))

        with sqlite3.connect(self.db) as connection:
            connection.execute("UPDATE customers SET customer_name='dashboard-fresh' WHERE id=2")
        refreshed = self.replay([*start, tap(4.5, "widget-refresh")], until=6.5)
        self.expect_rendered(refreshed, text="fixture-ada 1", within="dashboards-page")
        self.expect_rendered(refreshed, text="dashboard-fresh 2", within="dashboards-page")
        self.assertFalse(visible(refreshed, text="fixture-lin 2", within="dashboards-page"))
        self.assertEqual(json.loads((self.root / "dashboards.json").read_text())["widgets"], widgets)
        workflow = self.replay([tap(.5, "rail-workflows")], until=2.5)
        self.expect_rendered(workflow, key="workflow-list-empty")
