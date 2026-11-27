"""Native SQLite editor: selected/caret runs and explicit SELECT scripts."""
from contextlib import closing
import json
import sqlite3

from offline import fill, key
from scenarios import fixture, query_steps, tap, type_text, visible


class SqlSelectionJourneys:
    def sql_customer_rows(self):
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            return db.execute(
                "SELECT id, customer_name FROM customers ORDER BY id",
            ).fetchall()

    def sql_fixture_before(self):
        fixture(self.store, self.db)
        original = self.sql_customer_rows()
        self.assertEqual(original, [(1, "fixture-ada"), (2, "fixture-lin")])
        return original

    def test_native_sql_editor_keyword_completion_undo_preserves_caret_run(self):
        """Keyword selection is one undoable edit, not a query execution."""
        original = self.sql_fixture_before()
        prefix = "UPDATE customers SET customer_name='must-not-run' WHERE id=1;\n"
        steps = query_steps(prefix)[:-1]
        steps[-1] = {"t": 4.8, "type": "paste", "text": prefix}
        typed = [*steps, type_text(6.0, "s")]
        offered = self.replay(typed, until=7.0)
        for i, option in enumerate(("SELECT", "SET", "SUM")):
            self.expect_rendered(offered, text=option, within=f"sql-completion-item-{i}")

        selected = [*typed, key(7.1, "Down"), key(7.6, "Enter")]
        insertion = self.replay(selected, until=8.3)
        self.expect_rendered(insertion, text="SET", within="sql-line-1")
        self.assertFalse(visible(insertion, key="sql-completion"))
        self.assertFalse(visible(insertion, key="result-workspace"))

        undone = [*selected, key(8.6, "z", cmd=True)]
        restored = self.replay(undone, until=9.3)
        self.expect_rendered(restored, text="s", within="sql-line-1")
        self.assertFalse(visible(restored, key="sql-completion"))

        narrowed = [*undone, type_text(9.5, "e"), type_text(9.8, "l"), key(10.1, "Escape")]
        closed = self.replay(narrowed, until=10.8)
        self.expect_rendered(closed, text="sel", within="sql-line-1")
        self.assertFalse(visible(closed, key="sql-completion"))

        statement = "SELECT customer_name FROM customers WHERE id=2;"
        complete = [
            *narrowed, key(11.0, "Backspace"), type_text(11.3, "l"), key(11.7, "Enter"),
            {"t": 12.2, "type": "paste", "text": statement[len("SELECT"):]},
            key(13.2, "Enter", cmd=True),
        ]
        result = self.replay(complete, until=15.5)
        self.expect_rendered(result, text=original[1][1], within="result-cell-0-0")
        self.assertEqual(self.sql_customer_rows(), original)
        history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
        self.assertEqual([(item["success"], item["sql"].rstrip(";")) for item in history],
                         [(True, statement.rstrip(";"))])

    def test_native_sql_editor_executes_only_exact_selection_with_literal_semicolon(self):
        original = self.sql_fixture_before()
        selected = "SELECT '选中;值' AS picked"
        sql = (selected + " UNION ALL SELECT 'unselected sibling';\n"
               "UPDATE customers SET customer_name='unselected-write' WHERE id=1;")
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(selected).fetchone()[0]
        actions = [
            *query_steps(sql)[:-1],
            tap(9.0, "sql-find-open"), *fill(9.5, "sql-find-input", selected),
            tap(11.0, "sql-find-next"), key(11.5, "Escape"), tap(12.0, "sql-run"),
        ]
        tree = self.replay(actions, until=14.4)
        self.expect_rendered(tree, text=expected, within="result-workspace")
        self.expect_rendered(tree, key="result-grid-status", text="result · 1 rows × 1 cols")
        self.assertFalse(visible(tree, text="unselected sibling", within="result-workspace"))
        self.assertEqual(self.sql_customer_rows(), original)

    def test_native_sql_editor_find_dock_is_opt_in_then_focuses_next_and_escapes(self):
        """The live Query dock is scoped and search never runs or edits a draft."""
        self.sql_fixture_before()
        db_bytes = self.db.read_bytes()
        sql = "SELECT '中文';\nSELECT '中文';"
        prefix = query_steps(sql)[:-1]
        prefix[-1] = {"t": 4.8, "type": "paste", "text": sql}
        prefix.extend([key(5.8, "Home", cmd=True), key(6.2, "Home")])
        idle = self.replay(prefix, until=6.7)
        self.expect_rendered(idle, key="sql-find-open")
        self.assertFalse(visible(idle, key="sql-find-bar"))
        idle_box = self._settled_rect(idle, "sql-editor-scroll")

        opened = [*prefix, key(6.8, "f", cmd=True),
                  type_text(7.4, "中文"), key(7.9, "Enter", cmd=True),
                  key(8.3, "Enter")]
        first = self.replay(opened, until=8.9)
        self.expect_rendered(first, text="中文", key="sql-selected-0-8")
        self.assertFalse(visible(first, key="sql-selected-1-8"))
        self.assertFalse(visible(first, key="result-workspace"))
        self.assertEqual(self._settled_rect(first, "sql-editor-scroll")[1], idle_box[1])
        y, height = self._settled_rect(first, "sql-find-bar")[1::2]
        self.assertAlmostEqual(y+height, idle_box[1] + idle_box[3], delta=.1)
        self.assertAlmostEqual(height, 52.0, delta=.1)

        next_actions = [*opened, key(9.0, "F3")]
        second = self.replay(next_actions, until=9.55)
        self.expect_rendered(second, key="sql-selected-1-8", text="中文")
        self.assertFalse(visible(second, key="sql-selected-0-8"))

        restored = self.replay([*next_actions, key(9.6, "Escape")], until=10.3)
        self.assertFalse(visible(restored, key="sql-find-bar"))
        self.assertEqual(self._settled_rect(restored, "sql-editor-scroll"), idle_box)
        editor = next(e for e in restored["elements"] if e.get("key") == "sql-editor-scroll")
        self.assertTrue(editor["focused"])
        self.expect_rendered(restored, key="sql-selected-1-8", text="中文")
        self.assertFalse(visible(restored, key="result-workspace"))
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
        self.assertEqual(self.db.read_bytes(), db_bytes)

    def test_native_sql_editor_caret_keeps_literal_semicolon_and_ignores_other_write(self):
        original = self.sql_fixture_before()
        statement = "SELECT '保留;微笑' AS picked;"
        sql = ("UPDATE customers SET customer_name='unselected-write' WHERE id=2;\n"
               "/* ; not a separator */ " + statement)
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(statement).fetchone()[0]
        actions = [*query_steps(sql)[:-1], key(9.7, "Enter", cmd=True)]
        tree = self.replay(actions, until=12.3)
        self.expect_rendered(tree, text=expected, within="result-workspace")
        self.expect_rendered(tree, key="result-grid-status", text="result · 1 rows × 1 cols")
        self.assertEqual(self.sql_customer_rows(), original)

    def test_native_sql_editor_key_navigation_selects_and_runs_only_visible_line(self):
        """Real Home/End/arrow/Shift and a painted selection, no Find injection."""
        original = self.sql_fixture_before()
        selected = "SELECT '键盘;👩‍👩‍👧‍👧' AS picked"
        sql = ("UPDATE customers SET customer_name='must-not-run-left' WHERE id=1;\n"
               + selected + "\n"
               "UPDATE customers SET customer_name='must-not-run-right' WHERE id=2;")
        prefix = query_steps(sql)[:-1]
        prefix[-1] = {"t": 4.8, "type": "paste", "text": sql}
        select = [
            *prefix, key(6.0, "Home"), key(6.4, "Up"),
            key(6.8, "Right"), key(7.2, "Left"), key(7.6, "End"),
            {"t": 8.1, "type": "key", "name": "Home", "shift": True},
        ]
        highlighted = self.replay(select, until=8.8)
        self.expect_rendered(highlighted, key="sql-selected-1-0", text=selected)
        self.assertFalse(visible(highlighted, key="sql-selected-0-0"))
        self.assertFalse(visible(highlighted, key="sql-selected-2-0"))
        self.assertFalse(visible(highlighted, key="result-workspace"))

        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(selected).fetchone()[0]
        result = self.replay([*select, key(9.1, "Enter", cmd=True)], until=11.7)
        self.expect_rendered(result, text=expected, within="result-cell-0-0")
        self.assertEqual(self.sql_customer_rows(), original)
        history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
        self.assertEqual([(item["success"], item["sql"].rstrip(";")) for item in history],
                         [(True, selected)])

    def test_native_sql_editor_refuses_a_multi_statement_selection_without_writing(self):
        original = self.sql_fixture_before()
        sql = "SELECT 1; UPDATE customers SET customer_name='must-not-run' WHERE id=1;"
        tree = self.replay([
            *query_steps(sql)[:-1], key(8.3, "a", cmd=True), tap(9.2, "sql-run"),
        ], until=10.8)
        self.expect_rendered(tree, text="Select one SQL statement before running.", within="query-notice")
        self.assertFalse(visible(tree, key="result-workspace"))
        self.assertEqual(self.sql_customer_rows(), original)

    def test_native_sql_editor_refuses_incomplete_input_without_executing_valid_prefix(self):
        original = self.sql_fixture_before()
        sql = ("UPDATE customers SET customer_name='must-not-run' WHERE id=1;\n"
               "SELECT 'unfinished;仍未关闭")
        tree = self.replay([*query_steps(sql)[:-1], tap(9.2, "sql-run")], until=11)
        self.expect_rendered(tree, key="query-notice")
        self.assertTrue(any(
            "The SQLite statement is not complete." in (node.get("text") or "")
            for node in tree["elements"]
        ))
        self.assertFalse(visible(tree, key="result-workspace"))
        self.assertEqual(self.sql_customer_rows(), original)


    def test_legacy_sqlite_script_shows_separate_result_sets(self):
        """Migrate sqlite.ts multiple statements and verify both result tabs."""
        fixture(self.store, self.db)
        with closing(sqlite3.connect(self.db)) as db:
            with db:
                db.executescript("CREATE TABLE users(name TEXT); "
                                 "CREATE TABLE posts(title TEXT); "
                                 "INSERT INTO users VALUES ('Ada'), ('Lin'); "
                                 "INSERT INTO posts VALUES ('One'), ('Two'), ('Three');")
        statements = ("SELECT COUNT(*) AS users_total FROM users",
                      "/* ; not a separator */ SELECT COUNT(*) AS posts_total FROM posts")

        def counts():
            with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
                return tuple(db.execute(sql).fetchone()[0] for sql in statements)

        expected = counts()
        self.assertNotEqual(*expected)
        script = ";\n".join(statements) + ";"

        def replay_into_editor(sql):
            steps = query_steps(sql)[:-1]
            steps[-1] = {"t": 5, "type": "paste", "text": sql}
            return steps

        actions = [*replay_into_editor(script), tap(7, "sql-run-script")]

        def expect_tab(tree, chosen):
            for ident in (1, 2):
                self.expect_rendered(tree, key=f"query-result-tab-{ident}")
            names = ("users_total", "posts_total")
            self.expect_rendered(tree, text=names[chosen], within="result-data-grid-header")
            self.assertFalse(visible(tree, text=names[1-chosen], within="result-data-grid-header"))
            self.expect_rendered(tree, text=str(expected[chosen]), within="result-cell-0-0")
            self.expect_rendered(tree, key="result-grid-status", text="result · 1 rows × 1 cols")

        expect_tab(self.replay(actions, until=9.5), 0)
        second = [*actions, tap(9.8, "query-result-label-2")]
        expect_tab(self.replay(second, until=11), 1)
        expect_tab(self.replay([*second, tap(11.3, "query-result-label-1")], until=12.6), 0)

        mixed = statements[0] + "; DELETE FROM posts;"
        blocked = self.replay([*replay_into_editor(mixed), tap(7, "sql-run-script")], until=9.5)
        self.expect_rendered(blocked, text="The SQLite script may contain only SELECT statements.",
                             within="query-notice")
        self.assertFalse(visible(blocked, key="result-workspace"))
        self.assertEqual(counts(), expected)
