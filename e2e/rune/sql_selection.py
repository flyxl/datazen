"""Native SQLite editor: selected/caret runs and explicit SELECT scripts."""
from contextlib import closing
import sqlite3

from offline import fill, key
from scenarios import fixture, query_steps, tap, visible


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

    def test_native_sql_editor_executes_only_exact_selection_with_literal_semicolon(self):
        original = self.sql_fixture_before()
        selected = "SELECT '选中;值' AS picked"
        sql = (selected + " UNION ALL SELECT 'unselected sibling';\n"
               "UPDATE customers SET customer_name='unselected-write' WHERE id=1;")
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(selected).fetchone()[0]
        actions = [
            *query_steps(sql)[:-1],
            *fill(9.5, "sql-find-input", selected), tap(11.0, "sql-find-next"),
            tap(12.0, "sql-run"),
        ]
        tree = self.replay(actions, until=14.4)
        self.expect_rendered(tree, text=expected, within="result-workspace")
        self.expect_rendered(tree, key="result-grid-status", text="result · 1 rows × 1 cols")
        self.assertFalse(visible(tree, text="unselected sibling", within="result-workspace"))
        self.assertEqual(self.sql_customer_rows(), original)

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
