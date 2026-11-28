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
        settings.write_text(json.dumps({
            "ui": {"language": "en", "themeMode": "light", "tablePageSize": 100},
            "rowCountMode": "exact", "defaultTableSort": "primaryKeyAsc",
        }), encoding="utf-8")
        settings.chmod(0o600)

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
            for name in ("data-grid-status", "grid-readonly", "grid-filter", "fk-navigation"):
                control = self._settled_rect(tree, name)
                self.assertGreaterEqual(control[1], previous, name)
                previous = bottom(control)
            self.assertGreaterEqual(frame[1], previous)
            self.assertAlmostEqual(pager[1] - bottom(frame), 8, delta=.25)
            self.assertEqual(pager[3], 28)
            self.assertAlmostEqual(bottom(pager), bottom(panel), delta=.25)
            self.assertAlmostEqual(bottom(pager), bar[1], delta=.25)
            self.expect_rendered(tree, key="filter-input")
            self.expect_rendered(tree, key="grid-page-label", text="Page 1 / 1")
            return frame

        table = self.replay([*OPEN, tap(4.8, "cell-1-1")], until=5.6)
        view, initial = bars(table, "")
        tall_frame = table_frame(table)
        self.assertEqual(initial["scroll"][:2], [0, 0])
        self.assertGreater(initial["scroll"][2], 0)
        self.assertEqual(initial["activeDescendant"], "cell-1-1")
        self.expect_rendered(table, text="fixture-lin", within="cell-1-1")
        self.expect_rendered(table, key="data-grid-status",
                             text="41×17 · window at column 0 · selection r1c1..r1c1 · 0 dirty")

        short = self.replay(OPEN, until=5.3, size="1440x640")
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
        frozen = self._settled_rect(dragged, "head-0")
        self.assertAlmostEqual(frozen[0], view[0] + 40, delta=.25)
        self.assertAlmostEqual(frozen[1], view[1], delta=.25)

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
        self.assertEqual(self._settled_rect(query, "result-data-grid-scroll-scroll-area")[3], 200)
        self.assertEqual(query_state["activeDescendant"], "result-cell-1-1")
        self.expect_rendered(query, text="fixture-lin", within="result-cell-1-1")
        self.assertEqual(self.db.read_bytes(), original)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())
