"""Migrate the SQL-backed dashboard journey using an isolated SQLite file."""
import json
import sqlite3

from offline import fill
from scenarios import fixture, tap, visible


class DashboardJourney:
    def _create_local_dashboard_fixture(self):
        created = self.replay([
            tap(.55, "rail-dashboards"), tap(2.05, "dashboard-create-first"),
        ], until=3.4)
        self.expect_rendered(created, key="dashboard-selected-board-title")
        self.expect_rendered(created, key="dashboard-empty")
        return (self.root / "dashboards.json").read_bytes()

    def test_create_first_dashboard_without_database_then_restore_exact_board(self):
        (self.root / "settings.json").write_text(
            json.dumps({"ui": {"language": "en"}}), encoding="utf-8",
        )
        path = self.root / "dashboards.json"
        workflows = self.root / "workflows.json"
        enter = [tap(.55, "rail-dashboards")]
        empty = self.replay(enter, until=1.8)
        self.expect_rendered(empty, key="dashboard-empty-boards")
        self.expect_rendered(empty, key="dashboard-create-first", within="dashboard-empty-boards")
        self.assertFalse(visible(empty, key="widget-save"))
        self.assertFalse(path.exists())
        self.assertFalse(workflows.exists())
        self.assertFalse(self.store.exists())

        before = self._create_local_dashboard_fixture()
        data = json.loads(before)
        (created,) = data["dashboards"]
        self.assertEqual((created["id"], created["name"]), ("default", "New Dashboard"))
        self.assertTrue(created["enabled"])
        self.assertEqual(data["widgets"], [])
        self.assertFalse(workflows.exists())
        self.assertFalse(self.store.exists())
        self.assertFalse(self.db.exists())

        restored = self.replay(enter, until=1.8)
        self.expect_rendered(restored, key="dashboard-selected-board-title", text="New Dashboard")
        self.expect_rendered(restored, key="dashboard-empty")
        self.expect_rendered(restored, key="widget-save")
        self.assertFalse(visible(restored, key="dashboard-create-first"))
        self.assertEqual(path.read_bytes(), before)

        back = self.replay([
            *enter, tap(2.0, "rail-settings"), tap(3.5, "settings-back"),
        ], until=4.5)
        self.expect_rendered(back, key="dashboard-selected-board-title", text="New Dashboard")
        self.expect_rendered(back, key="dashboard-empty")
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse(workflows.exists())

    def test_dashboard_does_not_present_unknown_record_as_create_first(self):
        path = self.root / "dashboards.json"
        before = json.dumps({
            "dashboards": [{
                "id": "existing-unsupported",
                "name": "Keep this board",
                "createdAt": "2026-01-01T00:00:00Z",
                "updatedAt": "2026-01-01T00:00:00Z",
                "layoutCols": 12,
                "layoutRowHeight": 4,
                "enabled": True,
                "refreshPaused": False,
            }],
            "widgets": [],
        }).encode()
        path.write_bytes(before)
        tree = self.replay([tap(.55, "rail-dashboards")], until=2.1)
        self.expect_rendered(tree, key="dashboard-board-error")
        self.assertFalse(visible(tree, key="dashboard-create-first"))
        self.assertFalse(visible(tree, key="dashboard-empty-boards"))
        self.assertFalse(visible(tree, key="widget-save"))
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((self.root / "workflows.json").exists())

    def test_dashboard_sql_widget_persists_and_refreshes_live_sqlite(self):
        fixture(self.store, self.db)
        title = "SQLite E2E Bars"
        sql = "SELECT customer_name, id FROM customers ORDER BY id"
        empty_board = self._create_local_dashboard_fixture()
        start = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-dashboards")]
        invalid = self.replay([*start, tap(4.2, "widget-save")], until=5.4)
        self.expect_rendered(invalid, key="widget-form-error")
        self.expect_rendered(invalid, key="dashboard-empty")
        self.expect_rendered(invalid, key="dashboard-empty-icon", within="dashboard-empty")
        self.expect_rendered(invalid, key="dashboard-empty-message", within="dashboard-empty")
        rect = {node["key"]: node["rect"] for node in invalid["elements"] if node.get("key")}
        empty_x, empty_y, empty_w, empty_h = rect["dashboard-empty"]
        icon_x, icon_y, icon_w, icon_h = rect["dashboard-empty-icon"]
        self.assertAlmostEqual(icon_x + icon_w / 2, empty_x + empty_w / 2, delta=2)
        self.assertGreater(icon_y + icon_h / 2, empty_y + empty_h * 0.35)
        self.assertLess(icon_y + icon_h / 2, empty_y + empty_h * 0.65)
        self.assertEqual((self.root / "dashboards.json").read_bytes(), empty_board)
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
        self.assertFalse(visible(created, key="dashboard-empty"))
        self.expect_rendered(created, key="dashboard-widget-list")

        with sqlite3.connect(self.db) as connection:
            connection.execute("UPDATE customers SET customer_name='dashboard-fresh' WHERE id=2")
        refreshed = self.replay([*start, tap(4.5, "widget-refresh")], until=6.5)
        self.expect_rendered(refreshed, text="fixture-ada 1", within="dashboards-page")
        self.expect_rendered(refreshed, text="dashboard-fresh 2", within="dashboards-page")
        self.assertFalse(visible(refreshed, text="fixture-lin 2", within="dashboards-page"))
        self.assertEqual(json.loads((self.root / "dashboards.json").read_text())["widgets"], widgets)
        workflow = self.replay([tap(.5, "rail-workflows")], until=2.5)
        self.expect_rendered(workflow, key="workflow-list-empty")

    def test_legacy_dashboard_reference_prevents_deleting_shared_workflow(self):
        """Migrate UJ-10 against an actual persisted dashboard and workflow registry."""
        fixture(self.store, self.db)
        sql = "SELECT customer_name, id FROM customers ORDER BY id"
        title, shared = "Referenced Widget", "shared-e2e-workflow"
        self._create_local_dashboard_fixture()
        dashboard_start = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-dashboards")]
        created = self.replay([
            *dashboard_start, *fill(4.1, "widget-title-input", title),
            *fill(5.5, "widget-query-input", sql), tap(8, "widget-save"),
        ], until=10)
        self.expect_rendered(created, text="fixture-ada 1", within="dashboards-page")
        dashboard_file = self.root / "dashboards.json"
        registry = self.root / "workflows.json"
        dashboards = json.loads(dashboard_file.read_text(encoding="utf-8"))
        (widget,) = dashboards["widgets"]
        (hidden,) = json.loads(registry.read_text(encoding="utf-8"))
        self.assertEqual(widget["workflowId"], hidden["id"])
        self.assertEqual(widget["title"], title)
        widget_id = int(widget["id"].replace("-", "")[:16], 16)

        # The original fixture creates a widget reference through the old
        # desktop IPC. Use the same persisted foreign key with the native
        # dashboard writer's actual record shape. This journey covers the
        # deletion guard, not the distinct create-from-workflow UI journey.
        def record(name):
            return {
                "id": name, "name": name, "description": "", "visibility": "user",
                "definitionYaml": f'id: {name}\nname: {name}\n'
                                  'description: Persisted dashboard reference fixture\n'
                                  'connection: e2e-sqlite\nsteps:\n'
                                  f'  - type: query\n    id: lookup\n    sql: {sql}\n',
                "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
            }
        unrelated = "unreferenced-e2e-workflow"
        widget["workflowId"] = shared
        dashboards["dashboards"][0]["name"] = "Shared E2E Dashboard"
        dashboard_file.write_text(json.dumps(dashboards), encoding="utf-8")
        registry.write_text(json.dumps([hidden, record(shared), record(unrelated)]), encoding="utf-8")
        initial_dashboard = dashboard_file.read_bytes()
        initial_workflows = registry.read_bytes()

        prefix = [tap(.5, "rail-workflows")]
        def deleting(name):
            return [*prefix, tap(2.5, f"workflow-{name}-delete"),
                    tap(4, "workflow-confirm-dialog-confirm")]

        refused = self.replay(deleting(shared), until=5.5)
        self.expect_rendered(refused, key=f"workflow-{shared}")
        self.expect_rendered(refused, key="workflow-operation-error")
        errors = [node.get("text", "") for node in refused["elements"]
                  if (node.get("key") or "").startswith("workflow-operation-error")]
        self.assertTrue(any(title in error and "Shared E2E Dashboard" in error for error in errors))
        self.assertEqual(registry.read_bytes(), initial_workflows)
        self.assertEqual(dashboard_file.read_bytes(), initial_dashboard)

        allowed = self.replay(deleting(unrelated), until=5.5)
        self.expect_rendered(allowed, key=f"workflow-{shared}")
        self.assertFalse(visible(allowed, key=f"workflow-{unrelated}"))
        self.assertFalse(visible(allowed, key="workflow-operation-error"))
        self.assertEqual([item["id"] for item in json.loads(registry.read_bytes())],
                         [hidden["id"], shared])
        self.assertEqual(dashboard_file.read_bytes(), initial_dashboard)
        restored = self.replay([*dashboard_start, tap(4.5, "widget-refresh")], until=6.5)
        self.expect_rendered(
            restored, text=f"#{widget_id} · {title} · bar", within=f"widget-head-{widget_id}",
        )
        self.expect_rendered(restored, text="fixture-lin 2", within="dashboards-page")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            self.assertEqual(database.execute(sql).fetchall(), [("fixture-ada", 1), ("fixture-lin", 2)])
