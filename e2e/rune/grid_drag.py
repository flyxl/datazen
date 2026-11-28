"""Actual SQLite replay for legacy cell-drag selection."""
import json
import sqlite3

from offline import OPEN, key
from scenarios import fixture, query_steps, tap, visible


class GridDragJourney:
    def test_table_body_tints_only_nonempty_sqlite_cells_with_declared_numeric_type(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as seed:
            seed.executescript("""
                ALTER TABLE customers ADD COLUMN label TEXT;
                ALTER TABLE customers ADD COLUMN amount INTEGER;
                UPDATE customers SET label='123', amount=123 WHERE id=1;
                UPDATE customers SET label='', amount=NULL WHERE id=2;
                INSERT INTO customers VALUES (3,'literal','NULL',0);
            """)
        (self.root / "settings.json").write_text(json.dumps({
            "ui": {"language": "en"},
            "rowCountMode": "exact", "defaultTableSort": "primaryKeyAsc",
        }), encoding="utf-8")
        table = self.replay(OPEN, until=5.1)
        marker = "table-cell-number-value"
        self.expect_rendered(table, key=marker, text="123", within="cell-0-3")
        self.expect_rendered(table, key=marker, text="0", within="cell-2-3")
        self.expect_rendered(table, text="123", within="cell-0-2")
        self.expect_rendered(table, text="NULL", within="cell-2-2")
        for cell in ("cell-0-2", "cell-1-2", "cell-1-3", "cell-2-2"):
            self.assertFalse(visible(table, key=marker, within=cell))
        self.assertFalse(visible(table, text="NULL", within="cell-1-3"))

        query = self.replay(query_steps("SELECT label, amount FROM customers ORDER BY id;"), until=10.5)
        self.expect_rendered(query, text="123", within="result-cell-0-1")
        self.assertFalse(visible(query, key=marker, within="result-data-grid-scroll"))
        with sqlite3.connect(self.db) as oracle:
            self.assertEqual(oracle.execute("SELECT label, amount FROM customers ORDER BY id").fetchall(),
                             [("123", 123), ("", None), ("NULL", 0)])

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
                           [(i, f"scroll-{i}", 100 + i) for i in range(8, 97)])
            all_rows = db.execute("SELECT * FROM customers ORDER BY id").fetchall()
        db_before_scroll = self.db.read_bytes()
        # The responsive grid's larger viewport may retain 47 rows including
        # overscan. Use a longer single page and wheel to its actual bottom.
        (self.root / "settings.json").write_text(
            '{"ui":{"tablePageSize":100}}', encoding="utf-8")
        scrolled = self.replay([
            *OPEN, tap(5.1, "cell-0-1"), pointer(6.0, "down", "cell-2-0"),
            pointer(6.5, "move", "cell-5-2"),
            {"t": 6.9, "type": "wheel", "key": "data-grid-scroll", "dy": -3600},
            pointer(7.7, "up", "data-grid-scroll"),
        ], until=9.1)
        self.expect_rendered(scrolled, key="data-grid-status",
                             text="96×3 · window at column 0 · selection r0c1..r0c1 · 0 dirty")
        self.expect_rendered(scrolled, text="Page 1 / 1", key="grid-page-label")
        self.expect_rendered(scrolled, text="96 rows", within="grid-filter")
        self.assertFalse(any(node.get("key") == "cell-2-0" for node in scrolled["elements"]),
                         "the original cell never left the mounted virtual rows")
        self.expect_rendered(scrolled, text=all_rows[-1][1], within="cell-95-1")
        last = self._settled_rect(scrolled, "cell-95-1")
        viewport = self._settled_rect(scrolled, "data-grid-scroll")
        header = self._settled_rect(scrolled, "data-grid-header")
        self.assertGreaterEqual(last[1] + .25, viewport[1] + header[3])
        self.assertAlmostEqual(last[1] + last[3], viewport[1] + viewport[3], delta=.25)
        self.assertFalse(visible(scrolled, key="grid-editor"))
        self.assertEqual(self.db.read_bytes(), db_before_scroll)
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
                first = f"{prefix}cell-{row}-0"
                self.expect_rendered(tree, text=name, within=normal)
                self.expect_rendered(tree, text=str(row + 1), within=first)
                output.append((node(tree, normal)["fill"], node(tree, first)["fill"]))
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
        self.assertEqual([row[0] for row in table_on], [row[1] for row in table_on],
                         "with zero pinned columns, ID is an ordinary striped table cell")
        self.assertEqual([row[0] for row in table_off], [row[1] for row in table_off])
        self.assertEqual([row[1] for row in result_off], [row[1] for row in result_on],
                         "Query Result keeps its existing frozen paint")
        check_switch(self.replay(section, until=2.8), False)
        check_switch(self.replay([*section, tap(2.6, switch)], until=4), True)
        self.assertEqual(json.loads(settings.read_bytes()), {**original, "gridAlternatingRows": True})
        self.assertEqual(cells(self.replay(OPEN, until=5.5), ""), table_on)
        self.assertEqual(self.db.read_bytes(), db_original)
        self.assertEqual(self.store.read_bytes(), connection_bytes)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())

    def test_data_grid_scrollbars_use_the_saved_auto_hide_preference(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as seed:
            seed.executemany("INSERT INTO customers VALUES (?, ?)", [
                (i, f"autohide-{i:02}") for i in range(3, 96)
            ])
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        database_before = self.db.read_bytes()
        connection_before = self.store.read_bytes()
        config = self.root / "settings.json"
        original = {"ui": {"language": "en", "themeMode": "light", "tablePageSize": 100},
                    "rowCountMode": "exact", "keep": {"sentinel": 37}}
        config.write_text(json.dumps(original), encoding="utf-8")
        before = config.read_bytes()
        switch = "settings-grid-auto-hide-scrollbar"
        route = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-dataBrowsing")]
        outside = lambda t: {"t": t, "type": "move", "x": 1, "y": 1}

        def checked(tree, value):
            self.expect_rendered(tree, key=switch)
            def walk(node):
                yield node
                for child in node.get("children", ()):
                    yield from walk(child)
            controls = [n for n in walk(tree["accessibility"]) if n.get("key") == switch]
            self.assertEqual(len(controls), 1)
            self.assertEqual(controls[0]["role"], "switch")
            self.assertEqual("checked" in controls[0].get("states", ()), value)

        checked(self.replay(route, until=2.5), False)
        self.assertEqual(config.read_bytes(), before)
        table = ("data-grid-scroll", list(OPEN), "cell-0-1", 5.2)
        result = ("result-data-grid-scroll",
                  query_steps("SELECT id, customer_name FROM customers ORDER BY id;"),
                  "result-cell-0-1", 10.8)

        def focusable(tree, grid):
            owners = [node for node in tree["elements"]
                      if node.get("key") == grid and not node.get("exiting")]
            self.assertEqual(len(owners), 1)
            return owners[0]["focusable"]

        default = {}
        for grid, actions, cell, t in [table, result]:
            tree = self.replay([*actions, outside(t)], until=t + .65)
            self.expect_rendered(tree, key=cell)
            self.expect_rendered(tree, key="thumb", within="vertical-scrollbar")
            default[grid] = (self._settled_rect(tree, grid), focusable(tree, grid))
        checked(self.replay([*route, tap(2.4, switch)], until=3.5), True)
        expected = {**original, "gridAutoHideScrollbar": True}
        self.assertEqual(json.loads(config.read_bytes()), expected)
        checked(self.replay(route, until=2.5), True)

        for grid, actions, cell, t in [table, result]:
            idle = [*actions, outside(t)]
            tree = self.replay(idle, until=t + .65)
            self.expect_rendered(tree, key=cell)
            self.assertFalse(visible(tree, key="thumb", within="vertical-scrollbar"))
            self.assertEqual((self._settled_rect(tree, grid), focusable(tree, grid)), default[grid])
            hover = [*idle, {"t": t + .8, "type": "move", "key": cell}]
            tree = self.replay(hover, until=t + 1.4)
            self.expect_rendered(tree, key="thumb", within="vertical-scrollbar")
            self.assertEqual(self._settled_rect(tree, grid), default[grid][0])
            tree = self.replay([*hover, outside(t + 1.5)], until=t + 2.0)
            self.assertFalse(visible(tree, key="thumb", within="vertical-scrollbar"))
            self.assertEqual(self._settled_rect(tree, grid), default[grid][0])

        # Block the atomic replacement; keep the effective value and both
        # data profiles unchanged regardless of the attempted setting.
        collision = config.with_suffix(".json.tmp")
        collision.mkdir()
        try:
            checked_result = self.replay([*route, tap(2.4, switch)], until=3.5)
            checked(checked_result, True)
            self.expect_rendered(checked_result, text="Could not save the grid scrollbar setting.",
                                 within="settings-error")
            self.assertEqual(json.loads(config.read_bytes()), expected)
        finally:
            collision.rmdir()
        checked(self.replay([*route, tap(2.4, switch)], until=3.5), False)
        self.assertEqual(json.loads(config.read_bytes()), {**original, "gridAutoHideScrollbar": False})
        self.assertEqual(self.db.read_bytes(), database_before)
        self.assertEqual(self.store.read_bytes(), connection_before)

    def test_row_numbers_controls_real_readonly_sqlite_grids_after_restart(self):
        """Row Numbers controls the real Table and Query Result without changing their data."""
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        self.env["HOME"] = str(self.root)
        self.env["TZ"] = "UTC"

        sql = "SELECT id, customer_name FROM customers ORDER BY id;"
        oracle_uri = f"{self.db.as_uri()}?mode=ro&immutable=1"
        with sqlite3.connect(oracle_uri, uri=True) as oracle:
            expected = oracle.execute(sql).fetchall()
        self.assertEqual(expected, [(1, "fixture-ada"), (2, "fixture-lin")])
        original_db = self.db.read_bytes()
        original_connections = self.store.read_bytes()

        settings = self.root / "settings.json"
        original_settings = {
            "ui": {"language": "en", "themeMode": "light", "tablePageSize": 25},
            "rowCountMode": "exact",
            "defaultTableSort": "primaryKeyAsc",
            "gridAlternatingRows": False,
            "unrelated": {"keep": "row-number-fixture"},
        }
        settings.write_text(json.dumps(original_settings), encoding="utf-8")
        settings.chmod(0o600)
        initial_settings = settings.read_bytes()
        section = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-dataBrowsing")]
        switch = "settings-grid-row-numbers"

        def a11y_node(tree, target):
            def visit(entry):
                if entry.get("key") == target:
                    yield entry
                for child in entry.get("children", ()):
                    yield from visit(child)
            matches = list(visit(tree["accessibility"]))
            self.assertEqual(len(matches), 1, target)
            return matches[0]

        def check_switch(tree, on):
            self.expect_rendered(tree, key=switch)
            control = a11y_node(tree, switch)
            self.assertEqual((control.get("role"), control.get("name")), ("switch", "Row numbers"))
            self.assertIn("click", control.get("actions", ()))
            self.assertIn("checked" if on else "unchecked", control.get("states", ()))
            self.assertFalse(visible(tree, key="settings-error"))

        def check_grid(tree, prefix, show):
            header = f"{prefix}head-rownum"
            gutter = {f"{prefix}rownum-{row}" for row in range(len(expected))}
            relevant = [n for n in tree["elements"] if not n.get("exiting")]
            keys = {n.get("key") for n in relevant}
            if show:
                self.expect_rendered(tree, key=header)
                self.expect_rendered(tree, text="#", within=header)
                for row in range(len(expected)):
                    row_header = f"{prefix}rownum-{row}"
                    self.expect_rendered(tree, text=str(row + 1), within=row_header)
                    self.assertEqual(a11y_node(tree, row_header)["role"], "row-header")
                    self.assertAlmostEqual(self._settled_rect(tree, row_header)[2], 40, delta=.25)
            else:
                self.assertTrue(keys.isdisjoint({header, *gutter}), "no hidden gutter nodes")

            wanted = {f"{prefix}cell-{r}-{c}" for r, values in enumerate(expected)
                      for c in range(len(values))}
            found = {n["key"] for n in relevant if n.get("role") == "gridcell"
                     and (n.get("key") or "").startswith(f"{prefix}cell-")}
            self.assertEqual(found, wanted, "same logical cell count and identifiers")
            for row, values in enumerate(expected):
                for col, value in enumerate(values):
                    self.expect_rendered(tree, text=str(value), within=f"{prefix}cell-{row}-{col}")
            return {key: self._settled_rect(tree, key) for key in wanted}

        check_switch(self.replay(section, until=2.5), True)
        self.assertEqual(settings.read_bytes(), initial_settings, "mount does not write defaults")

        table_on = self.replay([*OPEN, tap(4.8, "rownum-1")], until=5.8)
        self.expect_rendered(table_on, key="grid-readonly")
        self.expect_rendered(table_on, key="data-grid-status",
                             text="2×2 · window at column 0 · selection row 2 · 0 dirty")
        self.assertEqual(a11y_node(table_on, "data-grid-scroll").get("activeDescendant"), "rownum-1")
        table_with_gutter = check_grid(table_on, "", True)
        query = query_steps(sql)
        result_on = self.replay([*query, tap(10.9, "result-cell-1-1")], until=11.8)
        result_with_gutter = check_grid(result_on, "result-", True)
        self.assertEqual(a11y_node(result_on, "result-data-grid-scroll").get("activeDescendant"),
                         "result-cell-1-1")

        check_switch(self.replay([*section, tap(2.6, switch)], until=4), False)
        persisted = {**original_settings, "gridRowNumbers": False}
        self.assertEqual(json.loads(settings.read_bytes()), persisted)
        # Each replay launches a new native process from the same disposable profile.
        check_switch(self.replay(section, until=2.5), False)

        table_off = self.replay([*OPEN, tap(4.8, "cell-1-1")], until=5.8)
        self.expect_rendered(table_off, key="data-grid-status",
                             text="2×2 · window at column 0 · selection r1c1..r1c1 · 0 dirty")
        self.assertEqual(a11y_node(table_off, "data-grid-scroll").get("activeDescendant"), "cell-1-1")
        table_without_gutter = check_grid(table_off, "", False)
        result_off = self.replay([*query, tap(10.9, "result-cell-1-1")], until=11.8)
        result_without_gutter = check_grid(result_off, "result-", False)
        self.assertEqual(a11y_node(result_off, "result-data-grid-scroll").get("activeDescendant"),
                         "result-cell-1-1")

        for previous, current in ((table_with_gutter, table_without_gutter),
                                  (result_with_gutter, result_without_gutter)):
            self.assertEqual(previous.keys(), current.keys())
            for cell, rect in previous.items():
                self.assertAlmostEqual(rect[0] - current[cell][0], 40, delta=.25)
                self.assertEqual(rect[1:], current[cell][1:])

        # Real clicks and a real pointer drag select the same TSV source values
        # with the gutter gone. The host integration test exercises Cmd+C itself.
        selection = self.replay([
            *OPEN, tap(4.8, "cell-1-1"),
            {"t": 5.4, "type": "down", "key": "cell-0-0"},
            {"t": 5.9, "type": "move", "key": "cell-1-1"},
            {"t": 6.3, "type": "up", "key": "cell-1-1"},
        ], until=7.0)
        self.expect_rendered(selection, key="data-grid-status",
                             text="2×2 · window at column 0 · selection r0c0..r1c1 · 0 dirty")
        check_grid(selection, "", False)

        self.assertEqual(json.loads(settings.read_bytes()), persisted)
        self.assertEqual(self.store.read_bytes(), original_connections)
        self.assertEqual(self.db.read_bytes(), original_db)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())
        with sqlite3.connect(oracle_uri, uri=True) as oracle:
            self.assertEqual(oracle.execute(sql).fetchall(), expected)

    def test_grid_density_changes_real_sqlite_row_geometry_and_keeps_find_reveal(self):
        """The mounted selector changes actual rows, including virtualized query results."""
        from offline import fill

        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as seed:
            seed.executemany("INSERT INTO customers VALUES (?, ?)", [
                (i, f"density-{i:02}") for i in range(3, 52)
            ])
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        self.env["HOME"] = str(self.root)
        self.env["TZ"] = "UTC"
        original_db = self.db.read_bytes()
        connection_bytes = self.store.read_bytes()
        sql = "SELECT id, customer_name FROM customers ORDER BY id;"
        uri = f"{self.db.as_uri()}?mode=ro&immutable=1"
        with sqlite3.connect(uri, uri=True) as oracle:
            expected = oracle.execute(sql).fetchall()
        self.assertEqual((len(expected), expected[-1]), (51, (51, "density-51")))

        settings = self.root / "settings.json"
        original = {"ui": {"language": "en", "themeMode": "light", "tablePageSize": 100},
                    "rowCountMode": "exact", "defaultTableSort": "primaryKeyAsc",
                    "gridRowNumbers": True, "unrelated": {"keep": True}}
        settings.write_text(json.dumps(original), encoding="utf-8")
        settings.chmod(0o600)
        initial = settings.read_bytes()
        route = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-dataBrowsing")]
        trigger = "settings-grid-density-trigger"

        def check_field(tree, caption):
            self.expect_rendered(tree, text=caption, within=trigger)
            self.assertFalse(visible(tree, key="settings-error"))
            control = self._settled_rect(tree, trigger)
            stripe = self._settled_rect(tree, "settings-grid-alternating-rows")
            self.assertEqual(control[3], 36)
            self.assertAlmostEqual(stripe[1] - control[1], 56, delta=.25)

        def check_grid(tree, prefix, height):
            header = self._settled_rect(tree, f"{prefix}head-0")
            first = self._settled_rect(tree, f"{prefix}cell-0-0")
            first_gutter = self._settled_rect(tree, f"{prefix}rownum-0")
            second = self._settled_rect(tree, f"{prefix}cell-1-0")
            self.assertEqual(header[3], 28 if prefix else 40,
                             "density must not resize the sticky column header")
            if prefix:
                self.assertFalse(visible(tree, key=f"{prefix}head-type-0"))
            else:
                for col, declaration in enumerate(("INTEGER", "TEXT")):
                    self.expect_rendered(tree, key=f"head-type-{col}", text=declaration)
                    owner = self._settled_rect(tree, f"head-{col}")
                    typed = self._settled_rect(tree, f"head-type-{col}")
                    self.assertGreaterEqual(typed[1], owner[1])
                    self.assertLessEqual(typed[1] + typed[3], owner[1] + owner[3])
                self.assertAlmostEqual(first[1], header[1] + header[3], delta=.25)
            self.assertEqual(first[3], height)
            self.assertEqual(first_gutter[3], height)
            self.assertEqual(second[1] - first[1], height)
            for row, values in enumerate(expected[:2]):
                for col, value in enumerate(values):
                    self.expect_rendered(tree, text=str(value), within=f"{prefix}cell-{row}-{col}")

        default = self.replay(route, until=2.5)
        check_field(default, "Standard")
        self.assertEqual(settings.read_bytes(), initial)
        query = query_steps(sql)
        for wire, caption, option, table_height, query_height in (
            ("standard", "Standard", None, 28, 32),
            ("compact", "Compact", 0, 22, 26),
            ("relaxed", "Relaxed", 2, 36, 40),
        ):
            with self.subTest(density=wire):
                if option is not None:
                    tree = self.replay([*route, tap(2.6, trigger),
                                        tap(3.2, f"option{option}")], until=4.1)
                    check_field(tree, caption)
                    self.assertEqual(json.loads(settings.read_bytes()), {**original, "gridDensity": wire})
                # Fresh native processes must observe the same saved density.
                table = self.replay(OPEN, until=5.3)
                self.expect_rendered(table, key="grid-readonly")
                check_grid(table, "", table_height)
                result = self.replay(query, until=10.5)
                self.expect_rendered(result, key="result-grid-status", text="result · 51 rows × 2 cols")
                check_grid(result, "result-", query_height)
                if wire != "relaxed":
                    continue
                self.assertFalse(visible(result, key="result-cell-50-1"))
                last = self.replay([
                    *query, tap(10.45, "result-find-toggle"),
                    *fill(11.0, "result-find-input", expected[-1][1]),
                    tap(12.5, "result-find-apply"),
                ], until=14.0)
                self.expect_rendered(last, key="result-find-count", text="1 of 1")
                self.expect_rendered(last, text=expected[-1][1], within="result-cell-50-1")
                viewport = self._settled_rect(last, "result-data-grid-scroll")
                hit = self._settled_rect(last, "result-cell-50-1")
                sticky = self._settled_rect(last, "result-data-grid-header")
                self.assertEqual(hit[3], 40)
                self.assertAlmostEqual(sticky[1], viewport[1], delta=.25)
                self.assertGreaterEqual(hit[1] + .25, viewport[1] + sticky[3])
                self.assertLessEqual(hit[1] + hit[3], viewport[1] + viewport[3] + .25)

        check_field(self.replay(route, until=2.5), "Relaxed")
        self.assertEqual(self.db.read_bytes(), original_db)
        self.assertEqual(self.store.read_bytes(), connection_bytes)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())

    def test_classic_grid_scrollbars_use_live_two_axis_overflow_and_keep_cells_working(self):
        """Wide SQLite tables fill the real pane without losing gutters, chrome or cells."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as seed:
            for index in range(15):
                seed.execute(f"ALTER TABLE customers ADD COLUMN value_{index} INTEGER DEFAULT {index}")
            seed.executemany("INSERT INTO customers (id, customer_name) VALUES (?, ?)", [
                (i, f"scrollbar-{i:02}") for i in range(3, 42)
            ])
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original = self.db.read_bytes()
        settings = self.root / "settings.json"

        def save_theme(theme):
            settings.write_text(json.dumps({
                "ui": {"language": "en", "themeMode": theme, "tablePageSize": 100},
                "rowCountMode": "exact", "defaultTableSort": "primaryKeyAsc",
            }), encoding="utf-8")
            settings.chmod(0o600)

        save_theme("light")

        def expect_status_colors(tree, surface, dot):
            self.expect_rendered(tree, key="status-connection-label", text="Connected")
            for key, expected in (("content-status-bar", surface), ("status-connection-dot", dot)):
                bounds = self._settled_rect(tree, key)
                nodes = [node for node in tree["elements"] if node.get("key") == key
                         and node["rect"] == bounds and not node.get("exiting")]
                self.assertEqual([node.get("fill") for node in nodes], [expected], key)

        def grid_a11y(tree, target):
            def visit(node):
                if node.get("key") == target:
                    yield node
                for child in node.get("children", ()):
                    yield from visit(child)
            matches = list(visit(tree["accessibility"]))
            self.assertEqual(len(matches), 1, target)
            return matches[0]

        def bars(tree, prefix):
            scope = f"{prefix}data-grid-scroll"
            frame = self._settled_rect(tree, f"{scope}-scroll-area")
            view = self._settled_rect(tree, scope)
            horizontal = self._settled_rect(tree, "horizontal-scrollbar")
            vertical = self._settled_rect(tree, "vertical-scrollbar")
            x, y, w, h = frame
            self.assertEqual(view, [x, y, w - 9, h - 9])
            self.assertEqual(horizontal, [x, y + h - 9, w - 9, 9])
            self.assertEqual(vertical, [x + w - 9, y, 9, h - 9])
            for track in ("horizontal-scrollbar", "vertical-scrollbar"):
                self.expect_rendered(tree, key="thumb", within=track)
            return view, grid_a11y(tree, scope)

        def table_frame(tree):
            page = self._settled_rect(tree, "datazen-page")
            pane = self._settled_rect(tree, "content-view-panes")
            workspace = self._settled_rect(tree, "table-workspace")
            switcher = self._settled_rect(tree, "table-view-switcher")
            panel = self._settled_rect(tree, "data-grid")
            bar = self._settled_rect(tree, "content-status-bar")
            frame = self._settled_rect(tree, "data-grid-scroll-scroll-area")
            pager = self._settled_rect(tree, "grid-footer")
            bottom = lambda rect: rect[1] + rect[3]
            self.assertEqual(workspace, pane)
            self.assertAlmostEqual(panel[1], bottom(switcher), delta=.25)
            previous = panel[1]
            for name in ("grid-filter", "data-grid-status", "grid-readonly", "fk-navigation"):
                control = self._settled_rect(tree, name)
                self.assertGreaterEqual(control[1], previous, name)
                previous = bottom(control)
            self.assertGreaterEqual(frame[1], previous)
            read_only = self._settled_rect(tree, "grid-readonly")
            fk = self._settled_rect(tree, "fk-navigation")
            self.assertAlmostEqual(fk[1] - bottom(read_only), 8, delta=.25)
            filter_bar = self._settled_rect(tree, "grid-filter")
            chip = self._settled_rect(tree, "filter-column")
            count = self._settled_rect(tree, "grid-filter-count")
            field = self._settled_rect(tree, "filter-input")
            self.assertEqual(filter_bar, [panel[0], panel[1], panel[2], 33])
            self.assertEqual(field[3], 28)
            self.assertAlmostEqual(field[1] - filter_bar[1], 2, delta=.25)
            self.assertAlmostEqual(chip[0] - filter_bar[0], 8, delta=.25)
            self.assertGreaterEqual(count[0], chip[0] + chip[2])
            self.assertGreaterEqual(field[0], count[0] + count[2])
            self.assertAlmostEqual(filter_bar[0] + filter_bar[2] - field[0] - field[2], 8, delta=.25)
            self.assertGreater(field[2], filter_bar[2] * .65, "the real input fills the remaining row")
            self.expect_rendered(tree, key="grid-filter-count", text="41 rows")
            self.expect_rendered(tree, key="fk-referenced-by")
            self.assertFalse(any(n.get("key") == "pending-changes-bar" for n in tree["elements"]))
            self.assertAlmostEqual(pager[1] - bottom(frame), 8, delta=.25)
            self.assertEqual(pager[3], 28)
            self.assertAlmostEqual(bottom(pager), bottom(panel), delta=.25)
            self.assertAlmostEqual(bottom(pager), bar[1], delta=.25)
            self.assertEqual(bar[3], 40)
            self.assertAlmostEqual(bottom(bar), bottom(page), delta=.25)
            self.assertAlmostEqual(bottom(bar), tree["viewport"][1], delta=.25)
            self.expect_rendered(tree, key="filter-input")
            self.expect_rendered(tree, key="grid-page-label", text="Page 1 / 1")
            return frame

        table = self.replay([*OPEN, tap(4.8, "cell-1-1")], until=5.6)
        view, initial = bars(table, "")
        tall_frame = table_frame(table)
        expect_status_colors(table, "#f1f5f9ff", "#18b77aff")
        self.assertEqual(initial["scroll"][:2], [0, 0])
        self.assertGreater(initial["scroll"][2], 0)
        self.assertEqual(initial["activeDescendant"], "cell-1-1")
        self.expect_rendered(table, text="fixture-lin", within="cell-1-1")
        self.expect_rendered(table, key="data-grid-status",
                             text="41×17 · window at column 0 · selection r1c1..r1c1 · 0 dirty")

        save_theme("dark")
        short = self.replay(OPEN, until=5.3, size="1440x640")
        expect_status_colors(short, "#0e121bff", "#5fd08aff")
        save_theme("light")
        short_frame = table_frame(short)
        bars(short, "")
        self.assertEqual(short_frame[:3], tall_frame[:3])
        self.assertAlmostEqual(tall_frame[3] - short_frame[3], 260, delta=.25)
        self.expect_rendered(short, text="fixture-lin", within="cell-1-1")

        dragged = self.replay([
            *OPEN,
            {"t": 4.9, "type": "drag", "key": "horizontal-scrollbar/thumb", "dx": 240, "dur": .35},
            {"t": 5.8, "type": "drag", "key": "vertical-scrollbar/thumb", "dy": 72, "dur": .35},
        ], until=6.7)
        dragged_view, drag_state = bars(dragged, "")
        self.assertEqual(dragged_view, view)
        self.assertGreater(drag_state["scroll"][0], 0)
        self.assertGreater(drag_state["scroll"][1], 0)
        self.assertFalse(visible(dragged, key="head-0"), "the stock Table Data default is unpinned")
        self.assertAlmostEqual(self._settled_rect(dragged, "data-grid-header")[1], view[1], delta=.25)

        wheeled = self.replay([
            *OPEN,
            {"t": 4.9, "type": "wheel", "key": "vertical-scrollbar", "dy": -75},
        ], until=5.9)
        self.assertEqual(bars(wheeled, "")[1]["scroll"][:2], [0, 75])

        query = self.replay([
            *query_steps("SELECT * FROM customers ORDER BY id;"),
            tap(10.8, "result-cell-1-1"),
        ], until=11.6)
        _, query_state = bars(query, "result-")
        expect_status_colors(query, "#f1f5f9ff", "#18b77aff")
        result_host = self._settled_rect(query, "query-results-host")
        query_main = self._settled_rect(query, "query-main")
        self.assertAlmostEqual(result_host[3], query_main[3] / 2, delta=.25)
        self.assertAlmostEqual(result_host[1] + result_host[3], query_main[1] + query_main[3], delta=.25)
        self.assertEqual(query_state["activeDescendant"], "result-cell-1-1")
        self.expect_rendered(query, text="fixture-lin", within="result-cell-1-1")
        self.assertEqual(self.db.read_bytes(), original)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())

    def test_sqlite_table_freeze_through_unfreeze_and_tab_ownership(self):
        """Real secondary menu, both scroll axes, sorting + selection on a disposable database."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as seed:
            seed.execute("UPDATE customers SET customer_name = 'zulu' WHERE id = 1")
            seed.execute("UPDATE customers SET customer_name = 'alpha' WHERE id = 2")
            seed.executemany("INSERT INTO customers VALUES (?, ?)",
                             [(i, f"row-{i:02}") for i in range(3, 49)])
            for i in range(14):
                seed.execute(f"ALTER TABLE customers ADD COLUMN v_{i} INTEGER DEFAULT {i}")
            seed.execute("CREATE TABLE orders AS SELECT * FROM customers")
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        settings = self.root / "settings.json"
        config = {"ui": {"language": "en", "tablePageSize": 50},
                  "rowCountMode": "exact", "defaultTableSort": "primaryKeyAsc"}
        settings.write_text(json.dumps(config), encoding="utf-8")
        before = self.db.read_bytes()
        settings_before = settings.read_bytes()
        store_before = self.store.read_bytes()

        menu = "table-column-freeze-menu"
        freeze = "table-column-freeze-item0"
        unfreeze = "table-column-freeze-item1"

        def secondary(at, col):
            return {"t": at, "type": "secondary-click", "key": f"head-{col}"}

        def scroll(at):
            return {"t": at, "type": "drag", "key": "horizontal-scrollbar/thumb",
                    "dx": 240, "dur": .35}

        def grid(tree):
            def walk(node):
                yield node
                for child in node.get("children", ()):
                    yield from walk(child)
            return next(node for node in walk(tree["accessibility"])
                        if node.get("key") == "data-grid-scroll")

        def assert_unpinned(tree):
            self.assertGreater(grid(tree)["scroll"][0], 2 * 120)
            self.assertFalse(visible(tree, key="head-0"))
            self.assertFalse(visible(tree, key="head-1"))
            self.assertFalse(visible(tree, key="cell-0-0"))
            self.assertFalse(visible(tree, key=menu))

        def assert_pinned(tree, row, gutter=True):
            gb = self._settled_rect(tree, "data-grid-scroll")
            x, hb = gb[0] + (40 if gutter else 0), gb[1]
            for index in range(2):
                head = self._settled_rect(tree, f"head-{index}")
                cell = self._settled_rect(tree, f"cell-{row}-{index}")
                self.assertAlmostEqual(head[0], x, delta=.25)
                self.assertAlmostEqual(cell[0], x, delta=.25)
                self.assertAlmostEqual(head[1], hb, delta=.25)
                x += head[2]
            if gutter:
                self.assertAlmostEqual(self._settled_rect(tree, "head-rownum")[0], gb[0], delta=.25)
                self.assertAlmostEqual(self._settled_rect(tree, f"rownum-{row}")[0], gb[0], delta=.25)
            else:
                self.assertFalse(visible(tree, key="head-rownum"))
                self.assertFalse(visible(tree, key=f"rownum-{row}"))

        # The actual data grid starts unpinned, not simply painted with a
        # different color. Dismissing the menu is not an action or a sort.
        before_menu = [*OPEN, tap(4.8, "cell-1-1")]
        before_menu_tree = self.replay(before_menu, until=5.1)
        self._expect_table_header_sort(before_menu_tree, 1, "customer_name")
        self.assertEqual(grid(before_menu_tree)["activeDescendant"], "cell-1-1")
        opened = [*before_menu, secondary(5.3, 1)]
        tree = self.replay(opened, until=5.9)
        self.expect_rendered(tree, key=menu)
        self.expect_rendered(tree, text="Freeze through This Column", within=freeze)
        self.expect_rendered(tree, text="Unfreeze Columns", within=unfreeze)
        self.expect_rendered(tree, key="head-sort-neutral-1", within="head-1")

        unpinned = self.replay([*opened, key(6.2, "Escape"), scroll(6.9)], until=7.9)
        assert_unpinned(unpinned)
        self.assertTrue(any(
            node.get("key") == "data-grid-status"
            and (node.get("text") or "").endswith(" · selection r1c1..r1c1 · 0 dirty")
            for node in unpinned["elements"]
        ), "a virtualized selection remains logically selected after Escape and scroll")

        # The primary header gesture still drives genuine SQLite sorting.
        # Selected raw cell remains the same through a secondary menu + drag.
        pinned = [*OPEN, tap(5.0, "head-1"), tap(6.4, "cell-1-1"),
                  secondary(6.9, 1), tap(7.6, freeze), scroll(8.35)]
        pinned_tree = self.replay(pinned, until=9.35)
        assert_pinned(pinned_tree, 1)
        self.assertGreater(grid(pinned_tree)["scroll"][0], 240)
        self._expect_table_header_sort(pinned_tree, 1, "customer_name", "ascending")
        self.expect_rendered(pinned_tree, text="alpha", within="cell-0-1")
        self.assertEqual(grid(pinned_tree)["activeDescendant"], "cell-1-1")
        self.expect_rendered(pinned_tree, key="data-grid-status")
        self.assertFalse(visible(pinned_tree, key="pending-changes-bar"))

        # Both axes now move; stickiness is horizontal only for body rows.
        two_axis = [*pinned, {"t": 9.55, "type": "wheel", "key": "vertical-scrollbar", "dy": -230}]
        after_y = self.replay(two_axis, until=10.4)
        offset = grid(after_y)["scroll"][:2]
        self.assertGreater(offset[1], 0)
        assert_pinned(after_y, 12)
        out = self.replay([*two_axis, secondary(10.6, 1), tap(11.3, unfreeze)], until=12.2)
        assert_unpinned(out)
        self.assertEqual(grid(out)["scroll"][:2], offset, "unfreezing is not a scroll reset")

        # Another table has the same raw names, but does not inherit a
        # different tab's view state. Returning restores it without writing it.
        other_route = [*pinned, tap(9.6, "tree-table-orders"), scroll(11.2)]
        other = self.replay(other_route, until=12.2)
        assert_unpinned(other)
        back = self.replay([*other_route, tap(12.6, "tab-1"), scroll(14.0)], until=15)
        assert_pinned(back, 1)
        self._expect_table_header_sort(back, 1, "customer_name", "ascending")

        self.assertEqual(self.store.read_bytes(), store_before)
        self.assertEqual(settings.read_bytes(), settings_before)
        settings.write_text(json.dumps(dict(config, gridRowNumbers=False)), encoding="utf-8")
        no_gutter_settings = settings.read_bytes()
        no_gutter = self.replay(pinned, until=9.35)
        assert_pinned(no_gutter, 1, gutter=False)
        self.assertEqual(settings.read_bytes(), no_gutter_settings)
        self.assertEqual(self.db.read_bytes(), before)
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as oracle:
            self.assertEqual(oracle.execute("SELECT customer_name FROM customers WHERE id=1").fetchone(),
                             ("zulu",))
            self.assertEqual(oracle.execute("SELECT COUNT(*) FROM orders").fetchone(), (48,))
