"""Service-free migration of workflow editor and execution journeys."""
import json
import re
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
