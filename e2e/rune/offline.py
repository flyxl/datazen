"""Rune migrations of legacy settings, grid, catalog and local file journeys."""
import csv
import json
import socket
import sqlite3

from scenarios import fixture, query_steps, tap, type_text, visible

OPEN = (tap(.5, "conn-open-e2e-sqlite"), tap(3, "tree-table-customers"))
SETTINGS = tap(.5, "rail-settings")
DATA_SETTINGS = tap(1.4, "settings-nav-dataBrowsing")
PAGE_SIZE = "settings-page-size-preset-trigger"
CUSTOM_PAGE_SIZE = "settings-page-size-custom"


def key(t, name, *, cmd=False):
    return {"t": t, "type": "key", "name": name, "cmd": cmd}


def open_custom_page_size(t):
    return [tap(t, PAGE_SIZE), tap(t + .65, "option5")]


def fill(t, element, value):
    return [tap(t, element), key(t + .25, "a", cmd=True),
            type_text(t + .45, value)]


class OfflineJourneys:
    """Mix into the native replay harness; no externally hosted service."""

    def test_settings_language_changes_other_ui_labels_and_survives_restart(self):
        settings = self.root / "settings.json"
        trigger = "settings-language-select-trigger"
        appearance = "settings-nav-appearance"
        opened = [SETTINGS, tap(1.5, trigger)]
        choices = self.replay(opened, until=2.5)
        self.expect_rendered(choices, text="外观", within=appearance)
        self.expect_rendered(choices, text="简体中文", within=trigger)
        self.expect_rendered(choices, key="option0")
        self.expect_rendered(choices, text="English", within="option0")
        self.expect_rendered(choices, key="option1")
        self.assertFalse(visible(choices, key="option2"))
        self.assertFalse(settings.exists(), "opening the selector must not save")

        changed = self.replay([*opened, tap(2.8, "option0")], until=4.2)
        self.expect_rendered(changed, text="Appearance", within=appearance)
        self.assertFalse(visible(changed, text="外观", within=appearance))
        self.assertFalse(visible(changed, key="settings-error"))
        self.assertEqual(json.loads(settings.read_text())["ui"]["language"], "en")

        restarted = self.replay([SETTINGS, tap(1.5, appearance)], until=2.8)
        self.expect_rendered(restarted, key="appearance-color-scheme-select-trigger")
        self.expect_rendered(restarted, text="Appearance", within=appearance)

        back = self.replay([*opened, tap(2.8, "option1")], until=4.2)
        self.expect_rendered(back, text="外观", within=appearance)
        self.assertEqual(json.loads(settings.read_text())["ui"]["language"], "zh-CN")
        restored = self.replay([SETTINGS], until=2.2)
        self.expect_rendered(restored, text="外观", within=appearance)
        self.expect_rendered(restored, key="settings-favorites")

    def test_settings_unsupported_saved_locale_falls_back_without_rewriting_file(self):
        settings = self.root / "settings.json"
        previous = '{"ui":{"language":"fr","queryMaxRows":23},"keep":"sentinel"}'
        settings.write_text(previous, encoding="utf-8")
        tree = self.replay([SETTINGS], until=2.3)
        self.expect_rendered(tree, text="外观", within="settings-nav-appearance")
        self.expect_rendered(tree, text="简体中文", within="settings-language-select-trigger")
        self.assertEqual(settings.read_text(encoding="utf-8"), previous)

        choice = self.replay([
            SETTINGS, tap(1.5, "settings-language-select-trigger"), tap(2.8, "option0"),
        ], until=4.2)
        self.expect_rendered(choice, text="Appearance", within="settings-nav-appearance")
        self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), {
            "ui": {"language": "en", "queryMaxRows": 23}, "keep": "sentinel",
        })

    def test_settings_language_failed_save_does_not_switch_rendered_language(self):
        blocked = self.root / "settings.json"
        blocked.mkdir()
        sentinel = blocked / "keep"
        sentinel.write_text("unchanged", encoding="utf-8")
        tree = self.replay([
            SETTINGS, tap(1.5, "settings-language-select-trigger"), tap(2.8, "option0"),
        ], until=4.4)
        self.expect_rendered(tree, key="settings-error")
        self.expect_rendered(tree, text="外观", within="settings-nav-appearance")
        self.expect_rendered(tree, text="简体中文", within="settings-language-select-trigger")
        self.assertFalse(visible(tree, key="option0"))
        self.assertEqual(sentinel.read_text(encoding="utf-8"), "unchanged")
        self.assertEqual(list(blocked.iterdir()), [sentinel])

    def test_settings_theme_persists_after_relaunch(self):
        path = self.root / "settings.json"
        path.write_text(json.dumps({"ui": {"language": "en", "themeMode": "light"}}),
                        encoding="utf-8")
        initial = path.read_bytes()
        appearance = tap(1.4, "settings-nav-appearance")
        scheme = "appearance-color-scheme-select-trigger"
        pack = "appearance-builtin-theme-status"
        closed = [SETTINGS, appearance]
        opened = [*closed, tap(2.3, scheme)]
        choices = self.replay(opened, until=3.1)
        self.expect_rendered(choices, text="Light", within=scheme)
        self.expect_rendered(choices, text="Light", within="option0")
        self.expect_rendered(choices, text="Dark", within="option1")
        self.expect_rendered(choices, text="System", within="option2")
        self.expect_rendered(choices, text="Built-in default", within=pack)
        self.assertFalse(visible(choices, key="appearance-theme-select-trigger"))
        self.assertEqual(path.read_bytes(), initial, "opening the real scheme selector does not save")

        dark = self.replay([*opened, tap(3.4, "option1")], until=4.6)
        self.assertFalse(visible(dark, key="option1"))
        self.expect_rendered(dark, text="Dark", within=scheme)
        settings = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(settings["ui"]["themeMode"], "dark")
        self.assertEqual(settings["ui"]["language"], "en")
        self.assertNotIn("themeDark", settings["ui"], "the current selector writes the canonical mode")

        saved = path.read_bytes()
        relaunched = self.replay(closed, until=2.8)
        self.expect_rendered(relaunched, text="Dark", within=scheme)
        self.expect_rendered(relaunched, text="Built-in default", within=pack)
        self.assertFalse(visible(relaunched, key="appearance-theme-select-trigger"))
        titlebar = [node for node in relaunched["elements"] if node.get("key") == "datazen-titlebar"]
        self.assertEqual([node["fill"] for node in titlebar], ["#0a0d14ff"],
                         "the reopened host uses the saved mode, not only the select label")
        self.assertEqual(path.read_bytes(), saved, "relaunch reads, but does not rewrite, the selection")

    def test_settings_theme_failed_save_keeps_previous_choice_and_reports_error(self):
        blocked = self.root / "settings.json"
        blocked.mkdir()
        sentinel = blocked / "keep"
        sentinel.write_bytes(b"unchanged")
        scheme = "appearance-color-scheme-select-trigger"
        popup = "appearance-color-scheme-select-content"
        enter_appearance = [SETTINGS, tap(1.4, "settings-nav-appearance")]
        open_scheme = [*enter_appearance, tap(2.3, scheme)]

        def identity(path):
            info = path.lstat()
            return (info.st_dev, info.st_ino, info.st_mode, info.st_size,
                    info.st_mtime_ns, info.st_ctime_ns)

        blocked_identity, sentinel_identity = identity(blocked), identity(sentinel)

        def expect_file_blocker_intact():
            self.assertTrue(blocked.is_dir())
            self.assertEqual(identity(blocked), blocked_identity)
            self.assertEqual(identity(sentinel), sentinel_identity)
            self.assertEqual(sentinel.read_bytes(), b"unchanged")
            self.assertEqual(list(blocked.iterdir()), [sentinel])

        def expect_previous_scheme(tree):
            expected = [label for label in ("System", "跟随系统")
                        if visible(tree, text=label, within=scheme)]
            self.assertEqual(len(expected), 1, "missing settings default to actual System mode")
            self.assertIs(tree["dark_system"], True, "the native replay runs with dark system appearance")
            for selector in ("datazen-titlebar", scheme, "appearance-builtin-theme-status"):
                painted = [node["fill"] for node in tree["elements"]
                           if node.get("key") == selector and not node.get("exiting")
                           and node.get("opacity", 0) > .95 and min(node["rect"][2:]) > 0]
                self.assertEqual(painted, ["#0a0d14ff"], selector)
            return expected[0]

        before = self.replay(enter_appearance, until=2.2)
        self.expect_rendered(before, key="settings-section-appearance")
        before_label = expect_previous_scheme(before)
        self.assertFalse(visible(before, key="settings-error"))
        self.assertFalse(visible(before, key=popup))
        expect_file_blocker_intact()

        options = self.replay(open_scheme, until=3.1)
        self.expect_rendered(options, key="option0", within=popup)
        light_label = {"System": "Light", "跟随系统": "浅色"}[before_label]
        self.expect_rendered(options, text=light_label, within="option0")
        self.assertFalse(visible(options, key="settings-error"))
        expect_file_blocker_intact()

        tree = self.replay([*open_scheme, tap(3.4, "option0")], until=4.8)
        self.expect_rendered(tree, key="settings-error")
        error_texts = [node["text"] for node in tree["elements"]
                       if "Is a directory" in (node.get("text") or "")]
        self.assertTrue(any(visible(tree, text=text, within="settings-error") for text in error_texts),
                        "the actual filesystem save failure is visible in Settings")
        self.assertEqual(expect_previous_scheme(tree), before_label)
        self.assertFalse(visible(tree, text=light_label, within=scheme))
        self.assertFalse(visible(tree, key=popup))
        expect_file_blocker_intact()

    def test_settings_page_size_and_query_limit_survive_restart(self):
        # These journeys assert the localized trigger caption as well as disk
        # values; pin English rather than depending on stock's locale fallback.
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"language": "en"}}), encoding="utf-8")
        tree = self.replay([
            SETTINGS, DATA_SETTINGS, *open_custom_page_size(2.1),
            *fill(3.35, "settings-page-size-input", "17"), tap(4.45, "settings-page-size-set"),
            *fill(5.15, "settings-max-rows-input", "23"), tap(6.25, "settings-max-rows-set"),
        ], until=7.35)
        self.assertFalse(visible(tree, key="settings-error"))
        ui = json.loads((self.root / "settings.json").read_text())["ui"]
        self.assertEqual((ui["tablePageSize"], ui["queryMaxRows"]), (17, 23))
        tree = self.replay([SETTINGS, DATA_SETTINGS], until=2.5)
        self.expect_rendered(tree, text="17 rows", within=PAGE_SIZE)
        self.expect_rendered(tree, text="Query max rows: 23", within="settings-data")

    def test_settings_page_size_preset_persists_without_changing_the_query_limit(self):
        config = self.root / "settings.json"
        initial = json.dumps({"ui": {"language": "en", "tablePageSize": 17, "queryMaxRows": 23}})
        config.write_text(initial, encoding="utf-8")
        preset = "settings-page-size-preset-trigger"

        cancelled = self.replay([
            SETTINGS, DATA_SETTINGS, tap(2.1, preset), key(3.3, "Escape"),
        ], until=4.3)
        self.expect_rendered(cancelled, text="17 rows", within=PAGE_SIZE)
        self.assertEqual(config.read_text(encoding="utf-8"), initial)

        applied = self.replay([
            SETTINGS, DATA_SETTINGS, tap(2.1, preset), tap(3.3, "option1"),
        ], until=4.3)
        self.expect_rendered(applied, text="50 rows", within=PAGE_SIZE)
        self.expect_rendered(applied, text="Query max rows: 23", within="settings-data")
        ui = json.loads(config.read_text(encoding="utf-8"))["ui"]
        self.assertEqual((ui["tablePageSize"], ui["queryMaxRows"]), (50, 23))

        restarted = self.replay([SETTINGS, DATA_SETTINGS], until=2.5)
        self.expect_rendered(restarted, text="50 rows", within=PAGE_SIZE)
        self.expect_rendered(restarted, key=preset)

    def test_settings_navigation_changes_real_grid_page_boundary_after_restart(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.execute("DELETE FROM customers")
            connection.executemany("INSERT INTO customers VALUES (?, ?)",
                                   [(i, f"row-{i:02}") for i in range(1, 53)])
            expected = {
                size: connection.execute(
                    "SELECT customer_name FROM customers ORDER BY id LIMIT 1 OFFSET ?",
                    (size,),
                ).fetchone()[0] for size in (17, 25)
            }
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"language": "en", "tablePageSize": 17, "queryMaxRows": 23}}),
            encoding="utf-8")
        next_page = [*OPEN, tap(5.0, "page-next")]
        first = self.replay(next_page, until=6.9)
        self.expect_rendered(first, key="grid-page-label", text="Page 2 / 4")
        self.expect_rendered(first, text=expected[17], within="cell-0-1")

        changed = self.replay([SETTINGS, DATA_SETTINGS, *open_custom_page_size(2.1),
                               *fill(3.35, "settings-page-size-input", "25"),
                               tap(4.45, "settings-page-size-set")], until=5.7)
        self.expect_rendered(changed, text="25 rows", within=PAGE_SIZE)
        self.assertFalse(visible(changed, key="settings-error"))
        settings = json.loads(self.root.joinpath("settings.json").read_text())["ui"]
        self.assertEqual((settings["tablePageSize"], settings["queryMaxRows"]), (25, 23))

        second = self.replay(next_page, until=6.9)
        self.expect_rendered(second, key="grid-page-label", text="Page 2 / 3")
        self.expect_rendered(second, text=expected[25], within="cell-0-1")

    def test_settings_invalid_page_size_is_not_saved(self):
        tree = self.replay([
            SETTINGS, DATA_SETTINGS, *open_custom_page_size(2.1),
            *fill(3.35, "settings-page-size-input", "abc"),
            tap(4.45, "settings-page-size-set"),
        ], until=5.7)
        self.expect_rendered(tree, key="settings-page-size-custom-error",
                             text="table page size must be a number", within=CUSTOM_PAGE_SIZE)
        self.assertFalse((self.root / "settings.json").exists())

    def test_settings_custom_page_size_discloses_and_cancels_without_writes(self):
        config = self.root / "settings.json"
        initial = json.dumps({"ui": {"language": "en", "tablePageSize": 17, "queryMaxRows": 23}})
        config.write_text(initial, encoding="utf-8")
        route = [SETTINGS, DATA_SETTINGS]
        idle = self.replay(route, until=2.75)
        self.expect_rendered(idle, text="17 rows", within=PAGE_SIZE)
        self.assertFalse(visible(idle, key="settings-page-size-input"))
        self.assertEqual(config.read_text(encoding="utf-8"), initial)

        # Native listbox's End/Enter selects the last disclosed option. The
        # numeric field owns focus before any write or direct field tap.
        keyboard = route + [tap(2.1, PAGE_SIZE), key(2.75, "End"), key(3.1, "Enter")]
        opened = self.replay(keyboard, until=4.1)
        self.expect_rendered(opened, key=CUSTOM_PAGE_SIZE)
        self.assertTrue(any(n.get("focused") and not n.get("exiting")
                            and "settings-page-size-input/" in n["path"] for n in opened["elements"]))
        self.assertEqual(config.read_text(encoding="utf-8"), initial)

        typed = keyboard + [key(3.65, "a", cmd=True), type_text(3.9, "31")]
        for close in (key(4.75, "Escape"), {"t": 4.75, "type": "tap", "x": 1408, "y": 870}):
            dismissed = self.replay([*typed, close], until=5.65)
            self.assertFalse(visible(dismissed, key=CUSTOM_PAGE_SIZE))
            self.expect_rendered(dismissed, text="17 rows", within=PAGE_SIZE)
            self.assertTrue(any(n.get("focused") and not n.get("exiting") and n.get("key") == PAGE_SIZE
                                for n in dismissed["elements"]))
            self.assertEqual(config.read_text(encoding="utf-8"), initial)

        saved = self.replay([*typed, key(4.75, "Enter")], until=5.85)
        self.assertFalse(visible(saved, key=CUSTOM_PAGE_SIZE))
        self.expect_rendered(saved, text="31 rows", within=PAGE_SIZE)
        ui = json.loads(config.read_text(encoding="utf-8"))["ui"]
        self.assertEqual((ui["tablePageSize"], ui["queryMaxRows"]), (31, 23))
        restarted = self.replay(route, until=2.75)
        self.expect_rendered(restarted, text="31 rows", within=PAGE_SIZE)

    def test_filter_grid_matches_sqlite_predicate_and_can_be_cleared(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT customer_name FROM customers WHERE customer_name LIKE ?",
                                  ("%lin%",)).fetchone()[0]
        filtered = [*OPEN, tap(4.5, "filter-column"), *fill(5, "filter-input", "lin"),
                    key(6.1, "Enter")]
        tree = self.replay(filtered, until=7.8)
        self.expect_rendered(tree, text=expected, within="data-grid")
        self.expect_rendered(tree, text="1 rows", within="grid-filter")
        self.assertFalse(visible(tree, text="fixture-ada", within="data-grid"))

        tree = self.replay([*filtered, tap(8, "filter-input"), key(8.35, "a", cmd=True),
                            key(8.6, "Backspace"), key(9, "Enter")], until=10.5)
        self.expect_rendered(tree, text="fixture-ada", within="data-grid")
        self.expect_rendered(tree, text=expected, within="data-grid")
        self.expect_rendered(tree, text="2 rows", within="grid-filter")

    def test_grid_header_cycles_sort_order(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            asc, desc = [row[0] for row in db.execute(
                "SELECT customer_name FROM customers ORDER BY customer_name")]
        actions = list(OPEN)
        for count, (direction, first) in enumerate(((" ↑", asc), (" ↓", desc), ("", asc)), 1):
            with self.subTest(direction=direction or "unsorted"):
                at = 3.8 + count * 1.35
                actions.append(tap(at, "head-1"))
                tree = self.replay(actions, until=at + 1.2)
                self.expect_rendered(tree, text=f"customer_name{direction}", within="head-1")
                self.expect_rendered(tree, text=first, within="cell-0-1")

    def test_legacy_table_cell_selection_clears_on_escape_and_page_change(self):
        """Migrate TC-SEL-001/004/006 on a real paged SQLite table."""
        fixture(self.store, self.db)
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"tablePageSize": 1}}), encoding="utf-8")
        with sqlite3.connect(self.db) as db:
            first, second = [row[0] for row in db.execute(
                "SELECT customer_name FROM customers ORDER BY id")]

        def fill_color(tree):
            return next(node["fill"] for node in tree["elements"]
                        if node.get("key") == "cell-0-1" and node.get("role") == "gridcell")

        no_selection = "1×2 · window at column 0 · selection none · 0 dirty"
        initial = self.replay(OPEN, until=4.8)
        self.expect_rendered(initial, key="data-grid-status", text=no_selection)
        plain_fill = fill_color(initial)
        chosen = [*OPEN, tap(5, "cell-0-1")]
        first_tree = self.replay(chosen, until=6.3)
        self.expect_rendered(first_tree, text=first, within="cell-0-1")
        self.expect_rendered(first_tree, key="data-grid-status",
                             text="1×2 · window at column 0 · selection r0c1..r0c1 · 0 dirty")
        self.assertNotEqual(fill_color(first_tree), plain_fill)

        cleared = self.replay([*chosen, key(6.5, "Escape")], until=7.8)
        self.expect_rendered(cleared, key="data-grid-status", text=no_selection)
        self.assertEqual(fill_color(cleared), plain_fill)
        self.expect_rendered(cleared, text=first, within="cell-0-1")
        self.assertFalse(visible(cleared, key="grid-editor"))

        next_page = self.replay([*chosen, tap(6.5, "page-next")], until=8.2)
        self.expect_rendered(next_page, key="grid-page-label", text="Page 2 / 2")
        self.expect_rendered(next_page, key="data-grid-status", text=no_selection)
        self.assertEqual(fill_color(next_page), plain_fill)
        self.expect_rendered(next_page, text=second, within="cell-0-1")
        self.assertFalse(visible(next_page, text=first, within="data-grid"))

    def test_legacy_table_row_gutter_selects_exactly_one_complete_row(self):
        """Migrate TC-SEL-005 with a real three-column, paged SQLite table."""
        fixture(self.store, self.db)
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"tablePageSize": 2}}), encoding="utf-8")
        with sqlite3.connect(self.db) as database:
            database.executescript(
                "ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0;"
                "UPDATE customers SET score = 100 + id;"
                "INSERT INTO customers VALUES (3, 'fixture-next-page', 103);")
            expected = database.execute("SELECT * FROM customers ORDER BY id").fetchall()

        def paints(tree, index):
            grid = {node.get("key"): node["fill"] for node in tree["elements"]
                    if node.get("role") in {"rowheader", "gridcell"}}
            return [grid[f"cell-{index}-{col}"] for col in range(3)], grid[f"rownum-{index}"]

        plain = self.replay(OPEN, until=4.8)
        for row, values in enumerate(expected[:2]):
            self.expect_rendered(plain, text=str(row+1), within=f"rownum-{row}")
            for col, value in enumerate(values):
                self.expect_rendered(plain, text=str(value), within=f"cell-{row}-{col}")
        original = [paints(plain, i) for i in range(2)]
        selected = [*OPEN, tap(5, "rownum-1")]
        one = self.replay(selected, until=6.2)
        self.expect_rendered(one, key="data-grid-status",
                             text="2×3 · window at column 0 · selection row 2 · 0 dirty")
        self.assertEqual(paints(one, 0), original[0])
        target_cells, target_gutter = paints(one, 1)
        self.assertTrue(all(a != b for a, b in zip(target_cells, original[1][0])))
        self.assertNotEqual(target_gutter, original[1][1])
        self.assertEqual(target_cells, [target_gutter] * 3, "no distinct active cell in row mode")
        self.assertFalse(visible(one, key="grid-editor"))

        single = self.replay([*selected, tap(6.5, "cell-0-2")], until=7.6)
        self.expect_rendered(single, key="data-grid-status",
                             text="2×3 · window at column 0 · selection r0c2..r0c2 · 0 dirty")
        self.assertEqual(paints(single, 1), original[1])
        self.assertEqual(paints(single, 0)[0][:2], original[0][0][:2])
        self.assertNotEqual(paints(single, 0)[0][2], original[0][0][2])

        cleared = self.replay([*selected, key(6.5, "Escape")], until=7.6)
        self.expect_rendered(cleared, key="data-grid-status",
                             text="2×3 · window at column 0 · selection none · 0 dirty")
        self.assertEqual([paints(cleared, i) for i in range(2)], original)
        paged = self.replay([*selected, tap(6.5, "page-next")], until=8.2)
        self.expect_rendered(paged, key="grid-page-label", text="Page 2 / 2")
        self.expect_rendered(paged, key="data-grid-status",
                             text="1×3 · window at column 0 · selection none · 0 dirty")
        self.assertEqual(paints(paged, 0), original[0])
        self.expect_rendered(paged, text=expected[2][1], within="cell-0-1")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as database:
            self.assertEqual(database.execute("SELECT * FROM customers ORDER BY id").fetchall(), expected)

    def test_legacy_table_shift_click_extends_from_original_cell_anchor(self):
        """Port TC-SEL-003 with native modifier events and a real SQLite grid."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript(
                "ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0;"
                "UPDATE customers SET score=100+id;"
                "INSERT INTO customers VALUES (3, 'fixture-grace', 103), (4, 'fixture-decoy', 104);")
            expected = db.execute("SELECT * FROM customers ORDER BY id").fetchall()

        def paints(tree):
            return {node["key"]: node["fill"] for node in tree["elements"]
                    if node.get("role") == "gridcell" and (node.get("key") or "").startswith("cell-")}

        plain = self.replay(OPEN, until=4.8)
        normal = paints(plain)
        start = [*OPEN, tap(5.0, "cell-0-1")]
        shift = [{"t": 6.1, "hold": "shift"}, tap(6.4, "cell-2-2"),
                 {"t": 6.8, "hold": ""}]
        expanded = self.replay([*start, *shift], until=7.9)
        self.expect_rendered(expanded, key="data-grid-status",
                             text="4×3 · window at column 0 · selection r0c1..r2c2 · 0 dirty")
        actual = paints(expanded)
        self.assertEqual(set(actual), set(normal))
        self.assertEqual({name for name in actual if actual[name] != normal[name]},
                         {f"cell-{r}-{c}" for r in range(3) for c in (1, 2)})
        self.expect_rendered(expanded, text=str(expected[2][2]), within="cell-2-2")
        self.expect_rendered(expanded, text=expected[3][1], within="cell-3-1")
        self.assertFalse(visible(expanded, key="grid-editor"))

        shrink = [{"t": 8.0, "hold": "shift"}, tap(8.3, "cell-1-1"),
                  {"t": 8.7, "hold": ""}]
        shrunk = self.replay([*start, *shift, *shrink], until=9.7)
        self.expect_rendered(shrunk, key="data-grid-status",
                             text="4×3 · window at column 0 · selection r0c1..r1c1 · 0 dirty")
        shrunk_paints = paints(shrunk)
        self.assertEqual({name for name in normal if shrunk_paints[name] != normal[name]},
                         {"cell-0-1", "cell-1-1"})

        clicked = self.replay([*start, *shift, tap(8.1, "cell-3-0")], until=9.6)
        self.expect_rendered(clicked, key="data-grid-status",
                             text="4×3 · window at column 0 · selection r3c0..r3c0 · 0 dirty")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), expected)

    def test_grid_next_and_previous_page_use_saved_page_size(self):
        fixture(self.store, self.db)
        self.root.joinpath("settings.json").write_text(
            json.dumps({"ui": {"tablePageSize": 1}}), encoding="utf-8")
        nxt = [*OPEN, tap(5, "page-next")]
        tree = self.replay(nxt, until=6.8)
        self.expect_rendered(tree, key="grid-page-label", text="Page 2 / 2")
        self.expect_rendered(tree, text="fixture-lin", within="cell-0-1")
        self.assertFalse(visible(tree, text="fixture-ada", within="data-grid"))
        tree = self.replay([*nxt, tap(7, "page-prev")], until=8.8)
        self.expect_rendered(tree, key="grid-page-label", text="Page 1 / 2")
        self.expect_rendered(tree, text="fixture-ada", within="cell-0-1")

    def test_sqlite_columns_and_indexes_match_catalog(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE INDEX idx_e2e_customer_name ON customers(customer_name)")
            column = db.execute("PRAGMA table_info(customers)").fetchall()[1][1]
            index = db.execute("PRAGMA index_list(customers)").fetchone()[1]
        for view, within, expected in (("columns", "columns-list", column),
                                       ("indexes", "indexes-list", index)):
            with self.subTest(view=view):
                tree = self.replay([*OPEN, tap(5, f"view-{view}")], until=6.8)
                self.expect_rendered(tree, text=expected, within=within)

    def test_sqlite_foreign_key_matches_catalog(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER "
                       "REFERENCES customers(id))")
            target = db.execute("PRAGMA foreign_key_list(orders)").fetchone()[2]
        tree = self.replay([OPEN[0], tap(3, "tree-table-orders"), tap(5, "view-fks")], until=6.8)
        self.expect_rendered(tree, text=f"{target} (id)", within="fks-list")
        self.expect_rendered(tree, text="customer_id", within="fks-list")

    def test_sqlite_query_view_returns_real_rows(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE VIEW e2e_customer_view AS SELECT customer_name FROM customers WHERE id=2")
            expected = db.execute("SELECT * FROM e2e_customer_view").fetchone()[0]
        tree = self.replay(query_steps("SELECT * FROM e2e_customer_view;"), until=11)
        self.expect_rendered(tree, text=expected, within="result-workspace")
        self.assertFalse(visible(tree, text="fixture-ada", within="result-workspace"))

    def test_csv_export_matches_sqlite_and_cancel_writes_nothing(self):
        fixture(self.store, self.db)
        destination = self.root / "customers.csv"
        actions = [*OPEN, tap(4.4, "rail-settings"), tap(5.2, "settings-nav-dataBrowsing"),
                   tap(6.1, "export-start")]
        tree = self.replay([*actions, {"t": 7, "type": "file-dialog-cancel"}], until=8.4)
        self.expect_rendered(tree, text="the export was cancelled; nothing was written",
                             within="export-error")
        self.assertFalse(destination.exists())
        self.assertFalse(list(self.root.rglob("*.csv")))
        self.replay([*actions, {"t": 7, "type": "file-dialog", "path": str(destination)}], until=10)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT CAST(id AS TEXT), customer_name FROM customers").fetchall()
        with destination.open(newline="", encoding="utf-8") as stream:
            rows = list(csv.reader(stream))
        self.assertEqual(rows[0], ["id", "customer_name"])
        self.assertCountEqual([tuple(row) for row in rows[1:]], expected)

    def test_json_export_preserves_sqlite_types_and_escaped_text(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("INSERT INTO customers VALUES (?, ?)", (3, 'line 1\nline 2, "quoted"'))
            expected = [dict(zip(("id", "customer_name"), row))
                        for row in db.execute("SELECT id, customer_name FROM customers")]
        destination = self.root / "customers.json"
        self.replay([
            *OPEN, tap(4.4, "rail-settings"), tap(5.2, "settings-nav-dataBrowsing"),
            tap(6, "export-format"), tap(6.7, "export-start"),
            {"t": 7.4, "type": "file-dialog", "path": str(destination)},
        ], until=10.5)
        self.assertCountEqual(json.loads(destination.read_text(encoding="utf-8")), expected)

    def test_sqlite_backup_restores_in_an_independent_database(self):
        fixture(self.store, self.db)
        destination = self.root / "customers.sql"
        self.replay([
            *OPEN, tap(4.4, "rail-settings"), tap(5.2, "settings-nav-dataBrowsing"),
            tap(6.4, "backup-start"),
            {"t": 7.3, "type": "file-dialog", "path": str(destination)},
        ], until=10.5)
        with sqlite3.connect(self.db) as db:
            expected = db.execute("SELECT id, customer_name FROM customers ORDER BY id").fetchall()
        with sqlite3.connect(":memory:") as restored:
            restored.executescript(destination.read_text(encoding="utf-8"))
            self.assertEqual(restored.execute(
                "SELECT id, customer_name FROM customers ORDER BY id").fetchall(), expected)

    def test_query_result_find_moves_between_matches(self):
        fixture(self.store, self.db)
        sql = "SELECT customer_name FROM customers ORDER BY id;"
        search = [*query_steps(sql), *fill(10.7, "result-find-input", "fixture-"),
                  tap(12.0, "result-find-apply")]
        tree = self.replay(search, until=13)
        self.expect_rendered(tree, key="result-find-count", text="1 of 2")
        tree = self.replay([*search, tap(13, "result-find-next")], until=14.2)
        self.expect_rendered(tree, key="result-find-count", text="2 of 2")

    def test_settings_sql_syntax_theme_select_colors_query_and_survives_restart(self):
        fixture(self.store, self.db)
        path = self.root / "settings.json"
        original = {"sqlSyntaxTheme": "future-unknown", "keep": True,
                    "ui": {"language": "en", "themeMode": "dark"}}
        path.write_text(json.dumps(original), encoding="utf-8")
        before = path.read_bytes()
        trigger = "settings-sql-syntax-theme-trigger"
        start = [SETTINGS, tap(1.4, "settings-nav-editor")]
        opened = [*start, tap(2.3, trigger)]
        choices = self.replay(opened, until=3.1)
        self.expect_rendered(choices, text="Default (DataZen)", within=trigger)
        self.expect_rendered(choices, text="Dracula", within="option5")
        self.assertEqual(path.read_bytes(), before)
        canceled = self.replay([*opened, key(3.4, "Escape")], until=4.4)
        self.expect_rendered(canceled, text="Default (DataZen)", within=trigger)
        self.assertEqual(path.read_bytes(), before)

        def fill_at(tree, key):
            self.expect_rendered(tree, key=key)
            [fill_value] = [node.get("fill") for node in tree["elements"]
                            if node.get("key") == key]
            return fill_value

        def expect_stacked_preview(tree):
            select = lambda key: next(node for node in tree["elements"] if node.get("key") == key)
            viewport = select("settings-sql-syntax-preview-scroll")
            vx, vy, vw, vh = viewport["rect"]
            self.assertGreater(vh, 100)
            first_x, previous_bottom = None, vy
            for index, text in enumerate((
                "-- Sample query preview",
                "SELECT u.name, COUNT(o.id) AS total",
                "FROM users u",
                "WHERE u.status = 'active' AND u.age > 18",
                "ORDER BY total DESC;",
            )):
                line_key = f"settings-sql-syntax-preview-line-{index}"
                self.expect_rendered(tree, key=line_key, text=text)
                line = select(line_key)
                self.assertTrue(line["path"].startswith(viewport["path"] + "/"))
                x, y, width, height = line["rect"]
                self.assertGreater(width, 0)
                self.assertGreater(height, 0)
                if first_x is None:
                    first_x = x
                self.assertAlmostEqual(x, first_x, delta=0.1)
                self.assertGreaterEqual(x, vx)
                self.assertLessEqual(x + width, vx + vw + 0.1)
                self.assertGreaterEqual(y, previous_bottom - 0.1)
                self.assertLessEqual(y + height, vy + vh + 0.1)
                previous_bottom = y + height

        expect_stacked_preview(canceled)
        applied = self.replay([*opened, tap(3.4, "option5")], until=4.4)
        expect_stacked_preview(applied)
        self.expect_rendered(applied, text="Dracula", within=trigger)
        self.assertEqual(fill_at(applied, "settings-sql-syntax-preview"), "#282a36ff")
        self.assertEqual(json.loads(path.read_text()), {**original, "sqlSyntaxTheme": "dracula"})

        sql = "SELECT customer_name FROM customers WHERE id = 1;"
        query = self.replay(query_steps(sql), until=11)
        self.assertEqual(fill_at(query, "sql-editor-body"), "#282a36ff")
        self.expect_rendered(query, text="fixture-ada", within="result-workspace")

        restored = self.replay([*opened, tap(3.4, "option0")], until=4.4)
        expect_stacked_preview(restored)
        self.expect_rendered(restored, text="Default (DataZen)", within=trigger)
        self.assertNotEqual(fill_at(restored, "settings-sql-syntax-preview"), "#282a36ff")
        self.assertEqual(json.loads(path.read_text()), {**original, "sqlSyntaxTheme": "default"})

        query = self.replay(query_steps(sql), until=11)
        self.assertNotEqual(fill_at(query, "sql-editor-body"), "#282a36ff")
        self.expect_rendered(query, text="fixture-ada", within="result-workspace")

    def test_settings_snippet_survives_restart_then_can_be_deleted(self):
        editor = tap(1.4, "settings-nav-editor")
        self.replay([
            SETTINGS, editor, *fill(2.3, "snippet-name-input", "Rune local snippet"),
            *fill(3.8, "snippet-sql-input", "SELECT 42;"), tap(5.1, "snippet-save"),
        ], until=6.3)
        target = self.root / "snippets.json"
        self.assertIn("SELECT 42;", target.read_text(encoding="utf-8"))
        tree = self.replay([SETTINGS, editor], until=3)
        self.expect_rendered(tree, text="Rune local snippet", within="settings-snippets")
        tree = self.replay([SETTINGS, editor, tap(2.5, "snippet-delete-0")], until=3.8)
        self.assertFalse(visible(tree, text="Rune local snippet", within="settings-snippets"))
        self.assertNotIn("SELECT 42;", target.read_text(encoding="utf-8"))

    def test_settings_import_validates_missing_source_without_writing(self):
        tree = self.replay([SETTINGS, DATA_SETTINGS, tap(2.6, "import-next")], until=4.1)
        self.expect_rendered(tree, text="a source is required", within="import-error")
        self.expect_rendered(tree, key="import-step-1")
        self.assertFalse(visible(tree, key="import-step-2"))
        self.assertFalse(self.db.exists())

    def test_connection_search_filters_by_name_case_insensitively(self):
        fixture(self.store, self.db)
        store = json.loads(self.store.read_text())
        store["connections"].append({**store["connections"][0], "id": "sandbox-local",
                                     "name": "Sandbox Local"})
        self.store.write_text(json.dumps(store), encoding="utf-8")
        search = fill(.5, "navigator-filter-input", "sAnDbOx")
        tree = self.replay(search, until=2)
        self.expect_rendered(tree, key="nav-conn-sandbox-local")
        self.assertFalse(visible(tree, key="nav-conn-e2e-sqlite"))
        tree = self.replay([*search, *fill(2.2, "navigator-filter-input", "no-match")], until=4)
        self.expect_rendered(tree, key="nav-group-all-empty")
        self.assertFalse(visible(tree, key="nav-conn-sandbox-local"))
        self.assertEqual(json.loads(self.store.read_text()), store)

    def test_legacy_connection_search_matches_saved_host_without_connecting(self):
        """Migrate the host-address case from connection-search-group.ts."""
        before = {"connections": [
            {"id": "primary", "name": "Reporting", "databaseType": "postgres",
             "host": "warehouse-a.invalid", "port": 5432},
            {"id": "secondary", "name": "Billing", "databaseType": "postgres",
             "host": "warehouse-b.invalid", "port": 5432},
            {"id": "local-file", "name": "Snapshot", "databaseType": "sqlite",
             "database": str(self.db)},
        ]}
        self.store.write_text(json.dumps(before), encoding="utf-8")
        keys = ("nav-conn-primary", "nav-conn-secondary", "nav-conn-local-file")
        initial = self.replay([], until=1)
        for row in keys:
            self.expect_rendered(initial, key=row)

        target = "navigator-filter-input"
        filtered = fill(.5, target, "HOUSE-a.INVALID")
        tree = self.replay(filtered, until=2)
        self.expect_rendered(tree, key=keys[0])
        for row in keys[1:]:
            self.assertFalse(visible(tree, key=row))
        self.assertFalse(visible(tree, key="nav-group-all-empty"))

        cleared = [*filtered, tap(2.15, target), key(2.4, "a", cmd=True), key(2.65, "Backspace")]
        tree = self.replay(cleared, until=3.5)
        for row in keys:
            self.expect_rendered(tree, key=row)
        self.assertEqual(json.loads(self.store.read_text(encoding="utf-8")), before)
        self.assertFalse(self.db.exists())

    def test_legacy_connection_probe_rejects_missing_host_and_invalid_port(self):
        """Migrate TC-CONN-005: validation is visible before any connection attempt."""
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        create = tap(.5, "connections-create-first")
        field = "connection-address-input"
        blank = [create, tap(1.3, field), key(1.6, "a", cmd=True), key(1.85, "Backspace"),
                 tap(2.4, "connection-test")]
        tree = self.replay(blank, until=3.3)
        self.expect_rendered(tree, text="Host or address is required.",
                             within="connection-probe-status")
        self.expect_rendered(tree, key="connection-dialog")
        self.expect_rendered(tree, key="connection-test")
        invalid = [*blank, *fill(3.6, field, "127.0.0.1:no-port")]
        tree = self.replay(invalid, until=4.9)
        self.assertFalse(visible(tree, key="connection-probe-status"))
        tree = self.replay([*invalid, tap(5, "connection-test")], until=6)
        self.expect_rendered(tree, text="Use host or host:port with a numeric port.",
                             within="connection-probe-status")
        self.assertFalse(self.store.exists())

    def test_legacy_connection_probe_fails_for_unreachable_loopback_endpoint(self):
        """Migrate TC-CONN-006 without DNS or an external network dependency."""
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        # Keep this assigned port reserved but never listen: no other test
        # can accidentally occupy it and make the negative case succeed.
        with socket.socket() as unused_endpoint:
            unused_endpoint.bind(("127.0.0.1", 0))
            port = unused_endpoint.getsockname()[1]
            actions = [tap(.5, "connections-create-first"),
                       *fill(1.4, "connection-address-input", f"127.0.0.1:{port}"),
                       tap(3, "connection-test")]
            tree = self.replay(actions, until=10.5)
        self.expect_rendered(tree, text="Could not connect. Check the address and that the server is running.",
                             within="connection-probe-status")
        self.expect_rendered(tree, key="connection-dialog")
        self.expect_rendered(tree, key="connection-test")
        self.assertFalse(self.store.exists())

    def test_connection_dialog_cancel_does_not_save_draft(self):
        actions = [tap(.5, "connections-create-first"),
                   *fill(1.4, "connection-name-input", "Unsaved draft")]
        tree = self.replay(actions, until=2.7)
        self.expect_rendered(tree, key="connection-dialog")
        tree = self.replay([*actions, tap(2.9, "dialog-cancel")], until=4)
        self.expect_rendered(tree, key="connections-empty")
        self.assertFalse(visible(tree, key="connection-dialog"))
        self.assertFalse(self.store.exists())

    def test_legacy_sqlite_connection_form_requires_only_a_database_file_path(self):
        """Migrate the SQLite-specific path field from new-connection.ts."""
        opened = [tap(.5, "connections-create-first"),
                  tap(1.5, "connection-engine-option-sqlite")]
        tree = self.replay(opened, until=2.5)
        self.expect_rendered(tree, key="connection-name-input", within="connection-section-basic")
        self.expect_rendered(tree, key="connection-database-input", within="connection-section-file")
        self.assertFalse(visible(tree, key="connection-engine-input"))
        for network_field in ("address", "username", "password"):
            self.assertFalse(visible(tree, key=f"connection-{network_field}-input"))

        tree = self.replay([*opened, tap(2.6, "connection-save")], until=4)
        self.expect_rendered(tree, text="a database file path is required",
                             within="connection-form-error")
        self.expect_rendered(tree, key="connection-dialog")
        self.assertFalse(self.store.exists())
        self.assertFalse(self.db.exists())

    def test_sqlite_chart_view_uses_numeric_results_and_returns_to_grid(self):
        fixture(self.store, self.db)
        sql = "SELECT 'Ada' AS label, id AS metric FROM customers ORDER BY id;"
        chart = [*query_steps(sql), tap(11, "result-view-chart")]
        tree = self.replay(chart, until=12.4)
        for index in (0, 1):
            self.expect_rendered(tree, key=f"result-block-{index}", within="result-chart")
        self.assertFalse(visible(tree, key="result-grid"))
        tree = self.replay([*chart, tap(12.8, "result-view-table")], until=14.2)
        self.expect_rendered(tree, text="Ada", within="result-cell-0-0")
        self.expect_rendered(tree, text="2", within="result-cell-1-1")

    def test_legacy_query_chart_table_exports_selected_retained_sqlite_result(self):
        """Migrate query-result-chart-journey.ts with real CSV and independent SQLite oracles."""
        fixture(self.store, self.db)
        statement = ("SELECT category, amount, missing, empty FROM export_probe "
                     "WHERE amount != 99 ORDER BY id;")
        mutation = "UPDATE export_probe SET category='changed after SELECT' WHERE id=1;"
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE export_probe (id INTEGER PRIMARY KEY, category TEXT NOT NULL, "
                       "amount INTEGER NOT NULL, missing TEXT, empty TEXT NOT NULL)")
            db.executemany("INSERT INTO export_probe VALUES (?, ?, ?, ?, ?)", [
                (1, '雪, "quoted"\r\nsecond line', 12, None, ""),
                (2, "negative", -7, "set", ""),
                (3, "excluded", 99, None, "control"),
            ])
            headers = [column[0] for column in db.execute(statement).description]
            before = db.execute(statement).fetchall()
        destination = self.root / "retained.csv"
        overwrite = self.root / "protected.csv"

        initial = [*query_steps(statement)[:-2],
                   {"t": 4.9, "type": "paste", "text": statement},
                   tap(8, "sql-run")]
        chart_steps = [*initial, tap(10.8, "result-view-chart")]
        chart = self.replay(chart_steps, until=12.2)
        self.expect_rendered(chart, key="result-block-0", within="result-chart")
        self.expect_rendered(chart, key="result-block-1", within="result-chart")
        self.assertFalse(visible(chart, key="result-grid"))
        self.assertFalse(visible(chart, key="data-table-export"))
        to_table = [*chart_steps, tap(12.4, "result-view-table")]
        review = [*to_table, tap(14.2, "data-table-export")]
        dialog = self.replay(review, until=15.7)
        self.expect_rendered(dialog, key="data-export-dialog")
        self.expect_rendered(dialog, text="CSV · 2 × 4", within="data-export-dialog")
        self.expect_rendered(dialog, text="12", within="result-cell-0-1")
        self.assertFalse(list(self.root.rglob("*.csv")))
        cancelled = self.replay([
            *review, tap(16, "query-csv-dialog-confirm"),
            {"t": 17.2, "type": "file-dialog-cancel"},
        ], until=19)
        self.expect_rendered(cancelled, key="query-csv-cancelled", within="result-workspace")
        self.assertFalse(visible(cancelled, key="query-csv-saved"))
        self.assertFalse(list(self.root.rglob("*.csv")))

        restore = [*to_table, *fill(14.6, "sql-editor-scroll", mutation), tap(18, "sql-run"),
                   tap(21.2, "query-result-tab-1"), tap(22.8, "data-table-export")]
        # Distinct new and old tab contents: the save is of the first retained
        # result, not the editor's current SQL, a fresh DB read, or the later tab.
        saved = self.replay([
            *restore, tap(24.6, "query-csv-dialog-confirm"),
            {"t": 25.7, "type": "file-dialog", "path": str(destination)},
        ], until=29, timeout=95)
        self.expect_rendered(saved, key="query-csv-saved", within="result-workspace")
        self.expect_rendered(saved, text="12", within="result-cell-0-1")
        self.assertFalse(visible(saved, key="query-csv-error"))
        with destination.open(newline="", encoding="utf-8") as stream:
            records = list(csv.reader(stream))
        self.assertEqual(records, [headers, *[
            ["" if value is None else str(value) for value in row] for row in before
        ]])
        self.assertIn(b'12,,""\n', destination.read_bytes())  # SQL NULL vs literal ""
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            after = db.execute(statement).fetchall()
        self.assertEqual(after, [("changed after SELECT", *before[0][1:]), before[1]])
        recorded = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
        self.assertEqual([row["sql"].rstrip(";") for row in recorded[-2:]],
                         [statement.rstrip(";"), mutation.rstrip(";")])

        overwrite.write_bytes(b"existing value")
        refused = self.replay([
            *review, tap(16, "query-csv-dialog-confirm"),
            {"t": 17.2, "type": "file-dialog", "path": str(overwrite)},
        ], until=20, timeout=80)
        self.expect_rendered(refused, key="query-csv-error", within="result-workspace")
        self.assertEqual(overwrite.read_bytes(), b"existing value")

    def test_sqlite_read_only_profile_rejects_query_writes(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_text())
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.replay(query_steps("UPDATE customers SET customer_name='must-not-write' WHERE id=1;"),
                    until=11)
        with sqlite3.connect(self.db) as db:
            self.assertEqual(db.execute(
                "SELECT customer_name FROM customers WHERE id=1").fetchone(), ("fixture-ada",))

    def test_settings_page_size_and_default_sort_do_not_share_popup_or_draft(self):
        config = self.root / "settings.json"
        config.write_text(json.dumps({
            "ui": {"language": "en", "tablePageSize": 17, "queryMaxRows": 23},
            "defaultTableSort": "primaryKeyDesc", "unrelated": ["keep"],
        }), encoding="utf-8")
        initial = config.read_bytes()
        sort = "settings-default-table-sort-trigger"
        sort_menu = "settings-default-table-sort-content"
        page_menu = "settings-page-size-preset-content"
        page_edit = [SETTINGS, DATA_SETTINGS, *open_custom_page_size(2.1),
                     *fill(3.35, "settings-page-size-input", "31")]
        page_open = self.replay(page_edit, until=4.5)
        self.expect_rendered(page_open, key=CUSTOM_PAGE_SIZE)
        self.assertFalse(visible(page_open, key=page_menu))
        self.assertFalse(visible(page_open, key=sort_menu))
        self.assertEqual(config.read_bytes(), initial)

        after_page = [*page_edit, key(4.75, "Escape"), tap(5.65, sort)]
        sort_open = self.replay(after_page, until=6.55)
        self.assertFalse(visible(sort_open, key=CUSTOM_PAGE_SIZE))
        self.assertFalse(visible(sort_open, key=page_menu))
        for label in ("Primary key, ascending", "Primary key, descending", "Do not sort"):
            self.expect_rendered(sort_open, text=label, within=sort_menu)
        self.expect_rendered(sort_open, text="17 rows", within=PAGE_SIZE)
        self.assertEqual(config.read_bytes(), initial)

        page_again = self.replay(
            [*after_page, key(6.8, "Escape"), tap(7.75, PAGE_SIZE)], until=8.65)
        self.assertFalse(visible(page_again, key=CUSTOM_PAGE_SIZE))
        self.assertFalse(visible(page_again, key=sort_menu))
        self.expect_rendered(page_again, text="50 rows", within=page_menu)
        self.expect_rendered(page_again, text="Primary key, descending", within=sort)
        self.assertEqual(config.read_bytes(), initial)

    def test_settings_row_count_mode_persists_and_controls_real_sqlite_pages(self):
        config = self.root / "settings.json"
        initial = {"rowCountMode": "future-mode", "rowCountEstimateThreshold": 7,
                   "defaultTableSort": "primaryKeyAsc", "unrelated": ["keep"],
                   "ui": {"language": "en", "tablePageSize": 2, "queryMaxRows": 23}}
        config.write_text(json.dumps(initial), encoding="utf-8")
        before_config = config.read_bytes()
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                INSERT INTO customers VALUES
                    (1,'row-a'), (2,'row-b'), (3,'row-c'), (4,'row-d'), (5,'row-e');
            """)
        self.store.write_text(json.dumps({"connections": [{
            "id": "e2e-sqlite", "name": "Rune SQLite row count", "databaseType": "sqlite",
            "database": str(self.db), "readOnly": True,
            "connectionTimeout": 30, "maxPoolSize": 2,
        }]}), encoding="utf-8")
        before_db = self.db.read_bytes()
        with sqlite3.connect(self.db.as_uri() + "?mode=ro", uri=True) as db:
            count = db.execute("SELECT count(*) FROM customers").fetchone()[0]
            ordered = db.execute("SELECT name FROM customers ORDER BY id").fetchall()
        self.assertEqual(count, 5)

        trigger = "settings-row-count-mode-trigger"
        menu = "settings-row-count-mode-content"
        menu_path = [SETTINGS, DATA_SETTINGS, tap(2.1, trigger)]
        opened = self.replay(menu_path, until=3.0)
        options = ("Automatic", "Always exact", "Estimate when possible", "Do not count")
        for label in options:
            self.expect_rendered(opened, text=label, within=menu)
        self.assertFalse(visible(opened, key="settings-default-table-sort-content"))
        self.assertFalse(visible(opened, key="settings-page-size-preset-content"))
        canceled = self.replay([*menu_path, key(3.3, "Escape")], until=4.4)
        self.expect_rendered(canceled, text="Automatic", within=trigger)
        self.assertFalse(visible(canceled, key=menu))
        self.assertEqual(config.read_bytes(), before_config)

        def choose(index, wire):
            picked = self.replay([*menu_path, tap(3.3, f"option{index}")], until=4.4)
            self.expect_rendered(picked, text=options[index], within=trigger)
            self.assertFalse(visible(picked, key=menu))
            self.assertFalse(visible(picked, key="settings-error"))
            self.assertEqual(json.loads(config.read_text(encoding="utf-8")),
                             {**initial, "rowCountMode": wire})

        def page(tree, number, rows, exact=False):
            label = f"Page {number} / 3" if exact else f"Page {number}"
            self.expect_rendered(tree, key="grid-page-label", text=label)
            self.expect_rendered(tree, key="grid-readonly", within="data-grid")
            footer = f"2 per page · {count} rows" if exact else "2 per page · Row count unavailable"
            self.expect_rendered(tree, text=footer, within="grid-footer")
            self.expect_rendered(tree, key="status-context")
            context = [node["text"] for node in tree["elements"]
                       if node.get("key") == "status-context" and not node.get("exiting")]
            self.assertEqual(len(context), 1)
            self.assertIn(f"{count} rows" if exact else "Row count unavailable", context[0])
            for i, (name,) in enumerate(rows):
                self.expect_rendered(tree, text=name, within=f"cell-{i}-1")
            self.assertFalse(visible(tree, key="table-loading", within="grid-footer"))

        choose(3, "none")
        reopened = self.replay([SETTINGS, DATA_SETTINGS], until=2.6)
        self.expect_rendered(reopened, text="Do not count", within=trigger)
        page(self.replay(OPEN, until=4.9), 1, ordered[:2])
        second = [*OPEN, tap(5.0, "page-next")]
        page(self.replay(second, until=6.7), 2, ordered[2:4])
        third = [*second, tap(7.0, "page-next"), tap(8.7, "page-next")]
        page(self.replay(third, until=9.8), 3, ordered[4:])
        page(self.replay([*third, tap(10.4, "page-prev")], until=11.9),
             2, ordered[2:4])

        choose(1, "exact")
        reopened = self.replay([SETTINGS, DATA_SETTINGS], until=2.6)
        self.expect_rendered(reopened, text="Always exact", within=trigger)
        page(self.replay(OPEN, until=4.9), 1, ordered[:2], exact=True)
        choose(2, "estimated")
        # SQLite genuinely cannot estimate: report the actual exact fallback.
        page(self.replay(OPEN, until=4.9), 1, ordered[:2], exact=True)
        self.assertEqual(self.db.read_bytes(), before_db)

    def _open_real_row_count_boundary_page(self, total, *, mode="exact"):
        config = self.root / "settings.json"
        initial = {
            "ui": {"language": "en", "tablePageSize": 25, "queryMaxRows": 23},
            "rowCountMode": mode, "defaultTableSort": "primaryKeyAsc",
            "rowCountEstimateThreshold": 200000, "unrelated": ["keep"],
        }
        config.write_text(json.dumps(initial), encoding="utf-8")
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT NOT NULL)")
            db.executemany("INSERT INTO customers VALUES (?,?)",
                           ((i, f"boundary-{i:03}") for i in range(1, total + 1)))
        self.store.write_text(json.dumps({"connections": [{
            "id": "e2e-sqlite", "name": "Rune SQLite page boundary", "databaseType": "sqlite",
            "database": str(self.db), "readOnly": True,
            "connectionTimeout": 30, "maxPoolSize": 2,
        }]}), encoding="utf-8")
        with sqlite3.connect(self.db.as_uri() + "?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT count(*) FROM customers").fetchone(), (total,))
            self.assertEqual(
                db.execute("SELECT name FROM customers ORDER BY id LIMIT 50 OFFSET 50").fetchall(),
                [] if total == 50 else [("boundary-051",)],
            )
        db_before = self.db.read_bytes()
        picked = self.replay([SETTINGS, DATA_SETTINGS, tap(2.1, PAGE_SIZE),
                              tap(3.2, "option1")], until=4.3)
        self.expect_rendered(picked, text="50 rows", within=PAGE_SIZE)
        self.expect_rendered(picked, text=("Always exact" if mode == "exact" else "Do not count"),
                             within="settings-row-count-mode-trigger")
        self.assertFalse(visible(picked, key="settings-error"))
        self.assertEqual(json.loads(config.read_text(encoding="utf-8")),
                         {**initial, "ui": {**initial["ui"], "tablePageSize": 50}})
        return config.read_bytes(), db_before

    def _row_count_next_at(self, tree, t):
        (x, y, width, height) = self._settled_rect(tree, "page-next")
        return {"t": t, "type": "tap", "x": x + width / 2, "y": y + height / 2}

    def _expect_row_count_next_action(self, tree, enabled):
        def visit(node):
            if node.get("key") == "page-next":
                yield node
            for child in node.get("children", []):
                yield from visit(child)
        matching = list(visit(tree["accessibility"]))
        self.assertEqual(len(matching), 1)
        target = matching[0]
        self.assertEqual(target.get("role"), "button")
        self.assertEqual("disabled" in target.get("states", []), not enabled)
        self.assertEqual("click" in target.get("actions", []), enabled)

    def test_settings_exact_50_rows_disables_next_on_full_final_page(self):
        settings, database = self._open_real_row_count_boundary_page(50)
        first = self.replay(OPEN, until=4.9)
        self.expect_rendered(first, key="grid-page-label", text="Page 1 / 1")
        self.expect_rendered(first, text="50 per page · 50 rows", within="grid-footer")
        self.expect_rendered(first, text="boundary-001", within="cell-0-1")
        self.expect_rendered(first, key="grid-readonly", within="data-grid")
        attempted = self.replay([*OPEN, self._row_count_next_at(first, 5.2)], until=6.9)
        self.expect_rendered(attempted, key="grid-page-label", text="Page 1 / 1")
        self.expect_rendered(attempted, text="boundary-001", within="cell-0-1")
        self.assertFalse(visible(attempted, key="table-loading"))
        self.assertEqual((self.root / "settings.json").read_bytes(), settings)
        self.assertEqual(self.db.read_bytes(), database)
        # Checked after the physical attempt, so a red result on old source
        # distinguishes a misleading click affordance from wrong rows/paging.
        self._expect_row_count_next_action(first, False)
        self._expect_row_count_next_action(attempted, False)

    def test_settings_exact_51_rows_enables_next_and_renders_row_51(self):
        settings, database = self._open_real_row_count_boundary_page(51)
        first = self.replay(OPEN, until=4.9)
        self.expect_rendered(first, key="grid-page-label", text="Page 1 / 2")
        self.expect_rendered(first, text="50 per page · 51 rows", within="grid-footer")
        self.expect_rendered(first, text="boundary-001", within="cell-0-1")
        self._expect_row_count_next_action(first, True)
        second = self.replay([*OPEN, self._row_count_next_at(first, 5.2)], until=6.9)
        self.expect_rendered(second, key="grid-page-label", text="Page 2 / 2")
        self.expect_rendered(second, text="boundary-051", within="cell-0-1")
        self.expect_rendered(second, key="grid-readonly", within="data-grid")
        self.assertFalse(visible(second, text="boundary-001", within="data-grid"))
        self._expect_row_count_next_action(second, False)
        self.assertEqual((self.root / "settings.json").read_bytes(), settings)
        self.assertEqual(self.db.read_bytes(), database)

    def test_settings_no_count_full_page_probe_preserves_data_and_resets_on_filter(self):
        settings, database = self._open_real_row_count_boundary_page(50, mode="none")
        first = self.replay(OPEN, until=4.9)
        self.expect_rendered(first, key="grid-page-label", text="Page 1")
        self.expect_rendered(first, text="50 per page · Row count unavailable", within="grid-footer")
        self.expect_rendered(first, text="boundary-001", within="cell-0-1")
        self._expect_row_count_next_action(first, True)
        probe = [*OPEN, self._row_count_next_at(first, 5.2)]
        retained = self.replay(probe, until=6.9)
        self.expect_rendered(retained, key="grid-page-label", text="Page 1")
        self.expect_rendered(retained, text="boundary-001", within="cell-0-1")
        self.expect_rendered(retained, text="50 per page · Row count unavailable", within="grid-footer")
        self.assertFalse(visible(retained, text="50 rows", within="grid-footer"))
        self._expect_row_count_next_action(retained, False)

        # A genuine empty result from a normal filter must still replace the
        # old rows. Clearing that filter resets the exhausted probe state.
        filtered = [*probe, tap(7.35, "filter-column"),
                    *fill(8.0, "filter-input", "missing-boundary"), key(9.0, "Enter")]
        empty = self.replay(filtered, until=10.4)
        self.expect_rendered(empty, key="grid-page-label", text="Page 1")
        self.expect_rendered(empty, key="grid-filter-active")
        self.assertFalse(visible(empty, text="boundary-001", within="data-grid"))
        self._expect_row_count_next_action(empty, False)

        clear = [*filtered, tap(10.8, "filter-input"), key(11.05, "a", cmd=True),
                 key(11.3, "Backspace"), key(11.55, "Enter")]
        again = self.replay(clear, until=13.1)
        self.expect_rendered(again, text="boundary-001", within="cell-0-1")
        self.expect_rendered(again, text="50 per page · Row count unavailable", within="grid-footer")
        self._expect_row_count_next_action(again, True)
        self.assertEqual((self.root / "settings.json").read_bytes(), settings)
        self.assertEqual(self.db.read_bytes(), database)

    def test_settings_default_table_sort_persists_and_controls_native_sqlite_reads(self):
        settings = self.root / "settings.json"
        initial = {"ui": {"language": "en", "tablePageSize": 2, "queryMaxRows": 23},
                   "future-prefs": ["keep"], "defaultTableSort": "future-mode"}
        settings.write_text(json.dumps(initial), encoding="utf-8")
        original = settings.read_bytes()
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                CREATE TABLE orders(name TEXT NOT NULL, payload TEXT NOT NULL);
                INSERT INTO customers VALUES (1,'zulu'),(2,'alpha'),(3,'mike'),(4,'bravo');
                INSERT INTO orders VALUES ('zulu','first'),('alpha','second'),
                    ('mike','third'),('bravo','fourth');
            """)
        self.store.write_text(json.dumps({"connections": [{
            "id": "e2e-sqlite", "name": "Rune SQLite default sort", "databaseType": "sqlite",
            "database": str(self.db), "readOnly": True, "connectionTimeout": 30, "maxPoolSize": 2,
        }]}), encoding="utf-8")
        with sqlite3.connect(self.db.as_uri() + "?mode=ro", uri=True) as db:
            desc = db.execute("SELECT name FROM customers ORDER BY id DESC LIMIT 2").fetchall()
            desc_next = db.execute("SELECT name FROM customers ORDER BY id DESC LIMIT 2 OFFSET 2").fetchall()
            asc = db.execute("SELECT name FROM customers ORDER BY id ASC LIMIT 2").fetchall()
            manual = db.execute("SELECT name FROM customers ORDER BY name ASC LIMIT 2").fetchall()
            driver_orders = db.execute("SELECT name FROM orders ORDER BY name ASC LIMIT 2").fetchall()
            none = db.execute("SELECT name FROM orders LIMIT 2").fetchall()
        self.assertNotEqual(desc, manual)
        self.assertNotEqual(driver_orders, none, "the SQLite fixture distinguishes local query plans")
        before = self.db.read_bytes()

        trigger = "settings-default-table-sort-trigger"
        menu = "settings-default-table-sort-content"
        menu_path = [SETTINGS, DATA_SETTINGS, tap(2.1, trigger)]
        opened = self.replay(menu_path, until=3.0)
        for label in ("Primary key, ascending", "Primary key, descending", "Do not sort"):
            self.expect_rendered(opened, text=label, within=menu)
        self.assertEqual(settings.read_bytes(), original)
        canceled = self.replay([*menu_path, key(3.3, "Escape")], until=4.3)
        self.assertFalse(visible(canceled, key=menu))
        self.expect_rendered(canceled, text="Primary key, ascending", within=trigger)
        self.assertEqual(settings.read_bytes(), original)

        def choose(index, label, wire):
            picked = self.replay([*menu_path, tap(3.3, f"option{index}")], until=4.4)
            self.assertFalse(visible(picked, key="settings-error"))
            self.assertFalse(visible(picked, key=menu))
            self.expect_rendered(picked, text=label, within=trigger)
            self.assertEqual(json.loads(settings.read_text(encoding="utf-8")),
                             {**initial, "defaultTableSort": wire})

        def expect_page(tree, rows, header="name", col=1):
            self.expect_rendered(tree, key="grid-readonly", within="data-grid")
            self.expect_rendered(tree, text=header, within=f"head-{col}")
            for i, (name,) in enumerate(rows):
                self.expect_rendered(tree, text=name, within=f"cell-{i}-{col}")

        choose(1, "Primary key, descending", "primaryKeyDesc")
        # All reads take place in a fresh real app, not a mutation of the
        # test state; the normal page control exercises the same worker.
        expect_page(self.replay(OPEN, until=4.9), desc)
        paged = self.replay([*OPEN, tap(5.0, "page-next")], until=6.8)
        self.expect_rendered(paged, key="grid-page-label", text="Page 2 / 2")
        expect_page(paged, desc_next)

        sort_a = [*OPEN, tap(5.2, "head-1")]
        expect_page(self.replay(sort_a, until=6.8), manual, "name ↑")
        b = [*sort_a, tap(7.0, "tree-table-orders")]
        expect_page(self.replay(b, until=8.8), driver_orders, col=0)
        a_again = self.replay([*b, tap(9.1, "tab-1")], until=10.9)
        expect_page(a_again, manual, "name ↑")
        # Clear through the two remaining real header cycle presses.
        expect_page(self.replay([*sort_a, tap(7.0, "head-1"), tap(8.5, "head-1")],
                                until=10.2), desc)

        choose(2, "Do not sort", "none")
        expect_page(self.replay([OPEN[0], tap(3, "tree-table-orders")], until=4.9),
                    none, col=0)
        choose(0, "Primary key, ascending", "primaryKeyAsc")
        expect_page(self.replay(OPEN, until=4.9), asc)
        self.assertEqual(self.db.read_bytes(), before)

    def test_sqlite_manual_table_sort_follows_only_its_own_tab(self):
        """An identical sortable column on two real SQLite tables must not leak order."""
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE customers(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                CREATE TABLE orders(id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                INSERT INTO customers VALUES (1,'zora'),(2,'amber'),(3,'mica');
                INSERT INTO orders VALUES (1,'tango'),(2,'bravo'),(3,'delta');
            """)
        self.store.write_text(json.dumps({"connections":[{
            "id":"e2e-sqlite", "name":"Rune SQLite sort ownership", "databaseType":"sqlite",
            "database":str(self.db), "readOnly":True, "connectionTimeout":30, "maxPoolSize":2,
        }]}), encoding="utf-8")
        before = self.db.read_bytes()
        with sqlite3.connect(self.db.as_uri()+"?mode=ro", uri=True) as db:
            customers_default = db.execute("SELECT name FROM customers").fetchall()
            customers_asc = db.execute("SELECT name FROM customers ORDER BY name ASC").fetchall()
            orders_default = db.execute("SELECT name FROM orders").fetchall()
            orders_desc = db.execute("SELECT name FROM orders ORDER BY name DESC").fetchall()
        self.assertNotEqual(customers_default, customers_asc)
        self.assertNotEqual(orders_default, orders_desc)

        def expect_order(tree, rows, header):
            self.expect_rendered(tree, key="grid-readonly", within="data-grid")
            self.expect_rendered(tree, text=header, within="head-1")
            for i, (name,) in enumerate(rows):
                self.expect_rendered(tree, text=name, within=f"cell-{i}-1")

        a = [*OPEN, tap(5.2, "head-1")]
        expect_order(self.replay(a, until=6.8), customers_asc, "name ↑")
        b = [*a, tap(7.0, "tree-table-orders")]
        unsorted_b = self.replay(b, until=8.8)
        self.expect_rendered(unsorted_b, text="customers", within="tab-1")
        self.expect_rendered(unsorted_b, text="orders", within="tab-2")
        expect_order(unsorted_b, orders_default, "name")
        ordered_b = [*b, tap(9.1, "head-1"), tap(10.6, "head-1")]
        expect_order(self.replay(ordered_b, until=12), orders_desc, "name ↓")

        back_a = [*ordered_b, tap(12.3, "tab-1")]
        expect_order(self.replay(back_a, until=13.9), customers_asc, "name ↑")
        back_b = [*back_a, tap(14.1, "tab-2")]
        expect_order(self.replay(back_b, until=15.6), orders_desc, "name ↓")

        reopened_a = self.replay([
            *back_b, tap(16, "tab-close-1"), tap(17, "tree-table-customers"),
        ], until=18.7, timeout=75)
        self.assertFalse(visible(reopened_a, key="tab-1"))
        self.expect_rendered(reopened_a, text="customers", within="tab-3")
        expect_order(reopened_a, customers_default, "name")
        self.assertEqual(self.db.read_bytes(), before)

    def test_sqlite_query_retains_rows_across_result_batches(self):
        fixture(self.store, self.db)
        sql = ("WITH RECURSIVE n(i) AS(SELECT 1 UNION ALL SELECT i+1 FROM n WHERE i<600) "
               "SELECT i, 'batch-'||i AS marker FROM n;")
        with sqlite3.connect(self.db) as connection:
            expected = connection.execute(sql).fetchall()
        self.assertEqual((len(expected), expected[-1]), (600, (600, "batch-600")))
        tree = self.replay([
            *query_steps(sql), *fill(11.0, "result-find-input", expected[-1][1]),
            tap(12.5, "result-find-apply"),
        ], until=14.0)
        self.expect_rendered(tree, key="result-grid-status", text="result · 600 rows × 2 cols")
        self.expect_rendered(tree, key="result-find-count", text="1 of 1")
        self.expect_rendered(tree, key="result-status-text")
        # The status prefix is localized; check the exact global cell address.
        self.assertTrue(any(
            node.get("key") == "result-status-text"
            and (node.get("text") or "").endswith(" · 600 rows × 2 cols · selection r599c1..r599c1")
            for node in tree["elements"]
        ))

    def test_sqlite_dirty_count_reverts_one_cell_and_commits_only_remaining_edits(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.execute("ALTER TABLE customers ADD COLUMN score INTEGER NOT NULL DEFAULT 0")
            connection.execute("UPDATE customers SET score = id * 10")

        def rows():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as connection:
                return connection.execute(
                    "SELECT id, customer_name, score FROM customers ORDER BY id").fetchall()

        def edit(t, cell, value):
            return [tap(t, cell), key(t + .35, "Enter"), tap(t + .6, "grid-editor"),
                    key(t + .95, "a", cmd=True), type_text(t + 1.3, value),
                    key(t + 2.15, "Enter")]

        before = [(1, "fixture-ada", 10), (2, "fixture-lin", 20)]
        self.assertEqual(rows(), before)
        staged_steps = [
            *OPEN, *edit(4.8, "cell-0-1", "Ada, staged"),
            *edit(7.4, "cell-0-2", "17"), *edit(10.0, "cell-1-1", "temporary"),
        ]
        staged = self.replay(staged_steps, until=12.8)
        self.expect_rendered(staged, text="3 pending changes — use Commit/Rollback",
                             within="pending-changes-bar")
        self.expect_rendered(staged, text="temporary", within="cell-1-1")
        self.assertEqual(rows(), before)

        reverted_steps = [*staged_steps, *edit(13.1, "cell-1-1", "fixture-lin")]
        reverted = self.replay(reverted_steps, until=15.8)
        self.expect_rendered(reverted, text="2 pending changes — use Commit/Rollback",
                             within="pending-changes-bar")
        self.expect_rendered(reverted, text="fixture-lin", within="cell-1-1")
        self.assertEqual(rows(), before)

        committed = self.replay([*reverted_steps, tap(16.2, "commit-staged")], until=19.0)
        self.assertFalse(visible(committed, text="2 pending changes — use Commit/Rollback"))
        self.expect_rendered(committed, text="Ada, staged", within="cell-0-1")
        self.expect_rendered(committed, text="17", within="cell-0-2")
        self.assertEqual(rows(), [(1, "Ada, staged", 17), before[1]])

    def test_sqlite_grid_rejects_atomic_batch_then_commits_corrected_values(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.executescript("""
                CREATE TABLE edit_audit (id INTEGER, name TEXT);
                CREATE TRIGGER reject_second_edit BEFORE UPDATE ON customers
                WHEN OLD.id=2 AND NEW.customer_name='reject-this'
                BEGIN SELECT RAISE(ABORT, 'fixture rejected row two'); END;
                CREATE TRIGGER log_edits AFTER UPDATE ON customers
                BEGIN INSERT INTO edit_audit VALUES (NEW.id, NEW.customer_name); END;
            """)

        def stored():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as connection:
                return (
                    connection.execute("SELECT * FROM customers ORDER BY id").fetchall(),
                    connection.execute("SELECT * FROM edit_audit ORDER BY rowid").fetchall(),
                )

        def edit(t, cell, value):
            return [tap(t, cell), key(t + .35, "Enter"), tap(t + .6, "grid-editor"),
                    key(t + .95, "a", cmd=True), type_text(t + 1.3, value),
                    key(t + 2.15, "Enter")]

        before = ([(1, "fixture-ada"), (2, "fixture-lin")], [])
        invalid = [*OPEN, *edit(4.8, "cell-0-1", "first-valid"),
                   *edit(7.4, "cell-1-1", "reject-this"), tap(10.2, "commit-staged")]
        rejected = self.replay(invalid, until=12.6)
        self.assertFalse(visible(rejected, key="commit-loading-banner"))
        self.expect_rendered(rejected, text="2 pending changes — use Commit/Rollback",
                             within="pending-changes-bar")
        self.expect_rendered(rejected, text="first-valid", within="cell-0-1")
        self.expect_rendered(rejected, text="reject-this", within="cell-1-1")
        self.assertEqual(stored(), before, "failed row must roll back the valid edit and its audit entry")

        discarded = self.replay([*invalid, tap(13.0, "rollback-staged")], until=14.8)
        self.assertFalse(visible(discarded, key="pending-changes-bar"))
        self.expect_rendered(discarded, text="fixture-ada", within="cell-0-1")
        self.expect_rendered(discarded, text="fixture-lin", within="cell-1-1")
        self.assertEqual(stored(), before)

        committed = self.replay([*invalid, *edit(13.2, "cell-1-1", "second-valid"),
                                 tap(16.0, "commit-staged")], until=18.6)
        self.assertFalse(visible(committed, key="commit-loading-banner"))
        self.assertFalse(visible(committed, key="pending-changes-bar"))
        self.expect_rendered(committed, text="first-valid", within="cell-0-1")
        self.expect_rendered(committed, text="second-valid", within="cell-1-1")
        expected = [(1, "first-valid"), (2, "second-valid")]
        self.assertEqual(stored(), (expected, expected))

    def test_sqlite_grid_without_primary_key_refuses_to_persist_staged_edit(self):
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as connection:
            connection.executescript("""
                CREATE TABLE keyless (label TEXT);
                INSERT INTO keyless VALUES ('first'), ('first');
            """)
        actions = [OPEN[0], tap(3, "tree-table-keyless"), tap(5, "cell-0-0"),
                   key(5.4, "Enter"), *fill(5.75, "grid-editor", "not-persisted"),
                   key(6.9, "Enter"), tap(7.6, "commit-staged")]
        tree = self.replay(actions, until=9.5)
        self.expect_rendered(tree, text="not-persisted", within="cell-0-0")
        self.expect_rendered(tree, text="first", within="cell-1-0")
        self.expect_rendered(tree, text="1 pending change — use Commit/Rollback",
                             within="pending-changes-bar")
        self.assertFalse(visible(tree, key="commit-loading-banner"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as connection:
            self.assertEqual(connection.execute("SELECT label FROM keyless ORDER BY rowid").fetchall(),
                             [("first",), ("first",)])

    def test_legacy_connection_group_expansion_and_search_clear_restore_connections(self):
        """Migrate the expand/collapse and clear cases in connection-search-group.ts."""
        fixture(self.store, self.db)
        saved = json.loads(self.store.read_text(encoding="utf-8"))
        saved["connections"].append({
            **saved["connections"][0], "id": "other-local", "name": "Other local SQLite",
        })
        self.store.write_text(json.dumps(saved), encoding="utf-8")
        group = "nav-group-all"
        search = "navigator-filter-input"
        first, second = "nav-conn-e2e-sqlite", "nav-conn-other-local"
        collapsed = self.replay([tap(.6, group)], until=1.9)
        self.expect_rendered(collapsed, key=group)
        self.assertFalse(visible(collapsed, key=first))
        self.assertFalse(visible(collapsed, key=second))

        filtered = [tap(.6, group), *fill(1.8, search, "Other local")]
        visible_match = self.replay(filtered, until=3.2)
        self.expect_rendered(visible_match, key=second)
        self.assertFalse(visible(visible_match, key=first))

        cleared = [*filtered, tap(3.4, search), key(3.6, "a", cmd=True), key(3.9, "Backspace")]
        closed_again = self.replay(cleared, until=4.9)
        self.assertFalse(visible(closed_again, key=first))
        self.assertFalse(visible(closed_again, key=second))

        reopened = self.replay([*cleared, tap(5.0, group)], until=6.1)
        self.expect_rendered(reopened, key=first)
        self.expect_rendered(reopened, key=second)
        self.assertEqual(json.loads(self.store.read_text(encoding="utf-8")), saved)

    def test_legacy_settings_navigation_switches_all_six_sections(self):
        """Migrate settings.ts SS-NAV-001 (navigation, not feature availability)."""
        sections = ("behavior", "logging", "ai", "mcpServer", "mcpClient", "extensions")
        for section in sections:
            with self.subTest(section=section):
                tree = self.replay([SETTINGS, tap(1.4, f"settings-nav-{section}")], until=2.3)
                self.expect_rendered(tree, key=f"settings-section-{section}")
                self.assertFalse(visible(tree, key="settings-section-general"))
                for other in sections:
                    if other != section:
                        self.assertFalse(visible(tree, key=f"settings-section-{other}"))
        self.assertFalse((self.root / "settings.json").exists(), "navigation must not save settings")

    def test_legacy_sqlite_published_view_opens_and_shows_only_its_result(self):
        """Migrate the display-view-data journey from sqlite.ts."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE posts(id INTEGER PRIMARY KEY, author TEXT, published INTEGER);
                INSERT INTO posts VALUES (1, 'published-author', 1), (2, 'draft-author', 0);
                CREATE VIEW published_posts AS
                    SELECT author FROM posts WHERE published = 1;
            """)
            expected = db.execute("SELECT author FROM published_posts").fetchall()
        self.assertEqual(expected, [("published-author",)])
        with sqlite3.connect(self.db) as db:
            original = tuple(db.iterdump())
        opened = self.replay([OPEN[0], tap(3, "tree-table-published_posts")], until=5.8)
        self.expect_rendered(opened, key="tree-table-published_posts", within="datazen-navigator")
        self.expect_rendered(opened, text=expected[0][0], within="cell-0-0")
        self.assertFalse(visible(opened, text="draft-author", within="table-workspace"))
        # The live SQLite driver may initialize WAL on read. Compare all
        # logical contents and schema rather than the physical journal bytes.
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(tuple(db.iterdump()), original)

    def test_legacy_sqlite_sidebar_lists_views_under_separate_category(self):
        """Migrate the show-views-in-sidebar case from sqlite.ts."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE posts(id INTEGER PRIMARY KEY, author TEXT, published INTEGER);
                CREATE VIEW published_posts AS SELECT author FROM posts WHERE published = 1;
                CREATE VIEW draft_posts AS SELECT author FROM posts WHERE published = 0;
            """)
            expected = [name for (name,) in db.execute(
                "SELECT name FROM sqlite_schema WHERE type = 'view' ORDER BY name"
            )]
        category = "tree-views-main-main"
        collapsed = [OPEN[0], tap(3, category)]
        initial = self.replay(collapsed, until=4.2)
        self.expect_rendered(initial, text="视图 (2)", within=category)
        self.expect_rendered(initial, key="tree-table-posts")
        for name in expected:
            self.assertFalse(visible(initial, key=f"tree-table-{name}"))

        reopened = self.replay([*collapsed, tap(4.5, category)], until=5.8)
        for name in expected:
            self.expect_rendered(reopened, key=f"tree-table-{name}", within="datazen-navigator")
        self.expect_rendered(reopened, key="tree-table-posts")

        filtered = [*collapsed, *fill(4.5, "navigator-filter-input", "published")]
        narrowed = self.replay(filtered, until=6.1)
        self.expect_rendered(narrowed, text="视图 (1)", within=category)
        self.expect_rendered(narrowed, key="tree-table-published_posts")
        self.assertFalse(visible(narrowed, key="tree-table-draft_posts"))
        self.assertFalse(visible(narrowed, key="tree-table-posts"))

    def test_legacy_workspace_switches_and_closes_tabs_then_returns_to_empty(self):
        """Migrate unified-tab-bar.ts UTB-001 through UTB-006."""
        fixture(self.store, self.db)
        sql = "SELECT customer_name FROM customers WHERE id = 2;"
        with sqlite3.connect(self.db) as db:
            expected = db.execute(sql).fetchone()[0]
            original = db.execute("SELECT * FROM customers ORDER BY id").fetchall()
        query = [*OPEN, tap(4.4, "conn-toolbar-new-query"), tap(5.5, "sql-editor-scroll"),
                 key(5.8, "a", cmd=True), type_text(6.0, sql), tap(8.9, "sql-run")]
        before = self.replay(query, until=10.5)
        self.expect_rendered(before, key="conn-toolbar-new-query")
        self.expect_rendered(before, text="customers", within="tab-1")
        self.expect_rendered(before, text="Query", within="tab-2")
        self.expect_rendered(before, text=expected, within="result-workspace")

        table = [*query, tap(10.7, "tab-1")]
        active_table = self.replay(table, until=12.0)
        self.expect_rendered(active_table, text="fixture-ada", within="table-workspace")
        self.assertFalse(visible(active_table, key="result-workspace"))
        self.expect_rendered(active_table, key="tab-2")

        switch_back = [*table, tap(12.2, "tab-2")]
        active_query = self.replay(switch_back, until=13.3)
        self.expect_rendered(active_query, text=expected, within="result-workspace")
        self.assertFalse(visible(active_query, key="table-workspace"))

        close_query = [*switch_back, tap(13.5, "tab-close-2")]
        closed = self.replay(close_query, until=14.9)
        self.assertFalse(visible(closed, key="tab-2"))
        self.expect_rendered(closed, key="tab-1")
        self.expect_rendered(closed, text="fixture-ada", within="table-workspace")

        empty = self.replay([*close_query, tap(15.1, "tab-close-1")], until=16.3)
        self.expect_rendered(empty, key="workspace-empty")
        self.expect_rendered(empty, key="conn-toolbar-new-query")
        self.assertFalse(visible(empty, key="tab-1"))
        self.assertFalse(visible(empty, key="tab-2"))
        self.assertFalse(visible(empty, key="table-workspace"))
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), original)

    def test_legacy_sqlite_empty_table_remains_a_table_as_rows_come_and_go(self):
        """Migrate table-data.ts TC-TABLE-009 with an exact empty-grid oracle."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.execute("CREATE TABLE empty_probe (id INTEGER PRIMARY KEY, marker TEXT NOT NULL)")
            original_customers = db.execute("SELECT * FROM customers ORDER BY id").fetchall()

        table = "tree-table-empty_probe"
        open_table = [tap(.5, "conn-open-e2e-sqlite"), tap(3.8, table)]

        def readback():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
                self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(),
                                 original_customers)
                self.assertEqual([row[1] for row in db.execute("PRAGMA table_info(empty_probe)")],
                                 ["id", "marker"])
                return db.execute("SELECT id, marker FROM empty_probe ORDER BY id").fetchall()

        def check_grid(tree, expected, tab):
            self.expect_rendered(tree, key="data-grid", within="table-workspace")
            self.expect_rendered(tree, text="empty_probe", within=tab)
            self.expect_rendered(tree, text="id", within="head-0")
            self.expect_rendered(tree, text="marker", within="head-1")
            self.expect_rendered(tree, text=f"{len(expected)} rows", within="grid-filter")
            for key in ("workspace-empty", "table-read-error", "table-loading-banner"):
                self.assertFalse(visible(tree, key=key), key)
            if expected:
                self.expect_rendered(tree, text=str(expected[0][0]), within="cell-0-0")
                self.expect_rendered(tree, text=expected[0][1], within="cell-0-1")
            else:
                self.assertFalse(visible(tree, key="cell-0-0"))
                self.assertFalse(visible(tree, key="cell-0-1"))
            self.assertFalse(visible(tree, key="cell-1-0"))

        initial = self.replay(open_table, until=5.7)
        self.assertEqual(readback(), [])
        check_grid(initial, [], "tab-1")

        def execute_then_open(sql):
            # Open the table from the real navigator after the execution.
            # A retained query result cannot satisfy the table-grid assertions.
            return self.replay([*query_steps(sql), tap(10.6, table)], until=12.6)

        inserted = execute_then_open("INSERT INTO empty_probe VALUES (47, 'only real row');")
        expected = [(47, "only real row")]
        self.assertEqual(readback(), expected)
        check_grid(inserted, expected, "tab-2")

        emptied = execute_then_open("DELETE FROM empty_probe WHERE id = 47;")
        self.assertEqual(readback(), [])
        check_grid(emptied, [], "tab-2")

    def test_legacy_table_double_click_edits_current_value_and_escape_discards_draft(self):
        """Migrate table-edit.ts DE-002 and DE-005 on the real SQLite grid."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            initial = db.execute("SELECT * FROM customers ORDER BY id").fetchall()
        open_editor = [*OPEN, {**tap(5, "cell-0-1"), "type": "double-click"}]
        shown = self.replay(open_editor, until=6.3)
        self.expect_rendered(shown, key="grid-editor", within="cell-0-1")

        def find_editor(node, within_editor=False):
            within_editor = within_editor or node.get("key") == "grid-editor"
            if within_editor and node.get("role") == "text-input":
                return node
            for child in node.get("children", ()):
                editor = find_editor(child, within_editor)
                if editor is not None:
                    return editor
            return None

        editor = find_editor(shown["accessibility"])
        self.assertIsNotNone(editor)
        self.assertEqual(editor.get("value"), "fixture-ada")
        draft_actions = [*open_editor, key(6.4, "a", cmd=True),
                         type_text(6.7, "discard-edited-draft")]
        staged_input = find_editor(self.replay(draft_actions, until=7.9)["accessibility"])
        self.assertIsNotNone(staged_input)
        self.assertEqual(staged_input.get("value"), "discard-edited-draft")
        cancelled = self.replay([
            *draft_actions, key(8.1, "Escape"), tap(9.1, "commit-staged"),
        ], until=10.6)
        self.assertFalse(visible(cancelled, key="grid-editor"))
        self.expect_rendered(cancelled, text="fixture-ada", within="cell-0-1")
        self.assertFalse(visible(cancelled, text="discard-edited-draft"))
        status = next(node["text"] for node in cancelled["elements"]
                      if node.get("key") == "data-grid-status")
        self.assertTrue(status.endswith(" · 0 dirty"), status)
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(db.execute("SELECT * FROM customers ORDER BY id").fetchall(), initial)

    def test_legacy_table_numeric_sql_update_survives_reload(self):
        """Migrate the typed update and readback in table-edit.ts DE-003b."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                ALTER TABLE customers ADD COLUMN score INTEGER DEFAULT 0;
                UPDATE customers SET score = id * 10;
            """)
        changed = "UPDATE customers SET score = 999 WHERE id = 2;"
        read = "SELECT score, typeof(score) FROM customers WHERE id = 2;"
        result = self.replay([
            *query_steps(changed), *fill(10.4, "sql-editor-scroll", read),
            tap(13.0, "sql-run"),
        ], until=15.4)
        self.expect_rendered(result, text="999", within="result-workspace")
        self.expect_rendered(result, text="integer", within="result-workspace")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(
                db.execute("SELECT id, score, typeof(score) FROM customers ORDER BY id").fetchall(),
                [(1, 10, "integer"), (2, 999, "integer")],
            )
        reopened = self.replay(OPEN, until=5.5)
        self.expect_rendered(reopened, text="10", within="cell-0-2")
        self.expect_rendered(reopened, text="999", within="cell-1-2")

    def test_legacy_er_opens_real_sqlite_tables_with_nonoverlapping_nodes(self):
        """Migrate ER-002, ER-003 and the node-layout part of ER-010."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE locations (location_id INTEGER PRIMARY KEY, label TEXT);
                CREATE TABLE visits (visit_id INTEGER PRIMARY KEY, locale TEXT,
                    location_id INTEGER REFERENCES locations(location_id));
            """)
            column_names = {
                table: [entry[1] for entry in db.execute(f"PRAGMA table_info({table})")]
                for table in ("customers", "locations", "visits")
            }
        connect = [OPEN[0]]
        connected = self.replay(connect, until=2.5)
        self.expect_rendered(connected, key="er-button")
        self.assertFalse(visible(connected, key="er-canvas"))

        show = [*connect, tap(3.0, "er-button")]
        diagram = self.replay(show, until=6.5)
        self.expect_rendered(diagram, key="er-page")
        self.expect_rendered(diagram, key="er-canvas")
        boxes = []
        for table, columns in column_names.items():
            node = f"er-box-{table}"
            self.expect_rendered(diagram, key=node, within="er-canvas")
            self.expect_rendered(diagram, key=f"er-title-{table}", text=table, within=node)
            for index, column in enumerate(columns):
                self.expect_rendered(diagram, key=f"er-col-{table}-{index}", text=column, within=node)
            self.assertFalse(visible(diagram, key=f"er-pending-{table}"))
            boxes.append(next(e["rect"] for e in diagram["elements"] if e.get("key") == node))

        from itertools import combinations
        for (ax, ay, aw, ah), (bx, by, bw, bh) in combinations(boxes, 2):
            self.assertTrue(ax + aw <= bx or bx + bw <= ax or ay + ah <= by or by + bh <= ay,
                            "SQLite ER table nodes overlap")

    def test_legacy_er_statistics_count_one_declared_relationship_for_composite_fk(self):
        """Migrate er-diagram.ts ER-006 with real SQLite catalog metadata."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE warehouse (region TEXT, code INTEGER, PRIMARY KEY(region, code));
                CREATE TABLE inventory (
                    row_id INTEGER PRIMARY KEY, region TEXT, code INTEGER,
                    FOREIGN KEY(region, code) REFERENCES warehouse(region, code)
                );
                INSERT INTO warehouse VALUES ('north', 1);
                INSERT INTO inventory VALUES (10, 'north', 1);
            """)
            tables = [name for (name,) in db.execute(
                "SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name"
            )]
            parts = db.execute("PRAGMA foreign_key_list(inventory)").fetchall()
            links = len({part[0] for part in parts})
            original = tuple(db.iterdump())
        self.assertEqual((len(tables), len(parts), links), (3, 2, 1))
        self.assertEqual([(part[2], part[3], part[4]) for part in parts],
                         [("warehouse", "region", "region"), ("warehouse", "code", "code")])

        opened = [OPEN[0], tap(3, "er-button")]
        stats = self.replay(opened, until=6)
        self.expect_rendered(stats, key="er-table-count", text=f"{len(tables)} 张表",
                             within="er-diagram-stats")
        self.expect_rendered(stats, key="er-relationship-count", text=f"{links} 个已声明外键",
                             within="er-diagram-stats")

        def states(node, expected_key):
            if node.get("key") == expected_key:
                return node.get("states", [])
            for child in node.get("children", ()):
                hit = states(child, expected_key)
                if hit is not None:
                    return hit
            return None

        self.assertIn("unchecked", states(stats["accessibility"], "er-box-inventory"))
        selected = self.replay([*opened, tap(6.2, "er-title-inventory")], until=7.7)
        self.expect_rendered(selected, key="er-box-inventory", within="er-canvas")
        self.assertIn("checked", states(selected["accessibility"], "er-box-inventory"))
        self.assertIn("unchecked", states(selected["accessibility"], "er-box-warehouse"))
        self.expect_rendered(selected, key="er-relationship-count", text="1 个已声明外键")
        with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
            self.assertEqual(tuple(db.iterdump()), original)

    def test_legacy_er_search_filters_visible_catalog_nodes_and_can_be_cleared(self):
        """Migrate er-diagram.ts ER-007 with non-vacuous matching assertions."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE pg_inventory (id INTEGER PRIMARY KEY);
                CREATE TABLE pg_receipts (id INTEGER PRIMARY KEY);
            """)
            matches = [name for (name,) in db.execute("""
                SELECT name FROM sqlite_schema
                WHERE type = 'table' AND substr(lower(name), 1, 3) = 'pg_' ORDER BY name
            """)]
        self.assertEqual(matches, ["pg_inventory", "pg_receipts"])
        search = "er-diagram-search"
        opened = [OPEN[0], tap(3, "er-button")]
        filtered = [*opened, *fill(5, search, "PG_")]
        view = self.replay(filtered, until=6.7)
        for name in matches:
            self.expect_rendered(view, key=f"er-box-{name}", within="er-canvas")
        self.assertFalse(visible(view, key="er-box-customers"))
        self.assertFalse(visible(view, key="er-search-no-matches"))

        no_match = [*filtered, *fill(7, search, "missing-table")]
        empty = self.replay(no_match, until=9.0)
        self.expect_rendered(empty, key="er-search-no-matches", within="er-canvas")
        for name in [*matches, "customers"]:
            self.assertFalse(visible(empty, key=f"er-box-{name}"))

        full = self.replay([*no_match, tap(9.2, search), key(9.4, "a", cmd=True),
                            key(9.7, "Backspace")], until=11)
        for name in [*matches, "customers"]:
            self.expect_rendered(full, key=f"er-box-{name}", within="er-canvas")
        self.assertFalse(visible(full, key="er-search-no-matches"))

    def test_legacy_er_relayout_restores_nonoverlapping_table_locations(self):
        """Migrate er-diagram.ts ER-013 and show that re-layout really moves a node."""
        fixture(self.store, self.db)
        with sqlite3.connect(self.db) as db:
            db.executescript("""
                CREATE TABLE east_nodes (id INTEGER PRIMARY KEY);
                CREATE TABLE west_nodes (id INTEGER PRIMARY KEY);
            """)
            names = [name for (name,) in db.execute(
                "SELECT name FROM sqlite_schema WHERE type = 'table' ORDER BY name"
            )]
        opened = [OPEN[0], tap(3, "er-button")]

        def bounds(tree):
            result = {}
            for name in names:
                box = f"er-box-{name}"
                self.expect_rendered(tree, key=box, within="er-canvas")
                result[name] = next(n["rect"] for n in tree["elements"] if n.get("key") == box)
            return result

        baseline = bounds(self.replay(opened, until=6))
        left, nearby = names[:2]
        horizontal = baseline[nearby][0] - baseline[left][0]
        self.assertGreater(horizontal, 0)
        drag = {**tap(6.2, f"er-title-{left}"), "type": "drag", "dx": horizontal, "dy": 0, "dur": .7}
        moved = bounds(self.replay([*opened, drag], until=7.7))
        self.assertGreater(moved[left][0] - baseline[left][0], 30,
                           "drag must actually move a real table node")
        self.assertEqual(moved[nearby], baseline[nearby])

        reset = bounds(self.replay([*opened, drag, tap(8.0, "er-diagram-relayout")], until=9.6))
        self.assertEqual(reset, baseline)
        from itertools import combinations
        for (ax, ay, aw, ah), (bx, by, bw, bh) in combinations(reset.values(), 2):
            self.assertTrue(ax + aw <= bx or bx + bw <= ax or ay + ah <= by or by + bh <= ay,
                            "re-layout left two rendered table nodes overlapping")

    def test_legacy_er_zoom_controls_scale_real_nodes_not_toolbar(self):
        """Migrate er-diagram.ts ER-005, proving the canvas really zooms."""
        fixture(self.store, self.db)
        opened = [OPEN[0], tap(3.0, "er-button")]

        def geometry(tree, label):
            self.expect_rendered(tree, key=label)
            node = next(node for node in tree["elements"] if node.get("key") == label)
            return node["rect"], node["scale"]

        initial = self.replay(opened, until=5.8)
        base_node, base_scale = geometry(initial, "er-box-customers")
        base_toolbar, base_toolbar_scale = geometry(initial, "er-diagram-zoom-out")
        twice_in = [*opened, tap(6, "er-diagram-zoom-in"), tap(7, "er-diagram-zoom-in")]
        zoomed = self.replay(twice_in, until=8.5)
        node, scale = geometry(zoomed, "er-box-customers")
        self.assertGreater(node[2], base_node[2] * 1.3)
        self.assertAlmostEqual(scale / base_scale, 1.44, delta=.02)
        self.assertEqual(geometry(zoomed, "er-diagram-zoom-out"), (base_toolbar, base_toolbar_scale))

        returned = self.replay([*twice_in, tap(8.7, "er-diagram-zoom-out"),
                               tap(9.7, "er-diagram-zoom-out")], until=11)
        returned_node, returned_scale = geometry(returned, "er-box-customers")
        self.assertAlmostEqual(returned_node[2], base_node[2], delta=1)
        self.assertAlmostEqual(returned_scale, base_scale, delta=.02)
        self.expect_rendered(returned, key="er-col-customers-1", text="customer_name")

    def test_legacy_staged_grid_update_previews_bound_sql_rolls_back_and_confirms(self):
        """Migrate table-edit.ts DE-006–008 with real SQLite rows."""
        fixture(self.store, self.db)
        baseline = [(1, "fixture-ada"), (2, "fixture-lin")]

        def rows():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
                return db.execute("SELECT * FROM customers ORDER BY id").fetchall()

        def stage(value, at=5):
            return [
                {**tap(at, "cell-0-1"), "type": "double-click"},
                key(at + .6, "a", cmd=True), type_text(at + .9, value), key(at + 1.8, "Enter"),
            ]

        draft = "Ada's draft"
        first = [*OPEN, *stage(draft), tap(7.6, "pending-preview")]
        preview = self.replay(first, until=9.5)
        self.expect_rendered(preview, key="pending-changes-bar")
        self.expect_rendered(preview, key="pending-plan-dialog")
        self.expect_rendered(preview, key="pending-plan-fingerprint", within="pending-plan-dialog")
        self.expect_rendered(preview, key="pending-plan-statement-sql-0", within="pending-plan-dialog")
        template = next(n["text"] for n in preview["elements"]
                        if n.get("key") == "pending-plan-statement-sql-0")
        self.assertIn("UPDATE", template.upper())
        self.assertIn("?", template)
        self.assertNotIn(draft, template, "value must not be interpolated into the SQL template")
        self.expect_rendered(preview, key="pending-plan-statement-params-0", within="pending-plan-dialog")
        params = next(n["text"] for n in preview["elements"]
                      if n.get("key") == "pending-plan-statement-params-0")
        self.assertIn(draft, params)
        self.assertEqual(rows(), baseline)

        rolled_back = [*first, tap(9.7, "pending-plan-close"), tap(10.6, "rollback-staged")]
        rolled = self.replay(rolled_back, until=12.0)
        self.assertFalse(visible(rolled, key="pending-plan-dialog"))
        self.assertFalse(visible(rolled, key="pending-changes-bar"))
        self.expect_rendered(rolled, text="fixture-ada", within="cell-0-1")
        self.assertEqual(rows(), baseline)

        committed = "Ada's committed"
        second = [*rolled_back, *stage(committed, at=12.4), tap(15.0, "pending-preview")]
        result = self.replay([*second, tap(17.1, "pending-plan-commit")], until=19.5)
        self.assertFalse(visible(result, key="pending-plan-dialog"))
        self.assertFalse(visible(result, key="pending-changes-bar"))
        self.expect_rendered(result, text=committed, within="cell-0-1")
        self.assertEqual(rows(), [(1, committed), (2, "fixture-lin")])
        reloaded = self.replay(OPEN, until=5.5)
        self.expect_rendered(reloaded, text=committed, within="cell-0-1")
        self.expect_rendered(reloaded, text="fixture-lin", within="cell-1-1")

    def test_legacy_connection_dialog_uses_driver_defaults_and_restores_per_engine_drafts(self):
        """Migrate default ports and driver-specific drafts from new-connection.ts."""
        create = tap(.5, "connections-create-first")

        def field_value(tree, key_name):
            def find(node, inside=False):
                inside = inside or node.get("key") == key_name
                if inside and node.get("role") == "text-input":
                    return node
                for child in node.get("children", ()):
                    match = find(child, inside)
                    if match is not None:
                        return match
                return None
            self.expect_rendered(tree, key=key_name, within="connection-dialog")
            result = find(tree["accessibility"])
            self.assertIsNotNone(result, key_name)
            return result.get("value", "")

        address, name, engine = ("connection-address-input", "connection-name-input",
                                 "connection-engine-input")
        default = self.replay([create], until=1.85)
        self.assertEqual(field_value(default, engine), "postgresql")
        self.assertEqual(field_value(default, address), "127.0.0.1:5432")
        for control in ("connection-database-input", "connection-username-input",
                        "connection-test", "dialog-cancel", "connection-save"):
            self.expect_rendered(default, key=control, within="connection-dialog")

        mysql = [create, *fill(1.9, name, "PG draft"),
                 *fill(3.1, address, "pg.example.test:5440"),
                 tap(4.5, "connection-engine-option-mysql")]
        mysql_tree = self.replay(mysql, until=5.9)
        self.assertEqual(field_value(mysql_tree, engine), "mysql")
        self.assertEqual(field_value(mysql_tree, address), "127.0.0.1:3306")

        back_to_pg = [*mysql, *fill(6.0, name, "MySQL draft"),
                      *fill(7.3, address, "my.example.test:3307"),
                      tap(8.7, "connection-engine-option-postgresql")]
        pg = self.replay(back_to_pg, until=10.0)
        self.assertEqual(field_value(pg, name), "PG draft")
        self.assertEqual(field_value(pg, address), "pg.example.test:5440")

        back_to_mysql = [*back_to_pg, tap(10.2, "connection-engine-option-mysql")]
        restored = self.replay(back_to_mysql, until=11.4)
        self.assertEqual(field_value(restored, name), "MySQL draft")
        self.assertEqual(field_value(restored, address), "my.example.test:3307")

        maria = [*back_to_mysql, tap(11.6, "connection-engine-option-mariadb")]
        mariadb = self.replay(maria, until=12.9)
        self.assertEqual(field_value(mariadb, engine), "mariadb")
        self.assertEqual(field_value(mariadb, address), "127.0.0.1:3306")

        reopened = self.replay([
            *maria, tap(13.1, "dialog-cancel"), tap(14, "connections-create-first"),
        ], until=15.4)
        self.assertEqual(field_value(reopened, engine), "postgresql")
        self.assertEqual(field_value(reopened, name), "")
        self.assertEqual(field_value(reopened, address), "127.0.0.1:5432")
        self.assertFalse(self.store.exists(), "switching and cancelling a draft must never save it")
