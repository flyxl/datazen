"""Original QLIMIT-003/004 through Native Settings and a real SQLite query."""
import json
import sqlite3

from offline import DATA_SETTINGS, SETTINGS, fill
from scenarios import fixture, query_steps, tap, visible


class QueryRowLimitJourney:
    def test_legacy_query_configured_row_limit_preserves_exact_boundary_and_warns(self):
        """Configure actual settings; distinguish equal and over-limit real SELECTs."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executemany("INSERT INTO customers VALUES (?, ?)", [
                (3, "sentinel-outside-cap"), (4, "sentinel-last"),
            ])
            sql = "SELECT customer_name FROM customers ORDER BY id;"
            exact_sql = "SELECT customer_name FROM customers WHERE id <= 2 ORDER BY id;"
            expected = [row[0] for row in db.execute(sql)]
            exact_expected = [row[0] for row in db.execute(exact_sql)]
            # SQLite's final VALUES arm does not permit an appended LIMIT.
            compound_two = "SELECT 1 AS probe UNION ALL VALUES (2);"
            compound_three = "SELECT 1 AS probe UNION ALL VALUES (2), (3);"
            two = [str(row[0]) for row in db.execute(compound_two)]
            three = [str(row[0]) for row in db.execute(compound_three)]
        self.assertEqual((two, three), (["1", "2"], ["1", "2", "3"]))
        self.assertEqual(exact_expected, expected[:2])
        self.assertEqual(len(expected), 4)

        def set_cap(rows):
            tree = self.replay([
                SETTINGS, DATA_SETTINGS,
                *fill(2.2, "settings-max-rows-input", str(rows)), tap(3.4, "settings-max-rows-set"),
            ], until=4.8)
            self.expect_rendered(tree, text=f"Query max rows: {rows}", within="settings-data")
            self.assertFalse(visible(tree, key="settings-error"))
            settings = json.loads((self.root / "settings.json").read_text(encoding="utf-8"))
            self.assertEqual(settings["ui"]["queryMaxRows"], rows)

        def check_rows(tree, values):
            self.expect_rendered(tree, key="result-grid-status",
                                 text=f"result · {len(values)} rows × 1 cols")
            for index, value in enumerate(values):
                self.expect_rendered(tree, text=value, within=f"result-cell-{index}-0")
            self.assertFalse(visible(tree, key=f"result-cell-{len(values)}-0"))

        set_cap(2)
        exact = self.replay(query_steps(exact_sql), until=10.6)
        check_rows(exact, exact_expected)
        self.assertFalse(visible(exact, key="result-truncated"))

        compound_equal = self.replay(query_steps(compound_two), until=10.6)
        check_rows(compound_equal, two)
        self.assertFalse(visible(compound_equal, key="result-truncated"))
        compound_extra = self.replay(query_steps(compound_three), until=10.6)
        check_rows(compound_extra, two)
        self.expect_rendered(compound_extra, key="result-truncated", within="result-workspace")

        limited = self.replay(query_steps(
            "SELECT customer_name FROM customers ORDER BY id LIMIT 1 OFFSET 2;"), until=10.6)
        check_rows(limited, [expected[2]])
        self.assertFalse(visible(limited, key="result-truncated"))

        over = self.replay(query_steps(sql), until=10.6)
        check_rows(over, expected[:2])
        self.expect_rendered(over, key="result-truncated", within="result-workspace")
        truncation = [node["text"] for node in over["elements"]
                      if node.get("key") == "result-truncated" and node.get("text")]
        self.assertTrue(any("2" in message for message in truncation), truncation)
        self.assertFalse(visible(over, text=expected[2], within="result-workspace"))
        self.assertFalse(visible(over, text=expected[3], within="result-workspace"))

        set_cap(4)
        all_rows = self.replay(query_steps(sql), until=10.6)
        check_rows(all_rows, expected)
        self.assertFalse(visible(all_rows, key="result-truncated"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual([row[0] for row in db.execute(sql)], expected)

    def test_legacy_query_truncated_result_disables_csv_export_for_identical_prefix(self):
        """Compare the actual complete/capped native grid and its export control."""
        fixture(self.store, self.db)
        exact_sql = "SELECT id, customer_name FROM customers WHERE id <= 2 ORDER BY id;"
        longer_sql = "SELECT id, customer_name FROM customers WHERE id <= 3 ORDER BY id;"
        with sqlite3.connect(self.db) as db:
            db.execute("INSERT INTO customers VALUES (?, ?)", (3, "must-be-outside-cap"))
            exact = db.execute(exact_sql).fetchall()
            longer = db.execute(longer_sql).fetchall()
        self.assertEqual(exact, longer[:2])
        self.assertEqual(len(exact), 2)
        self.assertEqual(len(longer), 3)

        settings = self.root / "settings.json"
        settings.write_text(json.dumps({"ui": {"language": "en", "queryMaxRows": 2}}),
                            encoding="utf-8")

        def steps(statement):
            actions = query_steps(statement)
            actions[-2] = {"t": 5.2, "type": "paste", "text": statement}
            return actions

        def prefix(tree):
            self.expect_rendered(tree, key="result-grid", within="result-workspace")
            self.expect_rendered(tree, key="result-grid-status", text="result · 2 rows × 2 cols")
            for row, values in enumerate(exact):
                for col, value in enumerate(values):
                    self.expect_rendered(tree, text=str(value), within=f"result-cell-{row}-{col}")
            self.assertFalse(visible(tree, key="result-cell-2-0"))
            self.assertFalse(visible(tree, text=longer[2][1], within="result-workspace"))

        complete = self.replay(steps(exact_sql), until=11.0)
        prefix(complete)
        self.assertFalse(visible(complete, key="result-truncated", within="result-workspace"))
        self.expect_rendered(complete, key="data-table-export", within="result-workspace")

        capped = self.replay(steps(longer_sql), until=11.0)
        prefix(capped)
        self.expect_rendered(capped, key="result-truncated", within="result-workspace")
        # Stronger than invisibility: no export control or modal is constructed
        # at all, including outside the viewport or in an exiting frame.
        self.assertFalse(any(node.get("key") in {"data-table-export", "data-export-dialog"}
                             for node in capped["elements"]))

        reviewed = self.replay([
            *steps(exact_sql),
            tap(11.5, "data-table-export"),
        ], until=13.0)
        self.expect_rendered(reviewed, key="data-export-dialog")
        self.expect_rendered(reviewed, text="CSV · 2 × 2", within="data-export-dialog")
        self.assertFalse(list(self.root.rglob("*.csv")))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute(longer_sql).fetchall(), longer)
