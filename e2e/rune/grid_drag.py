"""Actual SQLite replay for legacy cell-drag selection."""
import sqlite3

from offline import OPEN, key
from scenarios import fixture, tap, visible


class GridDragJourney:
    def test_legacy_table_drag_selects_exact_rectangle_and_escape_cancels_gesture(self):
        """Port TC-SEL-002 using native pointer input and real SQLite cell values."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript(
                "ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0;"
                "UPDATE customers SET score=100+id;"
                "INSERT INTO customers VALUES "
                "(3,'third',103),(4,'fourth',104),(5,'fifth',105),(6,'sixth',106),(7,'untouched',107);"
            )
            expected = db.execute("SELECT * FROM customers ORDER BY id").fetchall()

        def paints(tree):
            return {node["key"]: node["fill"] for node in tree["elements"]
                    if node.get("role") == "gridcell" and (node.get("key") or "").startswith("cell-")}

        def pointer(t, event, cell):
            return {"t": t, "type": event, "key": cell}

        baseline = self.replay(OPEN, until=4.8)
        normal = paints(baseline)
        self.assertEqual(set(normal), {f"cell-{r}-{c}" for r in range(7) for c in range(3)})
        drag = [pointer(5.2, "down", "cell-2-0"), pointer(5.7, "move", "cell-4-1"),
                pointer(6.2, "move", "cell-5-2"), pointer(6.6, "up", "cell-5-2")]
        selected = self.replay([*OPEN, *drag], until=7.9)
        self.expect_rendered(selected, key="data-grid-status",
                             text="7×3 · window at column 0 · selection r2c0..r5c2 · 0 dirty")
        actual = paints(selected)
        self.assertEqual(set(actual), set(normal))
        self.assertEqual({cell for cell in normal if actual[cell] != normal[cell]},
                         {f"cell-{r}-{c}" for r in range(2, 6) for c in range(3)})
        for row, values in enumerate(expected):
            for col, val in enumerate(values):
                self.expect_rendered(selected, text=str(val), within=f"cell-{row}-{col}")
        self.assertFalse(visible(selected, key="grid-editor"))

        cancelled = self.replay([
            *OPEN, tap(5.1, "cell-0-1"), pointer(6.0, "down", "cell-2-0"),
            pointer(6.5, "move", "cell-5-2"), key(6.9, "Escape"),
            pointer(7.2, "up", "cell-5-2"),
        ], until=8.3)
        self.expect_rendered(cancelled, key="data-grid-status",
                             text="7×3 · window at column 0 · selection r0c1..r0c1 · 0 dirty")
        restored = paints(cancelled)
        self.assertEqual({cell for cell in normal if restored[cell] != normal[cell]}, {"cell-0-1"})
        self.assertFalse(visible(cancelled, key="grid-editor"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), expected)

        # A scroll can virtualize the pressed source row before Rune dispatches
        # Up. It must restore the previous logical selection without blocking
        # the actual wheel, or highlighting rows in the newly visible viewport.
        with sqlite3.connect(self.db) as db:
            db.executemany("INSERT INTO customers VALUES (?, ?, ?)",
                           [(i, f"scroll-{i}", 100 + i) for i in range(8, 48)])
            all_rows = db.execute("SELECT * FROM customers ORDER BY id").fetchall()
        # The default 25-row page cannot move the dragged origin past the
        # virtual list's overscan. Load all 47 rows into this real grid page.
        (self.root / "settings.json").write_text(
            '{"ui":{"tablePageSize":50}}', encoding="utf-8")
        scrolled = self.replay([
            *OPEN, tap(5.1, "cell-0-1"), pointer(6.0, "down", "cell-2-0"),
            pointer(6.5, "move", "cell-5-2"),
            {"t": 6.9, "type": "wheel", "key": "data-grid-scroll", "dy": -1400},
            pointer(7.7, "up", "data-grid-scroll"),
        ], until=9.1)
        self.expect_rendered(scrolled, key="data-grid-status",
                             text="47×3 · window at column 0 · selection r0c1..r0c1 · 0 dirty")
        self.expect_rendered(scrolled, text="Page 1 / 1", key="grid-page-label")
        self.expect_rendered(scrolled, text="47 rows", within="grid-filter")
        self.assertFalse(any(node.get("key") == "cell-2-0" for node in scrolled["elements"]),
                         "the original cell never left the mounted virtual rows")
        self.expect_rendered(scrolled, text=all_rows[-1][1], within="cell-46-1")
        self.assertFalse(visible(scrolled, key="grid-editor"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), all_rows)
