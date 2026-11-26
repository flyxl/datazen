"""Service-free migration of workflow editor and execution journeys."""
import json
import sqlite3

from offline import key
from scenarios import fixture, tap, type_text, visible


class WorkflowJourney:
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
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            after = database.execute("SELECT * FROM customers ORDER BY id").fetchall()
        self.assertEqual(after, [(1, "first-step-committed"), (2, "fixture-lin")])
