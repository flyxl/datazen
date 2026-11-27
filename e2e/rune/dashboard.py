"""Migrate the SQL-backed dashboard journey using an isolated SQLite file."""
import json
import sqlite3

from offline import fill
from scenarios import fixture, tap, visible


class DashboardJourney:
    def _expect_selected_dashboard_chrome(self, tree, report_count, *, empty_add=False):
        keys = ("dashboards-page", "dashboard-selected-board", "dashboard-selected-board-default",
                "dashboard-selected-board-indicator", "dashboard-report-actions",
                "dashboard-report-count", "widget-refresh")
        nodes = {}
        for key in keys:
            self.expect_rendered(tree, key=key)
            (nodes[key],) = [n for n in tree["elements"] if n.get("key") == key and not n.get("exiting")]
        x, y, width, _ = nodes["dashboards-page"]["rect"]
        self.assertEqual(nodes["dashboard-selected-board"]["rect"], [x, y, width, 33])
        self.assertEqual(nodes["dashboard-report-actions"]["rect"], [x, y + 33, width, 41])
        marker = nodes["dashboard-selected-board-default"]
        self.assertEqual(marker["rect"][:2], [x, y])
        self.assertEqual(marker["rect"][3], 32)
        self.assertGreater(marker["rect"][2], 0)
        self.assertLess(marker["rect"][2], width)
        self.assertFalse(marker["focusable"], "one persisted board, not a fake switch")
        self.assertNotIn(marker["role"], ("button", "tab"))
        self.assertEqual(nodes["dashboard-selected-board-indicator"]["rect"][1::2], [y + 30, 2])
        self.assertTrue(any(visible(tree, key="dashboard-report-count", text=label)
                            for label in (f"{report_count} report(s)", f"{report_count} 个报表")))
        self.assertEqual(nodes["widget-refresh"]["focusable"], report_count > 0)
        for key in ("dashboard-tab-add", "dashboard-import", "dashboard-rename",
                    "dashboard-pause-toggle", "dashboard-export", "dashboard-delete-panel"):
            self.assertFalse(visible(tree, key=key), f"unsupported native control {key}")
        if empty_add:
            self.expect_rendered(tree, key="dashboard-add-widget", within="dashboard-empty")
            (add,) = [n for n in tree["elements"] if n.get("key") == "dashboard-add-widget"
                      and not n.get("exiting")]
            self.assertEqual(add["rect"], [x + 16, y + 33 + 41 + 16, 448, 320])

    def _create_local_dashboard_fixture(self):
        created = self.replay([
            tap(.55, "rail-dashboards"), tap(2.05, "dashboard-create-first"),
        ], until=3.4)
        self.expect_rendered(created, key="dashboard-selected-board-title")
        self.expect_rendered(created, key="dashboard-empty")
        self._expect_selected_dashboard_chrome(created, 0, empty_add=True)
        self.assertFalse(visible(created, key="widget-form"))
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
        self.assertFalse(visible(empty, key="dashboard-report-actions"))
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
        self._expect_selected_dashboard_chrome(restored, 0, empty_add=True)
        self.assertFalse(visible(restored, key="widget-form"))
        self.assertFalse(visible(restored, key="dashboard-create-first"))
        self.assertEqual(path.read_bytes(), before)

        back = self.replay([
            *enter, tap(2.0, "rail-settings"), tap(3.5, "settings-back"), tap(4.9, "widget-refresh"),
        ], until=6.0)
        self.expect_rendered(back, key="dashboard-selected-board-title", text="New Dashboard")
        self.expect_rendered(back, key="dashboard-empty")
        self._expect_selected_dashboard_chrome(back, 0, empty_add=True)
        self.assertFalse(visible(back, key="widget-form"))
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse(workflows.exists())

    def test_dashboard_add_report_cancel_is_unsaved_without_database(self):
        (self.root / "settings.json").write_text(
            json.dumps({"ui": {"language": "en"}}), encoding="utf-8",
        )
        before = self._create_local_dashboard_fixture()
        path = self.root / "dashboards.json"
        open_report = [tap(.55, "rail-dashboards"), tap(2.05, "dashboard-add-widget")]
        title = "Discard draft title"
        sql = "SELECT 42"
        typed = [*open_report, *fill(3.25, "widget-title-input", title),
                 *fill(4.8, "widget-query-input", sql)]
        back = [*typed, tap(6.5, "rail-settings"), tap(8.0, "settings-back")]
        open_draft = self.replay([*back, tap(9.5, "widget-save")], until=10.8)
        self.expect_rendered(open_draft, key="dashboard-selected-board-title", text="New Dashboard")
        self.expect_rendered(open_draft, key="widget-form")
        self._expect_selected_dashboard_chrome(open_draft, 0)
        self.expect_rendered(open_draft, key="dashboard-report-needs-connection",
                             text="Open a database connection before saving this report.")
        self.expect_rendered(open_draft, key="input", text=title, within="widget-title-input")
        self.expect_rendered(open_draft, key="input", text=sql, within="widget-query-input")
        self.expect_rendered(open_draft, key="widget-cancel")
        self.assertFalse(visible(open_draft, key="dashboard-add-widget"))
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((self.root / "workflows.json").exists())

        cancelled = self.replay([*back, tap(9.5, "widget-cancel")], until=10.8)
        self.expect_rendered(cancelled, key="dashboard-add-widget", within="dashboard-empty")
        self.assertFalse(visible(cancelled, key="widget-form"))
        self.assertFalse(visible(cancelled, key="widget-form-error"))
        self.assertEqual(path.read_bytes(), before)

        reopened = self.replay(
            [*back, tap(9.5, "widget-cancel"), tap(11, "dashboard-add-widget")], until=12.4,
        )
        self.expect_rendered(reopened, key="widget-form")
        self.expect_rendered(reopened, key="input", text="", within="widget-title-input")
        self.expect_rendered(reopened, key="input", text="", within="widget-query-input")
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((self.root / "workflows.json").exists())
        self.assertFalse(self.store.exists())
        self.assertFalse(self.db.exists())

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
        self.assertFalse(visible(tree, key="dashboard-add-widget"))
        self.assertFalse(visible(tree, key="dashboard-report-actions"))
        self.assertFalse(visible(tree, key="widget-save"))
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((self.root / "workflows.json").exists())

    def test_dashboard_sql_widget_persists_and_refreshes_live_sqlite(self):
        fixture(self.store, self.db)
        title = "SQLite E2E Bars"
        sql = "SELECT customer_name, id FROM customers ORDER BY id"
        empty_board = self._create_local_dashboard_fixture()
        start = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-dashboards")]
        open_report = [*start, tap(4.2, "dashboard-add-widget")]
        invalid = self.replay([*open_report, tap(5.6, "widget-save")], until=6.9)
        self.expect_rendered(invalid, key="widget-form-error", text="a widget needs a title")
        self.expect_rendered(invalid, key="widget-form")
        self.expect_rendered(invalid, key="widget-cancel")
        self.expect_rendered(invalid, key="dashboard-empty")
        self.assertFalse(visible(invalid, key="dashboard-add-widget"))
        self.assertFalse(visible(invalid, key="dashboard-report-needs-connection"))
        self.assertEqual((self.root / "dashboards.json").read_bytes(), empty_board)
        self.assertFalse((self.root / "workflows.json").exists())

        created = self.replay([
            *open_report, *fill(5.5, "widget-title-input", title),
            *fill(7, "widget-query-input", sql), tap(9.6, "widget-save"),
        ], until=11.6)
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
        self.assertFalse(visible(created, key="widget-form"))
        self.expect_rendered(created, key="dashboard-widget-list")
        self.expect_rendered(created, key="dashboard-add-widget", within="dashboard-report-actions")
        self.expect_rendered(created, key="widget-refresh", within="dashboard-report-actions")
        self._expect_selected_dashboard_chrome(created, 1)

        with sqlite3.connect(self.db) as connection:
            connection.execute("UPDATE customers SET customer_name='dashboard-fresh' WHERE id=2")
        refreshed = self.replay([*start, tap(4.5, "widget-refresh")], until=6.5)
        self.expect_rendered(refreshed, text="fixture-ada 1", within="dashboards-page")
        self.expect_rendered(refreshed, text="dashboard-fresh 2", within="dashboards-page")
        self._expect_selected_dashboard_chrome(refreshed, 1)
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
        dashboard_start = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-dashboards"),
                           tap(4.2, "dashboard-add-widget")]
        created = self.replay([
            *dashboard_start, *fill(5.5, "widget-title-input", title),
            *fill(7, "widget-query-input", sql), tap(9.6, "widget-save"),
        ], until=11.6)
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
