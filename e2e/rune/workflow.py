"""Service-free migration of workflow editor and execution journeys."""
import json
import re
import sqlite3
from datetime import datetime, timedelta, timezone

from offline import key
from scenarios import fixture, tap, type_text, visible


class WorkflowJourney:
    def test_settings_behavior_finite_workflow_retention_requires_consent(self):
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        history = self.root / "workflow-run-history.json"
        sql_history = self.root / "query-history.json"
        definitions = self.root / "workflows.json"
        now = datetime.now(timezone(timedelta(hours=5, minutes=30)))
        rows = [
            {"id": f"run-{age}", "workflowId": "retention-fixture", "workflowName": "Retention fixture",
             "variables": {}, "result": {"success": True, "finalOutput": "", "steps": [], "totalTimeMs": 6},
             "createdAt": (now - timedelta(days=age)).isoformat()}
            for age in (100, 45, 10, 1)
        ]
        fixture_bytes = json.dumps(rows, indent=2).encode()
        decoy = b" [ ] \n"
        sql_history.write_bytes(decoy)
        definitions.write_bytes(decoy)
        history.write_bytes(fixture_bytes)
        start = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-behavior")]
        ask = [*start, tap(2.3, "data-cleanup-workflow-run")]
        pending = self.replay(ask, until=3.2)
        self.expect_rendered(pending, key="data-cleanup-workflow-scope")
        self.expect_rendered(pending, key="data-cleanup-workflow-confirm")
        for unavailable in ("data-cleanup-scope-query", "settings-safe-mode"):
            self.assertFalse(visible(pending, key=unavailable))
        self.assertTrue(any("UTC" in e.get("text", "") and "Query history and workflow definitions" in e.get("text", "")
                            for e in pending["elements"] if e.get("key") == "data-cleanup-workflow-confirm-message"))
        self.assertEqual(history.read_bytes(), fixture_bytes)
        for dismiss in (tap(3.55, "data-cleanup-workflow-cancel"), key(3.55, "Escape")):
            canceled = self.replay([*ask, dismiss], until=4.55)
            self.assertFalse(visible(canceled, key="data-cleanup-workflow-confirm"))
            self.assertEqual(history.read_bytes(), fixture_bytes)

        for days, kept in ((7, ["run-1"]), (30, ["run-10", "run-1"]), (90, ["run-45", "run-10", "run-1"])):
            with self.subTest(preset=days):
                history.write_bytes(fixture_bytes)
                retained = self.replay([
                    *start, tap(2.3, f"data-cleanup-preset-{days}"),
                    tap(3.05, "data-cleanup-workflow-run"),
                    tap(4.0, "data-cleanup-workflow-accept"),
                ], until=5.05)
                self.expect_rendered(retained, key="data-cleanup-workflow-success")
                self.assertEqual([e["id"] for e in json.loads(history.read_bytes())], kept)
                self.assertEqual(sql_history.read_bytes(), decoy)
                self.assertEqual(definitions.read_bytes(), decoy)

        reopened = self.replay(
            [tap(.5, "rail-workflows"), tap(2, "workflow-side-tab-history")], until=3.3)
        self.expect_rendered(reopened, key="workflow-history-list-item-run-45")
        self.assertFalse(visible(reopened, key="workflow-history-list-item-run-100"))

        # New modes use the same real adapter but cannot bypass consent,
        # broaden a malformed finite draft, or touch any other store.
        for target in ("data-cleanup-scope-workflow", "data-cleanup-custom-days", "data-cleanup-clear-all"):
            self.expect_rendered(pending, key=target)
        history.write_bytes(fixture_bytes)
        no_scope = self.replay([*start, tap(2.3, "data-cleanup-scope-workflow"),
            tap(3.05, "data-cleanup-workflow-run")], until=4.0)
        self.expect_rendered(no_scope, key="data-cleanup-workflow-error")
        self.assertFalse(visible(no_scope, key="data-cleanup-workflow-confirm"))
        self.assertEqual(history.read_bytes(), fixture_bytes)

        def custom(value):
            return [*start, tap(2.3, "data-cleanup-custom-days"), key(2.6, "a", cmd=True),
                    type_text(2.9, value), tap(3.65, "data-cleanup-workflow-run")]

        invalid = self.replay(custom("3"), until=4.5)
        self.expect_rendered(invalid, key="data-cleanup-workflow-error")
        self.assertFalse(visible(invalid, key="data-cleanup-workflow-confirm"))
        self.assertEqual(history.read_bytes(), fixture_bytes)
        changed = self.replay([*custom("12"), tap(4.45, "data-cleanup-workflow-accept")], until=5.4)
        self.expect_rendered(changed, key="data-cleanup-workflow-success")
        self.assertEqual([e["id"] for e in json.loads(history.read_bytes())], ["run-10", "run-1"])

        history.write_bytes(fixture_bytes)
        clear = [*start, tap(2.3, "data-cleanup-clear-all"), tap(3.05, "data-cleanup-workflow-run")]
        clear_dialog = self.replay(clear, until=3.8)
        self.assertTrue(any("all local workflow runs" in e.get("text", "")
                            for e in clear_dialog["elements"]
                            if e.get("key") == "data-cleanup-workflow-confirm-message"))
        self.assertEqual(history.read_bytes(), fixture_bytes)
        canceled = self.replay([*clear, key(4.0, "Escape")], until=4.9)
        self.assertFalse(visible(canceled, key="data-cleanup-workflow-confirm"))
        self.assertEqual(history.read_bytes(), fixture_bytes)
        cleared = self.replay([*clear, tap(4.0, "data-cleanup-workflow-accept")], until=4.9)
        self.expect_rendered(cleared, key="data-cleanup-workflow-success")
        self.assertEqual(json.loads(history.read_bytes()), [])
        self.assertEqual(sql_history.read_bytes(), decoy)
        self.assertEqual(definitions.read_bytes(), decoy)

    def test_settings_behavior_light_numeric_arrows_only_change_the_confirmed_draft(self):
        (self.root / "settings.json").write_text(
            '{"ui":{"language":"en","themeMode":"light"}}', encoding="utf-8")
        history = self.root / "workflow-run-history.json"
        query = self.root / "query-history.json"
        now = datetime.now(timezone.utc)
        rows = [
            {"id": f"run-{age}", "workflowId": "numeric-flow", "workflowName": "Numeric fixture",
             "variables": {}, "result": {"success": True, "finalOutput": "", "steps": [], "totalTimeMs": 1},
             "createdAt": (now - timedelta(days=age)).isoformat()}
            for age in (20, 11.5, 1)
        ]
        initial = json.dumps(rows, indent=2).encode()
        sentinel = b" [ ] \n"
        history.write_bytes(initial)
        query.write_bytes(sentinel)
        draft = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-behavior"),
                 tap(2.3, "data-cleanup-custom-days"), key(2.6, "a", cmd=True),
                 type_text(2.9, "11"), key(3.35, "Up"), key(3.65, "Up"), key(3.95, "Down")]
        confirmation = [*draft, tap(4.6, "data-cleanup-workflow-run")]
        canceled = self.replay([*confirmation, key(5.4, "Escape")], until=6.1)
        self.expect_rendered(canceled, within="data-cleanup-custom-days", text="12")
        self.assertFalse(visible(canceled, key="data-cleanup-workflow-confirm"))
        self.assertEqual(history.read_bytes(), initial)
        self.assertEqual(query.read_bytes(), sentinel)

        accepted = self.replay(
            [*confirmation, tap(5.4, "data-cleanup-workflow-accept")], until=6.1)
        self.expect_rendered(accepted, key="data-cleanup-workflow-success")
        self.assertEqual([entry["id"] for entry in json.loads(history.read_bytes())],
                         ["run-11.5", "run-1"])
        self.assertEqual(query.read_bytes(), sentinel)

    def test_workflow_inline_visual_form_fills_pane_and_scrolls_above_fixed_footer(self):
        settings = self.root / "settings.json"
        registry = self.root / "workflows.json"
        self.store.write_text(json.dumps({"connections": []}), encoding="utf-8")
        store_before = self.store.read_bytes()
        workflow_id = "layout-fixture"
        workflow_name = "Layout fixture"
        # The native workflow list keys its physical row actions by name.
        workflow_row = f"workflow-{workflow_name}"
        definition = (
            f"id: {workflow_id}\nname: {workflow_name}\ndescription: \"\"\nsteps:\n" +
            "".join(f"  - type: query\n    id: step-{i}\n    sql: SELECT {i}\n"
                    for i in range(1, 13))
        )
        registry.write_text(json.dumps([{
            "id": workflow_id, "name": workflow_name, "description": "",
            "visibility": "user", "definitionYaml": definition,
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
        }]), encoding="utf-8")
        registry_before = registry.read_bytes()

        def rect(tree, key, *, optional=False):
            nodes = [node for node in tree["elements"]
                     if node.get("key") == key and not node.get("exiting")
                     and node.get("opacity", 0) > 0]
            if optional and not nodes:
                return None
            self.assertEqual(len(nodes), 1, key)
            return nodes[0]["rect"]

        def inside(child, container):
            if child is None:
                return False
            x, y, w, h = child
            xx, yy, ww, hh = container
            return (w > 0 and h > 0 and x >= xx - 0.25 and y >= yy - 0.25
                    and x + w <= xx + ww + 0.25 and y + h <= yy + hh + 0.25)

        def geometry(tree, viewport, sidebar, middle):
            self.assertEqual(tree["viewport"], viewport)
            self.assertAlmostEqual(rect(tree, "workflow-sidebar")[2], sidebar, delta=0.25)
            pane = rect(tree, "workflow-edit-panel")
            main = rect(tree, "workflow-main")
            tabs = rect(tree, "workflow-editor-mode-tabs")
            body = rect(tree, middle)
            footer = rect(tree, "workflow-editor-action-bar")
            self.assertTrue(inside(pane, (0, 0, *viewport)), pane)
            for child in (tabs, body, footer):
                self.assertTrue(inside(child, pane), (child, pane))
            self.assertAlmostEqual(tabs[1], pane[1], delta=0.25)
            self.assertAlmostEqual(tabs[1] + tabs[3], body[1], delta=0.25)
            self.assertAlmostEqual(body[1] + body[3], footer[1], delta=0.25)
            self.assertAlmostEqual(footer[1] + footer[3], pane[1] + pane[3], delta=0.25)
            self.assertAlmostEqual(footer[1] + footer[3], main[1] + main[3], delta=0.25)
            self.assertAlmostEqual(footer[1] + footer[3], viewport[1], delta=0.25)
            for key in ("workflow-editor-cancel", "workflow-yaml-save"):
                self.assertTrue(inside(rect(tree, key), footer), key)
            return tabs, body, footer

        landing = [tap(.5, "rail-workflows")]
        create = [*landing, tap(1.8, "workflow-create")]
        edit_long = [*landing, tap(1.8, f"{workflow_row}-edit")]
        scroll_long = [*edit_long, {"t": 3.0, "type": "wheel",
                                  "key": "workflow-edit-visual", "dy": -10000}]
        tail = "workflow-form-step-11-id"
        for width, height, sidebar in [(1440, 900, 256), (960, 640, 320)]:
            with self.subTest(width=width, height=height, sidebar=sidebar):
                size = f"{width}x{height}"
                viewport = [width, height]
                settings.write_text(json.dumps({"ui": {"language": "en", "layout": {
                    "workflow-sidebar-width": sidebar,
                }}}), encoding="utf-8")
                settings_before = settings.read_bytes()

                initial = self.replay(create, until=2.8, size=size)
                self.expect_rendered(initial, key=workflow_row)
                self.expect_rendered(initial, key=f"{workflow_row}-edit",
                                     within=workflow_row)
                self.expect_rendered(initial, key="workflow-tab-edit-1", within="workflow-workspace")
                _, _, create_footer = geometry(initial, viewport, sidebar, "workflow-edit-visual")
                yaml = self.replay([*create, tap(3.0, "workflow-mode-yaml")], until=4.2, size=size)
                self.expect_rendered(yaml, key="workflow-yaml-editor")
                self.assertFalse(visible(yaml, key="workflow-edit-visual"))
                _, _, yaml_footer = geometry(yaml, viewport, sidebar, "workflow-edit-yaml")
                self.assertEqual(create_footer, yaml_footer)

                long = self.replay(edit_long, until=2.8, size=size)
                top_geometry = geometry(long, viewport, sidebar, "workflow-edit-visual")
                self.assertFalse(inside(rect(long, tail, optional=True), top_geometry[1]))
                bottom = self.replay(scroll_long, until=4.2, size=size)
                bottom_geometry = geometry(bottom, viewport, sidebar, "workflow-edit-visual")
                self.assertEqual(top_geometry, bottom_geometry, "scroll must not move the footer")
                self.assertTrue(inside(rect(bottom, tail), bottom_geometry[1]),
                                "the wheel must reveal the real last editable step")

                # The page owns this anchored overlay, not the clipped form.
                # Open it on the real last editable row after the wheel.
                select = "workflow-form-step-11-type"
                trigger = f"{select}-trigger"
                popup = f"{select}-content"
                open_select = [*scroll_long, tap(4.35, trigger)]
                opened = self.replay(open_select, until=5.5, size=size)
                trigger_rect = rect(opened, trigger)
                popup_rect = rect(opened, popup)
                self.assertTrue(inside(trigger_rect, bottom_geometry[1]), trigger_rect)
                self.assertTrue(inside(popup_rect, (0, 0, *viewport)), popup_rect)
                self.assertFalse(visible(opened, key=popup, within="workflow-edit-visual"))
                self.expect_rendered(opened, key="listbox", within=popup)
                self.expect_rendered(opened, text="Command", within=popup)
                self.expect_rendered(opened, text="Query (legacy)", within=trigger)
                # The Select uses BottomEnd placement, including when vertically flipped.
                self.assertAlmostEqual(popup_rect[0] + popup_rect[2],
                                       trigger_rect[0] + trigger_rect[2], delta=1.0)
                vertical_gaps = (popup_rect[1] - (trigger_rect[1] + trigger_rect[3]),
                                 trigger_rect[1] - (popup_rect[1] + popup_rect[3]))
                self.assertTrue(any(abs(gap - 2.0) <= 1.0 for gap in vertical_gaps),
                                (trigger_rect, popup_rect))

                close_select = [*open_select, key(5.65, "Escape")]
                dismissed = self.replay(close_select, until=6.8, size=size)
                self.assertFalse(visible(dismissed, key=popup))
                self.expect_rendered(dismissed, text="Query (legacy)", within=trigger)
                self.assertEqual(geometry(dismissed, viewport, sidebar, "workflow-edit-visual"),
                                 bottom_geometry)

                canceled = self.replay([*close_select, tap(6.95, "workflow-editor-cancel")],
                                       until=8.1, size=size)
                self.expect_rendered(canceled, key="workflow-empty-hint")
                self.expect_rendered(canceled, key=workflow_row)
                self.assertFalse(visible(canceled, key="workflow-edit-panel"))
                self.assertAlmostEqual(rect(canceled, "workflow-sidebar")[2], sidebar, delta=0.25)
                self.assertEqual(registry.read_bytes(), registry_before)
                self.assertEqual(self.store.read_bytes(), store_before)
                self.assertEqual(settings.read_bytes(), settings_before)

    def test_workflow_yaml_validation_save_and_execute_after_relaunch(self):
        fixture(self.store, self.db)
        name = "e2e-workflow"
        sql = "UPDATE customers SET customer_name='workflow-wrote' WHERE id=1"
        start = [tap(.5, "rail-workflows"), tap(1.8, "workflow-create"),
                 tap(3, "workflow-mode-yaml")]
        invalid = self.replay([*start, tap(4, "workflow-yaml-save")], until=5.2)
        self.expect_rendered(invalid, key="workflow-feedback")
        self.expect_rendered(invalid, key="workflow-yaml-editor")
        self.assertFalse((self.root / "workflows.json").exists())

        document = f"name: {name}\nsteps:\n  - {sql}\n"
        saved = self.replay([
            *start, tap(3.8, "yaml-editor-scroll"), key(4.1, "a", cmd=True),
            type_text(4.4, document), tap(7.8, "workflow-yaml-save"),
        ], until=9.1)
        self.expect_rendered(saved, key=f"workflow-{name}")
        self.assertFalse(visible(saved, key="workflow-yaml-editor"))
        stored = json.loads((self.root / "workflows.json").read_text())
        self.assertIn(sql, json.dumps(stored))
        with sqlite3.connect(self.db) as database:
            self.assertEqual(database.execute(
                "SELECT customer_name FROM customers WHERE id=1").fetchone(), ("fixture-ada",))

        ran = self.replay([
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
            tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
        ], until=9)
        self.expect_rendered(ran, key="workflow-run-status")
        self.assertTrue(any(node.get("text", "").startswith("✓")
                            for node in ran["elements"] if node.get("key") == "workflow-run-status"))
        with sqlite3.connect(self.db) as database:
            values = database.execute("SELECT customer_name FROM customers ORDER BY id").fetchall()
            self.assertEqual(values, [("workflow-wrote",), ("fixture-lin",)])

    def test_workflow_step_failure_stops_later_sql_and_retains_prior_commits(self):
        fixture(self.store, self.db)
        name = "e2e-failed-workflow"
        document = """name: e2e-failed-workflow
steps:
  - UPDATE customers SET customer_name='first-step-committed' WHERE id=1
  - SELECT * FROM fixture_missing_table
  - UPDATE customers SET customer_name='third-step-must-not-run' WHERE id=2
"""
        saved = self.replay([
            tap(.5, "rail-workflows"), tap(1.8, "workflow-create"),
            tap(3, "workflow-mode-yaml"), tap(3.8, "yaml-editor-scroll"),
            key(4.1, "a", cmd=True), type_text(4.4, document),
            tap(12.0, "workflow-yaml-save"),
        ], until=13.4)
        self.expect_rendered(saved, key=f"workflow-{name}")
        with sqlite3.connect(self.db) as database:
            before = database.execute("SELECT * FROM customers ORDER BY id").fetchall()
        self.assertEqual(before, [(1, "fixture-ada"), (2, "fixture-lin")])

        result = self.replay([
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
            tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
        ], until=9.7)
        self.assertTrue(any(node.get("text", "").startswith("✗")
                            for node in result["elements"] if node.get("key") == "workflow-run-status"))
        # SW-06: the shared engine's failed step and concrete SQLite error.
        self.expect_rendered(result, key="workflow-step-detail-id", text="s2")
        self.expect_rendered(result, key="workflow-step-s2-status", text="✗")
        self.expect_rendered(result, key="workflow-step-error-text",
                             within="workflow-step-error")
        error = next(node.get("text", "") for node in result["elements"]
                     if node.get("key") == "workflow-step-error-text")
        self.assertIn("fixture_missing_table", error)
        self.assertFalse(visible(result, key="workflow-step-s3-status", text="✓"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            after = database.execute("SELECT * FROM customers ORDER BY id").fetchall()
        self.assertEqual(after, [(1, "first-step-committed"), (2, "fixture-lin")])


    def test_legacy_workflow_passes_live_query_fields_to_next_step(self):
        """Migrate the live rows[0].field reference from SW-04b and SW-04c."""
        fixture(self.store, self.db)
        name = "e2e-workflow-step-values"
        upstream = "SELECT customer_name AS label, score + 7 AS points FROM customers WHERE id=1"
        label = "{{steps.s1.rows[0].label}}"
        points = "{{steps.s1.rows[0].points}}"
        downstream = (f"UPDATE customers SET customer_name='{label}', score={points} "
                      f"WHERE id=2 AND typeof({points})='integer'")
        document = f"name: {name}\nsteps:\n  - {upstream}\n  - {downstream}\n"
        with sqlite3.connect(self.db) as database:
            database.executescript(
                "ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0;"
                "UPDATE customers SET score=11 WHERE id=1;"
                "INSERT INTO customers VALUES (3, 'fixture-decoy', 99);")

        saved = self.replay([
            tap(.5, "rail-workflows"), tap(1.8, "workflow-create"),
            tap(3, "workflow-mode-yaml"), tap(3.8, "yaml-editor-scroll"),
            key(4.1, "a", cmd=True), type_text(4.4, document),
            tap(4.4 + len(document) * .027 + 2, "workflow-yaml-save"),
        ], until=4.4 + len(document) * .027 + 3.6)
        self.expect_rendered(saved, key=f"workflow-{name}")
        self.assertFalse(visible(saved, key="workflow-yaml-editor"))
        stored = json.loads((self.root / "workflows.json").read_text(encoding="utf-8"))
        self.assertEqual(len(stored), 1)
        self.assertEqual(stored[0]["name"], name)
        definition = stored[0]["definitionYaml"]
        self.assertEqual(re.findall(r"^[ \t]*id: (s[0-9]+)[ \t]*$", definition, re.MULTILINE),
                         ["s1", "s2"])
        self.assertIn(label, definition)
        self.assertIn(points, definition)

        # Change the source after saving to prove execution uses live rows,
        # not values captured by the editor or a fixture precomputed at save.
        rows_sql = "SELECT id, customer_name, score, typeof(score) FROM customers ORDER BY id"
        with sqlite3.connect(self.db) as database:
            self.assertEqual(database.execute(rows_sql).fetchall(),
                             [(1, "fixture-ada", 11, "integer"),
                              (2, "fixture-lin", 0, "integer"),
                              (3, "fixture-decoy", 99, "integer")])
            database.execute("UPDATE customers SET customer_name=?, score=? WHERE id=1",
                             ("夜班 O'Neil", 35))
            expected = database.execute(upstream).fetchone()
        self.assertEqual(expected, ("夜班 O'Neil", 42))

        ran = self.replay([
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
            tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
        ], until=10.4)
        self.expect_rendered(ran, key="workflow-run-status")
        self.assertTrue(any(node.get("text", "").startswith("✓")
                            for node in ran["elements"] if node.get("key") == "workflow-run-status"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            self.assertEqual(database.execute(rows_sql).fetchall(),
                             [(1, "夜班 O'Neil", 35, "integer"),
                              (2, expected[0], expected[1], "integer"),
                              (3, "fixture-decoy", 99, "integer")])


    def test_legacy_workflow_confirmed_delete_preserves_unrelated_definition(self):
        """The save/list/delete slice of SW-01 through the real native host."""
        target = "e2e-workflow-delete-target"
        decoy = "e2e-workflow-delete-keep"
        store = self.root / "workflows.json"

        def create(name, marker):
            document = f"name: {name}\nsteps:\n  - SELECT {marker} AS saved_marker\n"
            return self.replay([
                tap(.5, "rail-workflows"), tap(1.8, "workflow-create"),
                tap(3, "workflow-mode-yaml"), tap(3.8, "yaml-editor-scroll"),
                key(4.1, "a", cmd=True), type_text(4.4, document),
                tap(7.8, "workflow-yaml-save"),
            ], until=9.1)

        self.expect_rendered(create(target, 32), key=f"workflow-{target}")
        with_both = create(decoy, 81)
        self.expect_rendered(with_both, key=f"workflow-{target}")
        self.expect_rendered(with_both, key=f"workflow-{decoy}")
        records = {row["name"]: row for row in json.loads(store.read_text(encoding="utf-8"))}
        self.assertEqual(set(records), {target, decoy})
        self.assertIn("SELECT 32 AS saved_marker", records[target]["definitionYaml"])
        self.assertIn("SELECT 81 AS saved_marker", records[decoy]["definitionYaml"])
        original = store.read_bytes()

        ask = [tap(.5, "rail-workflows"), tap(2.3, f"workflow-{target}-delete")]
        confirm_panel = self.replay(ask, until=3.8)
        self.expect_rendered(confirm_panel, key="workflow-confirm-dialog")
        self.expect_rendered(confirm_panel, key="workflow-confirm-dialog-confirm")
        self.assertEqual(store.read_bytes(), original)

        canceled = self.replay([*ask, tap(4, "workflow-confirm-dialog-cancel")], until=5.3)
        self.assertFalse(visible(canceled, key="workflow-confirm-dialog"))
        self.expect_rendered(canceled, key=f"workflow-{target}")
        self.assertEqual(store.read_bytes(), original)

        deleted = self.replay([*ask, tap(4, "workflow-confirm-dialog-confirm")], until=5.8)
        self.assertFalse(visible(deleted, key=f"workflow-{target}"))
        self.assertFalse(visible(deleted, key="workflow-operation-error"))
        self.expect_rendered(deleted, key=f"workflow-{decoy}")
        self.assertEqual(json.loads(store.read_text(encoding="utf-8")), [records[decoy]])

        reopened = self.replay([tap(.5, "rail-workflows")], until=2.5)
        self.assertFalse(visible(reopened, key=f"workflow-{target}"))
        self.expect_rendered(reopened, key=f"workflow-{decoy}")
        self.assertEqual(json.loads(store.read_text(encoding="utf-8")), [records[decoy]])


    def test_legacy_workflow_window_switches_live_steps_and_closes_run_tab(self):
        """Migrate the original workflow run-tab and step-navigation journey."""
        fixture(self.store, self.db)
        name = "e2e-workflow-run-tabs"
        first = "SELECT customer_name AS person, id AS customer_id FROM customers ORDER BY id"
        second = "SELECT id AS chosen FROM customers WHERE id=2"
        document = f"name: {name}\nsteps:\n  - {first}\n  - {second}\n"
        save_at = 4.4 + .027 * len(document) + 2
        saved = self.replay([
            tap(.5, "rail-workflows"), tap(1.8, "workflow-create"),
            tap(3, "workflow-mode-yaml"), tap(3.8, "yaml-editor-scroll"),
            key(4.1, "a", cmd=True), type_text(4.4, document),
            tap(save_at, "workflow-yaml-save"),
        ], until=save_at + 1.6)
        self.expect_rendered(saved, key=f"workflow-{name}")
        fixture_uri = f"{self.db.as_uri()}?mode=ro"
        with sqlite3.connect(fixture_uri, uri=True) as db:
            self.assertEqual(db.execute(first).fetchall(), [("fixture-ada", 1), ("fixture-lin", 2)])
            self.assertEqual(db.execute(second).fetchall(), [(2,)])

        opened = [
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
            tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
        ]
        result = self.replay(opened, until=9)
        self.expect_rendered(result, key="workflow-tab-wf-1")
        self.expect_rendered(result, key="workflow-tab-wf-1-close")
        self.expect_rendered(result, key="workflow-steps")
        # The successful-run actions are compact neighbors, not a full-width
        # Execute followed by a second action band. Both are real controls.
        band = self._settled_rect(result, "workflow-run-actions")
        execute = self._settled_rect(result, "workflow-execute-button")
        dashboard = self._settled_rect(result, "workflow-add-to-dashboard")
        panel_tabs = self._settled_rect(result, "workflow-panel-tabs")
        variables = self._settled_rect(result, "workflow-variables")
        self.assertAlmostEqual(panel_tabs[3], 33, delta=1)
        self.assertAlmostEqual(panel_tabs[1] + panel_tabs[3], variables[1], delta=1)
        self.assertAlmostEqual(execute[1] - variables[1], 12, delta=1)
        status = self._settled_rect(result, "workflow-run-status-row")
        order = self._settled_rect(result, "workflow-step-order-toggle")
        steps_band = self._settled_rect(result, "workflow-steps")
        self.assertAlmostEqual(status[3], 30, delta=1)
        self.assertAlmostEqual(order[3], 20.5, delta=1)
        self.assertAlmostEqual(status[1], variables[1] + variables[3], delta=1)
        self.assertAlmostEqual(steps_band[1], status[1] + status[3], delta=1)
        self.assertAlmostEqual(execute[0], band[0], delta=1)
        self.assertAlmostEqual(execute[1], dashboard[1], delta=1)
        self.assertAlmostEqual(execute[3], 32, delta=1)
        self.assertAlmostEqual(dashboard[3], 32, delta=1)
        self.assertAlmostEqual(dashboard[0] - execute[0] - execute[2], 8, delta=1)
        self.assertLess(execute[2], band[2] / 2)
        # Default descending order means the first displayed step (s2) is
        # already selected; the Run control is above the step navigation.
        self.expect_rendered(result, key="workflow-step-detail-id", text="s2")
        positions = {node["key"]: node["rect"] for node in result["elements"]
             if node.get("key") in {"workflow-step-s1", "workflow-step-s2",
                                    "workflow-execute-button"}}
        self.assertLess(positions["workflow-execute-button"][1], positions["workflow-step-s2"][1])
        self.assertLess(positions["workflow-step-s2"][0], positions["workflow-step-s1"][0])
        for step in ("s1", "s2"):
            self.expect_rendered(result, key=f"workflow-step-{step}")
            self.expect_rendered(result, key=f"workflow-step-{step}-status", text="✓")
        self.assertFalse(visible(result, key="workflow-empty-hint"))

        from_second = self.replay([*opened, tap(8.7, "workflow-step-s2")], until=10.0)
        self.expect_rendered(from_second, key="workflow-step-detail-id", text="s2")
        self.expect_rendered(from_second, key="workflow-step-sql-text", text=second)
        self.expect_rendered(from_second, key="workflow-step-table-index-header", text="#")
        self.expect_rendered(from_second, key="workflow-step-table-row-index-0", text="1")
        head = self._settled_rect(from_second, "workflow-step-table-head")
        first_row = self._settled_rect(from_second, "workflow-step-table-row-0")
        chosen = self._settled_rect(from_second, "workflow-step-table-col-0")
        self.assertAlmostEqual(head[3], 40, delta=1)
        self.assertAlmostEqual(first_row[3], 32, delta=1)
        self.assertAlmostEqual(first_row[1], head[1] + head[3], delta=1)
        chosen_value = self._settled_rect(from_second, "workflow-step-table-cell-0-0")
        self.assertAlmostEqual(chosen[0] - head[0], 52, delta=2)
        self.assertAlmostEqual(chosen[0], chosen_value[0], delta=1)
        self.expect_rendered(from_second, key="workflow-step-table-col-0", text="chosen")
        self.expect_rendered(from_second, key="workflow-step-table-cell-0-0", text="2")
        self.assertFalse(visible(from_second, key="workflow-step-table-row-1"))
        switched = self.replay([
            *opened, tap(8.7, "workflow-step-s2"), tap(10.3, "workflow-step-s1"),
        ], until=11.5)
        self.expect_rendered(switched, key="workflow-step-detail-id", text="s1")
        self.expect_rendered(switched, key="workflow-step-sql-text", text=first)
        self.expect_rendered(switched, key="workflow-step-table-col-0", text="person")
        self.expect_rendered(switched, key="workflow-step-table-cell-0-0", text="fixture-ada")
        self.expect_rendered(switched, key="workflow-step-table-row-index-1", text="2")
        self.expect_rendered(switched, key="workflow-step-table-cell-1-0", text="fixture-lin")

        self.expect_rendered(switched, key="workflow-step-table-col-1", text="customer_id")
        self.expect_rendered(switched, key="workflow-step-table-cell-1-1", text="2")
        grid = self._settled_rect(switched, "workflow-step-table")
        first_col = self._settled_rect(switched, "workflow-step-table-col-slot-0")
        second_col = self._settled_rect(switched, "workflow-step-table-col-slot-1")
        second_label = self._settled_rect(switched, "workflow-step-table-col-1")
        second_value = self._settled_rect(switched, "workflow-step-table-cell-1-1")
        self.assertAlmostEqual(first_col[2], 150, delta=1)
        self.assertAlmostEqual(second_col[0], first_col[0] + first_col[2], delta=1)
        self.assertAlmostEqual(second_label[0], second_value[0], delta=1)
        self.assertLess(second_col[0] + second_col[2], grid[0] + grid[2] - 40)

        # React mounts one selected-step detail, not the aggregate history
        # report. Check the actual tree, not just visibility below the fold.
        for snapshot in (result, from_second, switched):
            keys = [node.get("key") for node in snapshot["elements"]]
            self.assertEqual(keys.count("workflow-step-detail"), 1)
            self.assertNotIn("workflow-execution-result", keys)
            self.assertNotIn("workflow-panel-content-scroll", keys)

        closed = self.replay([*opened, tap(9, "workflow-tab-wf-1-close")], until=10.8)
        self.expect_rendered(closed, key="workflow-empty-hint")
        self.expect_rendered(closed, key=f"workflow-{name}")
        for gone in ("workflow-tab-wf-1", "workflow-steps", "workflow-step-detail"):
            self.assertFalse(visible(closed, key=gone))
        with sqlite3.connect(fixture_uri, uri=True) as db:
            self.assertEqual(db.execute(first).fetchall(), [("fixture-ada", 1), ("fixture-lin", 2)])

    def test_legacy_workflow_binds_required_values_per_run_tab(self):
        """Migrate SW-04: run the native registry's declared variable against real SQLite."""
        fixture(self.store, self.db)
        first, other = "e2e-variable-main", "e2e-variable-default"
        with sqlite3.connect(self.db) as database:
            database.executescript(
                "ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0;"
                "INSERT INTO customers VALUES (3, 'fixture-decoy', 99);")

        def record(name, row, default):
            default_line = f"\n    default: {json.dumps(default)}" if default else ""
            definition = (
                f"id: {name}\nname: {name}\ndescription: Native run bindings\n"
                "variables:\n"
                "  - name: val\n    type: string\n    description: Customer name\n"
                f"    required: true{default_line}\n"
                "  - name: score\n    type: number\n    description: Numeric score\n"
                "    required: true\n    default: 14\n"
                "steps:\n  - type: query\n    id: update\n    sql: >-\n"
                "      UPDATE customers SET customer_name='{{val}}', score={{score}}\n"
                f"      WHERE id={row} AND typeof({{{{score}}}})='integer'"
            )
            return {"id": name, "name": name, "description": "Native run bindings",
                    "visibility": "user", "definitionYaml": definition,
                    "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"}

        # The prior suite seeded the definition through workflow_save IPC.
        # Here the native production registry loads equivalent records from its
        # real store. This does not assert unsupported visual authoring.
        registry = self.root / "workflows.json"
        registry.write_text(json.dumps([record(first, 1, None),
                                        record(other, 2, "default-lin")]), encoding="utf-8")
        start = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
                 tap(4.5, f"workflow-{first}")]
        missing = self.replay([*start, tap(6, "workflow-execute-button")], until=9)
        self.expect_rendered(missing, key="workflow-variable-val-required")
        self.expect_rendered(missing, key="workflow-operation-error-text")
        text = next(node.get("text", "") for node in missing["elements"]
                    if node.get("key") == "workflow-operation-error-text")
        self.assertIn("val", text)
        self.assertFalse(visible(missing, key="workflow-run-status"))
        select = "SELECT id, customer_name, score, typeof(score) FROM customers ORDER BY id"
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            self.assertEqual(database.execute(select).fetchall(),
                             [(1, "fixture-ada", 0, "integer"),
                              (2, "fixture-lin", 0, "integer"),
                              (3, "fixture-decoy", 99, "integer")])

        label = "夜班 O'Neil"
        run = self.replay([
            *start, tap(6, "workflow-variable-val"), type_text(6.4, label),
            tap(7.9, "workflow-variable-score"), key(8.2, "a", cmd=True), type_text(8.5, "42"),
            tap(10, f"workflow-{other}"), tap(11.5, "workflow-execute-button"),
            tap(15, f"workflow-{first}"), tap(16.5, "workflow-execute-button"),
        ], until=20.5)
        self.expect_rendered(run, key="workflow-run-status")
        self.expect_rendered(run, key="workflow-step-detail-id", text="update")
        self.expect_rendered(run, key="workflow-step-sql-text")
        executed = next(node.get("text", "") for node in run["elements"]
                        if node.get("key") == "workflow-step-sql-text")
        self.assertIn("O''Neil", executed)
        self.assertFalse(visible(run, key="workflow-operation-error-text"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            self.assertEqual(database.execute(select).fetchall(),
                             [(1, label, 42, "integer"),
                              (2, "default-lin", 14, "integer"),
                              (3, "fixture-decoy", 99, "integer")])

    def test_legacy_workflow_condition_routes_on_live_sqlite_rows(self):
        """Migrate SW-05 using both determinate branches and a real query result."""
        fixture(self.store, self.db)
        name = "e2e-native-condition"
        definition = f"""id: {name}
name: {name}
description: Run a branch from a live SQL count
steps:
  - type: query
    id: count_check
    sql: SELECT id FROM customers WHERE id=1 AND customer_name='choose-then'
  - type: condition
    id: branch
    if: steps.count_check.rows_count > 0
    then_steps:
      - type: query
        id: when_true
        sql: UPDATE customers SET customer_name='then-committed' WHERE id=2
    else_steps:
      - type: query
        id: when_false
        sql: UPDATE customers SET customer_name='else-committed' WHERE id=2
"""
        registry = self.root / "workflows.json"
        registry.write_text(json.dumps([{
            "id": name, "name": name, "description": "Run a branch from a live SQL count",
            "visibility": "user", "definitionYaml": definition,
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
        }]), encoding="utf-8")
        select = "SELECT id, customer_name FROM customers ORDER BY id"
        source = "SELECT id FROM customers WHERE id=1 AND customer_name='choose-then'"
        fixture_uri = f"{self.db.as_uri()}?mode=ro"
        with sqlite3.connect(self.db) as database:
            database.execute("UPDATE customers SET customer_name='choose-then' WHERE id=1")
            database.execute("INSERT INTO customers VALUES (3, 'fixture-decoy')")
            self.assertEqual(database.execute(source).fetchall(), [(1,)])

        def run_branch(wanted, not_wanted):
            tree = self.replay([
                tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
                tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
            ], until=9.5)
            self.expect_rendered(tree, key="workflow-step-count_check-status", text="✓")
            self.expect_rendered(tree, key=f"workflow-step-{wanted}-status", text="✓")
            self.assertFalse(visible(tree, key=f"workflow-step-{not_wanted}"))

        run_branch("when_true", "when_false")
        with sqlite3.connect(fixture_uri, uri=True) as database:
            self.assertEqual(database.execute(select).fetchall(),
                             [(1, "choose-then"), (2, "then-committed"), (3, "fixture-decoy")])

        with sqlite3.connect(self.db) as database:
            database.execute("UPDATE customers SET customer_name='choose-else' WHERE id=1")
            self.assertEqual(database.execute(source).fetchall(), [])
        run_branch("when_false", "when_true")
        with sqlite3.connect(fixture_uri, uri=True) as database:
            self.assertEqual(database.execute(select).fetchall(),
                             [(1, "choose-else"), (2, "else-committed"), (3, "fixture-decoy")])

    def test_legacy_workflow_history_survives_restart_and_requires_confirmation_to_clear(self):
        """Migrate SW-03 and history tab reuse using actual SQLite and native storage."""
        fixture(self.store, self.db)
        name = "e2e-native-workflow-history"
        query = "SELECT customer_name AS observed FROM customers WHERE id=2"
        definition = (f'id: {name}\nname: {name}\ndescription: ""\nsteps:\n'
                      f'  - type: query\n    id: read_customer\n    sql: {query}\n')
        registry = self.root / "workflows.json"
        registry.write_text(json.dumps([{
            "id": name, "name": name, "description": "", "visibility": "user",
            "definitionYaml": definition, "createdAt": "2026-01-01T00:00:00Z",
            "updatedAt": "2026-01-01T00:00:00Z",
        }]), encoding="utf-8")
        history = self.root / "workflow-run-history.json"
        self.assertFalse(history.exists())

        live = self.replay([
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
            tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
        ], until=9.5)
        self.expect_rendered(live, key="workflow-step-read_customer-status", text="✓")
        self.expect_rendered(live, key="workflow-step-table-cell-0-0", text="fixture-lin")
        original_history = history.read_bytes()
        (record,) = json.loads(original_history)
        self.assertEqual(record["workflowId"], name)
        self.assertEqual(record["workflowName"], name)
        self.assertTrue(record["result"]["success"])
        self.assertGreaterEqual(record["result"]["totalTimeMs"], 0)
        (step,) = record["result"]["steps"]
        self.assertEqual(step["stepId"], "read_customer")
        self.assertEqual(step["sqlExecuted"], query)

        with sqlite3.connect(self.db) as database:
            database.execute("UPDATE customers SET customer_name='edited-after-run' WHERE id=2")
            self.assertEqual(database.execute(query).fetchone(), ("edited-after-run",))

        # No connection is opened in the restarted app: history must render the
        # saved result, not rerun the SQL or reuse an in-memory result.
        row = f"workflow-history-list-item-{record['id']}"
        start = [tap(.5, "rail-workflows"), tap(2.5, "workflow-side-tab-history")]
        opened = self.replay([*start, tap(4, row), tap(5.4, row)], until=6.6)
        self.expect_rendered(opened, key=row)
        self.expect_rendered(opened, key="workflow-tab-hist-1")
        self.assertFalse(visible(opened, key="workflow-tab-hist-2"))
        self.expect_rendered(opened, key="workflow-step-detail-id", text="read_customer")
        self.expect_rendered(opened, key="workflow-step-table-cell-0-0", text="fixture-lin")
        self.assertEqual(history.read_bytes(), original_history)

        ask = [*start, tap(4, "workflow-history-list-clear")]
        canceled = self.replay([*ask, tap(5.4, "workflow-confirm-dialog-cancel")], until=6.7)
        self.expect_rendered(canceled, key=row)
        self.assertEqual(history.read_bytes(), original_history)
        cleared = self.replay([*ask, tap(5.4, "workflow-confirm-dialog-confirm")], until=6.7)
        self.expect_rendered(cleared, key="workflow-history-list-empty")
        self.assertFalse(visible(cleared, key=row))
        self.assertEqual(json.loads(history.read_bytes()), [])
        restarted = self.replay(start, until=4)
        self.expect_rendered(restarted, key="workflow-history-list-empty")
        self.assertFalse(visible(restarted, key=row))
        self.assertEqual(json.loads(registry.read_text())[0]["id"], name)
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            self.assertEqual(database.execute(query).fetchone(), ("edited-after-run",))
