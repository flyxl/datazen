"""Actual SQLite replay for legacy cell-drag selection."""
import json
import sqlite3

from offline import OPEN, key
from scenarios import fixture, query_steps, tap, visible


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

    def test_table_and_query_result_alternating_rows_follow_the_saved_switch(self):
        """The real Data Browsing control changes ordinary SQLite table and result cells."""
        fixture(self.store, self.db)
        connection = json.loads(self.store.read_text(encoding="utf-8"))
        connection["connections"][0]["readOnly"] = True
        connection_bytes = json.dumps(connection).encode()
        self.store.write_bytes(connection_bytes)
        settings = self.root / "settings.json"
        original = {"ui": {"language": "en", "tablePageSize": 50}, "rowCountMode": "exact",
                    "defaultTableSort": "primaryKeyAsc", "unrelated": {"keep": 1}}
        legacy = json.dumps(original).encode()
        settings.write_bytes(legacy)
        db_original = self.db.read_bytes()
        section = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-dataBrowsing")]
        switch = "settings-grid-alternating-rows"
        do_query = query_steps("SELECT id,customer_name FROM customers ORDER BY id;")

        def node(tree, key):
            items = [n for n in tree["elements"] if n.get("key") == key
                     and not n.get("exiting") and n.get("opacity", 0) > .95
                     and min(n["rect"][2:]) > 0]
            self.assertEqual(len(items), 1, key)
            return items[0]

        def cells(tree, prefix):
            output = []
            for row, name in [(0, "fixture-ada"), (1, "fixture-lin")]:
                normal = f"{prefix}cell-{row}-1"
                frozen = f"{prefix}cell-{row}-0"
                self.expect_rendered(tree, text=name, within=normal)
                self.expect_rendered(tree, text=str(row + 1), within=frozen)
                output.append((node(tree, normal)["fill"], node(tree, frozen)["fill"]))
            return output

        def check_switch(tree, want):
            control = node(tree, switch)
            self.assertEqual(control["role"], "switch")
            self.assertTrue(control["focusable"])
            self.assertEqual(control["rect"][2:], [36, 20])

            def walk(entry):
                yield entry
                for child in entry.get("children", ()):
                    yield from walk(child)

            accessible = [e for e in walk(tree["accessibility"]) if e.get("key") == switch]
            self.assertEqual(len(accessible), 1)
            self.assertEqual(accessible[0]["role"], "switch")
            self.assertEqual(accessible[0]["name"], "Alternating row backgrounds")
            self.assertEqual("checked" in accessible[0].get("states", ()), want)

        table_on = cells(self.replay(OPEN, until=5.5), "")
        result_on = cells(self.replay(do_query, until=11.5), "result-")
        for on in (table_on, result_on):
            self.assertNotEqual(on[0][0], on[1][0])
        check_switch(self.replay(section, until=2.8), True)
        self.assertEqual(settings.read_bytes(), legacy)

        check_switch(self.replay([*section, tap(2.6, switch)], until=4), False)
        self.assertEqual(json.loads(settings.read_bytes()), {**original, "gridAlternatingRows": False})

        table_off = cells(self.replay(OPEN, until=5.5), "")
        result_off = cells(self.replay(do_query, until=11.5), "result-")
        for on, off in ((table_on, table_off), (result_on, result_off)):
            self.assertEqual(off[0][0], off[1][0])
            self.assertEqual(on[0][0], off[0][0], "ordinary even base remains")
            self.assertEqual([row[1] for row in off], [row[1] for row in on],
                             "active/frozen cells keep their real paint")
        check_switch(self.replay(section, until=2.8), False)
        check_switch(self.replay([*section, tap(2.6, switch)], until=4), True)
        self.assertEqual(json.loads(settings.read_bytes()), {**original, "gridAlternatingRows": True})
        self.assertEqual(cells(self.replay(OPEN, until=5.5), ""), table_on)
        self.assertEqual(self.db.read_bytes(), db_original)
        self.assertEqual(self.store.read_bytes(), connection_bytes)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())
