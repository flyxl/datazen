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
        first = "SELECT customer_name AS person FROM customers ORDER BY id"
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
            self.assertEqual(db.execute(first).fetchall(), [("fixture-ada",), ("fixture-lin",)])
            self.assertEqual(db.execute(second).fetchall(), [(2,)])

        opened = [
            tap(.5, "conn-open-e2e-sqlite"), tap(3, "rail-workflows"),
            tap(4.5, f"workflow-{name}"), tap(6, "workflow-execute-button"),
        ]
        result = self.replay(opened, until=9)
        self.expect_rendered(result, key="workflow-tab-wf-1")
        self.expect_rendered(result, key="workflow-tab-wf-1-close")
        self.expect_rendered(result, key="workflow-steps")
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
        self.expect_rendered(switched, key="workflow-step-table-cell-1-0", text="fixture-lin")

        closed = self.replay([*opened, tap(9, "workflow-tab-wf-1-close")], until=10.8)
        self.expect_rendered(closed, key="workflow-empty-hint")
        self.expect_rendered(closed, key=f"workflow-{name}")
        for gone in ("workflow-tab-wf-1", "workflow-steps", "workflow-step-detail"):
            self.assertFalse(visible(closed, key=gone))
        with sqlite3.connect(fixture_uri, uri=True) as db:
            self.assertEqual(db.execute(first).fetchall(), [("fixture-ada",), ("fixture-lin",)])

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
