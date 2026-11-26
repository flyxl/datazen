"""Native editor runs only the selected or caret SQLite statement."""
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
