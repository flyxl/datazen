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

    def test_native_execute_shortcut_setting_runs_only_from_its_focused_editor_after_relaunch(self):
        original = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        settings = self.root / "settings.json"
        prefs = {"ui": {"language": "en"}, "keymapPreset": "default",
                 "customKeymap": {"saveQuery": "Mod-s", "future": ["preserve"]}, "unrelated": 17}
        settings.write_text(json.dumps(prefs), encoding="utf-8")
        initial = settings.read_bytes()
        db_bytes = self.db.read_bytes()
        trigger = "settings-execute-shortcut-trigger"
        route = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-editor")]
        default = self.replay(route, until=2.5)
        self.expect_rendered(default, text="Mod+Enter", within=trigger)
        card = "settings-keymap-customize"
        self.expect_rendered(default, key=card, within="settings-section-editor")
        self.expect_rendered(default, text="Customize Shortcuts", within="settings-keymap-customize-header")
        self.expect_rendered(default, key="settings-execute-shortcut-row", within="settings-keymap-customize-rows")
        self.expect_rendered(default, key=trigger, within=card)
        _, top, _, height = self._settled_rect(default, card)
        _, syntax_top, _, syntax_height = self._settled_rect(default, "settings-live-syntax-theme")
        _, snippets_top, _, _ = self._settled_rect(default, "settings-snippets")
        self.assertLessEqual(syntax_top + syntax_height, top)
        self.assertLessEqual(top + height, snippets_top)
        for absent in ("settings-keymap-preset-trigger", "settings-keymap-reset", "settings-keymap-row-formatSql"):
            self.assertFalse(visible(default, key=absent))
        self.assertEqual(settings.read_bytes(), initial)

        # Fail a real atomic file-stage write; the UI must still show Mod+Enter.
        obstruction = settings.with_suffix(".json.tmp")
        obstruction.mkdir()
        change = [*route, tap(2.65, trigger), tap(3.55, "option1")]
        failed = self.replay(change, until=4.65)
        self.expect_rendered(failed, key="settings-error")
        self.expect_rendered(failed, text="Mod+Enter", within=trigger)
        self.assertEqual(settings.read_bytes(), initial)
        obstruction.rmdir()
        saved = self.replay(change, until=4.65)
        self.expect_rendered(saved, text="Mod+R", within=trigger)
        self.assertFalse(visible(saved, key="settings-error"))
        expected = {**prefs, "customKeymap": {**prefs["customKeymap"], "execute": "Mod-r"}}
        self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), expected)

        # Each replay is a fresh binary. Confirm the old key, an extra
        # modifier and the real focused Find field execute nothing.
        picked = "SELECT 'shortcut-picked' AS value;"
        query = query_steps(picked + "\nSELECT 'not-selected' AS value;")[:-1]
        query[-1] = {"t": 4.8, "type": "paste", "text": picked + "\nSELECT 'not-selected' AS value;"}
        history = self.root / "query-history.json"
        blocked = self.replay([*query, key(6.1, "Enter", cmd=True),
                               {**key(6.45, "r", cmd=True), "shift": True},
                               {**key(6.8, "r", cmd=True), "repeat": True},
                               key(7.15, "f", cmd=True), key(7.65, "r", cmd=True)], until=8.5)
        self.expect_rendered(blocked, key="sql-find-input")
        self.expect_rendered(blocked, key="query-idle-hint", text="Press Mod+R in the SQL editor to execute")
        self.assertFalse(visible(blocked, text="Press ⌘+Enter to execute", within="query-main"))
        self.assertFalse(visible(blocked, key="sql-line-2"), "old Mod+Enter inserted a line")
        self.assertFalse(visible(blocked, key="result-workspace"))
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))

        selected = [*query, key(6.1, "Home"), key(6.45, "Up"), key(6.8, "End"),
                    {**key(7.15, "Home"), "shift": True}]
        ran = self.replay([*selected, key(7.8, "r", cmd=True)], until=10.1)
        self.expect_rendered(ran, key="sql-selected-0-0", text=picked)
        self.expect_rendered(ran, text="shortcut-picked", within="result-cell-0-0")
        self.assertFalse(visible(ran, text="not-selected", within="result-workspace"))
        self.assertEqual([item["sql"].rstrip(";") for item in json.loads(history.read_text(encoding="utf-8"))],
                         [picked.rstrip(";")])
        repeated = self.replay([
            *selected, key(7.8, "r", cmd=True),
            {**key(10.4, "r", cmd=True), "repeat": True},
        ], until=11.6)
        self.expect_rendered(repeated, text="shortcut-picked", within="result-cell-0-0")
        self.assertEqual(len({node["key"] for node in repeated["elements"]
                              if (node.get("key") or "").startswith("query-result-tab-")
                              and not node.get("exiting")}), 1)
        self.assertEqual(self.sql_customer_rows(), original)
        self.assertEqual(self.db.read_bytes(), db_bytes)
        self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), expected)

    def test_native_execute_shortcut_keeps_legacy_run_from_results_when_default_or_restored(self):
        self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        settings = self.root / "settings.json"
        prefs = {"ui": {"language": "en"}, "customKeymap": {"future": ["keep"]}}
        settings.write_text(json.dumps(prefs), encoding="utf-8")
        original = settings.read_bytes()
        before = self.db.read_bytes()

        def legacy_run(value):
            statement = f"SELECT '{value}';"
            query = query_steps(statement)
            query[-2] = {"t": 4.8, "type": "paste", "text": statement}
            # Mod+R is inactive even on the editor. The old app command works
            # after a result cell took focus, with Shift as it did before.
            ran = self.replay([*query[:-1], key(7.5, "r", cmd=True), query[-1],
                               tap(10.3, "result-cell-0-0"),
                               {**key(10.9, "Enter", cmd=True), "shift": True}], until=12.9)
            self.expect_rendered(ran, text=value, within="result-cell-0-0")
            self.assertEqual(len({node["key"] for node in ran["elements"]
                                  if (node.get("key") or "").startswith("query-result-tab-")
                                  and not node.get("exiting")}), 2)

        legacy_run("legacy-when-absent")
        self.assertEqual(settings.read_bytes(), original)

        settings.write_text(json.dumps({**prefs, "customKeymap": {
            **prefs["customKeymap"], "execute": "Mod-r",
        }}), encoding="utf-8")
        trigger = "settings-execute-shortcut-trigger"
        restored = self.replay([
            tap(.5, "rail-settings"), tap(1.4, "settings-nav-editor"),
            tap(2.65, trigger), tap(3.55, "option0"),
        ], until=4.65)
        self.expect_rendered(restored, text="Mod+Enter", within=trigger)
        self.assertFalse(visible(restored, key="settings-error"))
        self.assertEqual(json.loads(settings.read_text(encoding="utf-8")), {
            **prefs, "customKeymap": {**prefs["customKeymap"], "execute": "Mod-Enter"},
        })
        restored_bytes = settings.read_bytes()
        legacy_run("legacy-when-restored")
        self.assertEqual(settings.read_bytes(), restored_bytes)
        self.assertEqual(self.db.read_bytes(), before)

    def test_native_query_toolbar_snippets_preserve_literal_saved_sql_and_undo_without_execution(self):
        original = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        db_bytes = self.db.read_bytes()
        saved_name = "query.run"  # not a translation key when it is a user name
        saved_sql = "SELECT '${1:literal} 中文';"
        self.replay([
            tap(.5, "rail-settings"), tap(1.4, "settings-nav-editor"),
            tap(1.95, "snippet-add"), *fill(2.65, "snippet-name-input", saved_name),
            tap(3.8, "snippet-sql-input"), key(4.05, "a", cmd=True),
            {"t":4.25,"type":"paste","text":saved_sql}, tap(5.4, "snippet-save"),
        ], until=6.6)
        saved_file = self.root / "snippets.json"
        saved_stamp = saved_file.read_bytes(), saved_file.stat().st_mtime_ns
        payload = json.loads(saved_stamp[0])
        self.assertTrue(any(item["name"] == saved_name and item["sql"] == saved_sql
                            for item in payload))

        def draft_line(tree, index):
            row = next(i for i in tree["elements"] if i.get("key") == f"sql-line-{index}" and not i.get("exiting"))
            return "".join(n.get("text") or "" for n in tree["elements"]
                           if n["path"].startswith(row["path"] + "/") and not n.get("exiting"))

        def focused(tree, node):
            self.assertTrue(any(i.get("key") == node and i.get("focused") and not i.get("exiting")
                                for i in tree["elements"]), node)

        original_sql = "SELECT '原稿';"
        route = query_steps(original_sql)[:-1]
        route[-1] = {"t":4.8,"type":"paste","text":original_sql}
        route.extend([key(5.7, "a", cmd=True), tap(6.3, "query-library-history-toggle")])
        idle = self.replay(route, until=7.0)
        editor = self._settled_rect(idle, "sql-editor-scroll")
        self.assertFalse(visible(idle, key="query-snippets-popup"))
        opened = [*route, tap(7.15, "query-library-snippets-toggle")]
        guarded = [*opened, key(7.7, "Enter", cmd=True), type_text(8.05, "q"),
                   {"t":8.5,"type":"paste","text":"must not enter the draft"}]
        menu = self.replay(guarded, until=9.15)
        self.expect_rendered(menu, key="snippet-menu")
        self.expect_rendered(menu, key="snippet-saved-0-label", text=saved_name)
        self.expect_rendered(menu, key="query-history-panel")
        self.assertEqual(self._settled_rect(menu, "sql-editor-scroll"), editor)
        self.assertEqual(draft_line(menu, 0), original_sql)
        focus_back = [*guarded, key(9.3, "Escape")]
        closed = self.replay(focus_back, until=9.9)
        self.assertFalse(visible(closed, key="query-snippets-popup"))
        focused(closed, "query-library-snippets-toggle")
        activation = [*focus_back, {"t":10.1,"type":"key","name":" ","code":"Space"},
                      key(10.55, "Down"), key(11.0, "Enter")]
        builtin = self.replay(activation, until=11.8)
        self.assertEqual(draft_line(builtin, 0), "SELECT columns")
        self.assertEqual(draft_line(builtin, 1), "FROM table_name;")
        self.expect_rendered(builtin, key="sql-selected-1-5", text="table_name")
        self.assertFalse(visible(builtin, key="query-snippets-popup"))
        focused(builtin, "sql-editor-scroll")
        history_undo = [*activation, key(12.1, "z", cmd=True)]
        undone = self.replay(history_undo, until=12.85)
        self.assertEqual(draft_line(undone, 0), original_sql)
        self.expect_rendered(undone, key="sql-selected-0-0")
        literal = [*history_undo, tap(13.0, "query-library-snippets-toggle"), tap(13.65, "snippet-saved-0")]
        saved = self.replay(literal, until=14.45)
        self.assertEqual(draft_line(saved, 0), saved_sql)
        focused(saved, "sql-editor-scroll")
        restored = self.replay([*literal, key(14.7, "z", cmd=True)], until=15.4)
        self.assertEqual(draft_line(restored, 0), original_sql)
        self.assertFalse(visible(restored, key="result-workspace"))
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
        self.assertEqual((saved_file.read_bytes(), saved_file.stat().st_mtime_ns), saved_stamp)
        self.assertEqual(self.sql_customer_rows(), original)
        self.assertEqual(self.db.read_bytes(), db_bytes)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name+suffix).exists())

    def test_native_sql_physically_typed_space_preserves_draft_and_read_history(self):
        original = self.sql_fixture_before()
        store = json.loads(self.store.read_text(encoding="utf-8"))
        store["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(store), encoding="utf-8")
        db_bytes = self.db.read_bytes()
        sql = "SELECT  '双 空格'  AS note;"
        route = [tap(.5, "conn-open-e2e-sqlite"), tap(3.0, "conn-toolbar-new-query"),
                 tap(4.0, "sql-editor-scroll")]
        typed = [*route, type_text(4.45, "SELECT"),
                 {"t":4.95,"type":"key","name":" ","code":"Space"},
                 {"t":5.25,"type":"key","name":" ","code":"Space","shift":True},
                 type_text(5.65, "'双 空格'  AS note;")]

        def actual_draft(tree):
            row = next(n for n in tree["elements"] if n.get("key")=="sql-line-0")
            return "".join(n.get("text") or "" for n in tree["elements"]
                           if n["path"].startswith(row["path"]+"/"))
        typed_tree = self.replay(typed, until=6.8)
        self.assertEqual(actual_draft(typed_tree), sql, "real typed spaces never reach the editor")
        self.assertFalse(visible(typed_tree, key="result-workspace"))
        # Independent stock clipboard-path control, never used by the typing
        # or execution witness; exact drafts must match.
        pasted = self.replay([*route, {"t":4.45,"type":"paste","text":sql}], until=5.55)
        self.assertEqual(actual_draft(pasted), sql)
        self.assertFalse(visible(pasted, key="result-workspace"))

        result = self.replay([*typed, tap(7.1, "query-library-history-toggle"),
                              tap(8.2, "sql-run")], until=10.2)
        self.expect_rendered(result, text="双 空格", within="result-cell-0-0")
        # The history card intentionally collapses whitespace for its preview;
        # the actual stored SQL and draft above must preserve every byte.
        preview = " ".join(sql.rstrip(";").split())
        self.expect_rendered(result, text=preview, within="query-history-entry-0")
        history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
        self.assertEqual([(r["sql"].rstrip(";"), r["success"]) for r in history],
                         [(sql.rstrip(";"), True)])
        self.assertEqual(self.sql_customer_rows(), original)
        self.assertEqual(self.db.read_bytes(), db_bytes)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name+suffix).exists())

    def test_native_sql_editor_keyword_completion_undo_preserves_caret_run(self):
        """Tab accepts the highlighted keyword as one edit without running SQL."""
        original = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        db_bytes = self.db.read_bytes()
        prefix = "UPDATE customers SET customer_name='must-not-run' WHERE id=1;\n"
        steps = query_steps(prefix)[:-1]
        steps[-1] = {"t": 4.8, "type": "paste", "text": prefix}
        typed = [*steps, type_text(6.0, "s")]
        offered = self.replay(typed, until=7.0)
        for i, option in enumerate(("SELECT", "SET", "SUM")):
            self.expect_rendered(offered, text=option, within=f"sql-completion-item-{i}")

        selected = [*typed, key(7.1, "Down"), key(7.6, "Tab")]
        insertion = self.replay(selected, until=8.3)
        self.expect_rendered(insertion, text="SET", within="sql-line-1")
        canvas = [n for n in insertion["elements"]
                  if n.get("key") == "sql-editor-scroll" and not n.get("exiting")]
        self.assertEqual(len(canvas), 1)
        self.assertTrue(canvas[0]["focused"], "acceptance keeps the caret in the real Query canvas")
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
        self.assertEqual(self.db.read_bytes(), db_bytes)

    def test_native_sql_editor_common_function_completion_undo_and_real_sqlite_run(self):
        """The live base source offers the label and inserts its distinct apply text."""
        original = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        db_bytes = self.db.read_bytes()
        history = self.root / "query-history.json"

        def draft(tree):
            row = next(n for n in tree["elements"] if n.get("key") == "sql-line-0")
            return "".join(n.get("text") or "" for n in tree["elements"]
                           if n["path"].startswith(row["path"] + "/"))

        prefix = "SELECT "
        applied = "SELECT COALESCE("
        entry = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": prefix}]
        typed = [*entry, type_text(5.6, "coa")]
        offered = self.replay(typed, until=6.6)
        self.assertEqual(draft(offered), prefix + "coa")
        self.expect_rendered(offered, text="COALESCE", within="sql-completion-item-0")
        self.assertFalse(visible(offered, text="COALESCE(", within="sql-completion"))

        accepted = [*typed, key(7.0, "Tab")]
        selected = self.replay(accepted, until=7.7)
        self.assertEqual(draft(selected), applied)
        self.assertFalse(visible(selected, key="sql-completion"))
        self.assertFalse(visible(selected, key="result-workspace"))

        undo = [*accepted, key(8.0, "z", cmd=True)]
        restored = self.replay(undo, until=8.7)
        self.assertEqual(draft(restored), prefix + "coa")
        self.assertFalse(visible(restored, key="sql-completion"))
        self.assertFalse(visible(restored, key="result-workspace"))
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))

        suffix = "NULL, customer_name) FROM customers ORDER BY id;"
        statement = applied + suffix
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(statement).fetchall()
        result = self.replay([
            *undo, {**key(9.1, "z", cmd=True), "shift": True},
            {"t": 9.9, "type": "paste", "text": suffix},
            tap(10.8, "sql-run"),
        ], until=12.9)
        self.assertEqual(draft(result), statement)
        self.assertFalse(visible(result, key="query-error-message"))
        for row, (name,) in enumerate(expected):
            self.expect_rendered(result, text=name, within=f"result-cell-{row}-0")
        self.assertEqual([(item["success"], item["sql"].rstrip(";"))
                          for item in json.loads(history.read_text(encoding="utf-8"))],
                         [(True, statement.rstrip(";"))])
        self.assertEqual(self.sql_customer_rows(), original)
        self.assertEqual(self.db.read_bytes(), db_bytes)

    def test_native_sql_editor_tab_accepts_real_sqlite_from_completion(self):
        self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        db_bytes = self.db.read_bytes()
        prefix = "SELECT customer_name FROM "
        candidate = '"main"."customers"'
        selected = prefix + candidate
        suffix = " ORDER BY id;"
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(selected + suffix).fetchall()
        typed = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": prefix},
                 type_text(5.6, "cust")]
        offered = self.replay(typed, until=6.6)
        self.expect_rendered(offered, text=candidate, within="sql-completion-item-0")

        accepted = [*typed, key(7.0, "Tab")]
        result = self.replay([*accepted, {"t": 7.65, "type": "paste", "text": suffix},
                              tap(8.85, "sql-run")], until=10.8)
        self.assertFalse(visible(result, key="sql-completion"))
        self.assertFalse(visible(result, key="query-error-message"))
        for row, (name,) in enumerate(expected):
            self.expect_rendered(result, text=name, within=f"result-cell-{row}-0")
        history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
        self.assertEqual([(item["success"], item["sql"].rstrip(";")) for item in history],
                         [(True, (selected + suffix).rstrip(";"))])
        self.assertEqual(self.db.read_bytes(), db_bytes)

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
        # Writable SQLite connections initialize WAL when opened. This
        # no-execution Find journey must use the real read-only profile
        # before taking its exact database-byte witness.
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
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
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())

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

    def test_editor_replace_first_seeks_then_only_plain_enter_replaces_the_current_match(self):
        original_rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original_bytes = self.db.read_bytes()
        sql = "SELECT 'cat' AS first_value, 'cat' AS second_value;"
        before = [
            *query_steps(sql)[:-1], tap(8.3, "sql-find-open"),
            *fill(8.9, "sql-find-input", "cat"),
            *fill(9.7, "sql-replace-input", "dog"),
            tap(10.8, "sql-replace-current"),
        ]
        selected = self.replay(before, until=11.4)
        self.expect_rendered(selected, key="sql-selected-0-8", text="cat")
        self.assertFalse(visible(selected, key="result-workspace"))
        history_path = self.root / "query-history.json"
        self.assertEqual(json.loads(history_path.read_bytes()) if history_path.exists() else [],
                         [], "seeking never executes")
        replace = [
            *before, tap(11.4, "sql-replace-input"),
            key(11.8, "Enter", cmd=True),
            key(12.2, "Enter"),
        ]
        changed = self.replay(replace, until=12.8)
        next_offset = sql.rfind("cat")
        self.expect_rendered(changed, key=f"sql-selected-0-{next_offset}", text="cat")
        self.assertFalse(visible(changed, key="result-workspace"))

        run = self.replay([
            *replace, key(12.8, "Escape"), key(13.15, "Left"), tap(13.6, "sql-run"),
        ], until=15.5)
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(sql.replace("cat", "dog", 1)).fetchone()
        self.assertEqual(expected, ("dog", "cat"))
        for index, value in enumerate(expected):
            self.expect_rendered(run, text=value, within=f"result-cell-0-{index}")
        self.assertEqual(self.sql_customer_rows(), original_rows)
        self.assertEqual(self.db.read_bytes(), original_bytes)
        self.assertEqual(len(json.loads(history_path.read_bytes())), 1)

    def test_editor_find_buttons_wrap_sole_and_overlapping_matches_without_editing_sqlite(self):
        original_rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original_db = self.db.read_bytes()

        def editor(sql):
            events = query_steps(sql)[:-1]
            events[-1] = {"t": 4.8, "type": "paste", "text": sql}
            return events

        sole = "SELECT '猫';"
        selected = [
            *editor(sole), key(6.3, "End"), key(6.7, "Left"), key(7.1, "Left"),
            {**key(7.5, "Left"), "shift": True}, tap(8.1, "sql-find-open"),
        ]
        before = self.replay(selected, until=8.65)
        self.expect_rendered(before, text="猫", within="sql-find-input")
        self.expect_rendered(before, key="sql-selected-0-8", text="猫")
        wrapped = self.replay([
            *selected, tap(8.9, "sql-find-next"), key(9.4, "Escape"),
            {**key(9.9, "Left"), "shift": True},
        ], until=10.45)
        # Find remounts the sole match as a forward range. Shift+Left now
        # collapses it rather than extending a backward range over the quote.
        self.expect_rendered(wrapped, key="sql-caret", within="sql-editor-scroll")
        self.assertFalse(visible(wrapped, key="sql-selected-0-7"))
        self.assertFalse(visible(wrapped, key="sql-selected-0-8"))
        self.assertFalse(visible(wrapped, key="sql-find-bar"))

        overlap = "SELECT 'aabbbba' AS overlap;"
        find = [
            *editor(overlap), key(6.3, "End"), {**key(6.8, "Home"), "shift": True},
            tap(7.5, "sql-find-open"), *fill(8.15, "sql-find-input", "bb"),
        ]
        first = self.replay([*find, tap(9.2, "sql-find-next")], until=9.85)
        last = self.replay([*find, tap(9.2, "sql-find-prev")], until=9.85)
        for scene, expected, other in ((first, 10, 12), (last, 12, 10)):
            self.expect_rendered(scene, text="bb", within="sql-find-input")
            self.expect_rendered(scene, key=f"sql-selected-0-{expected}", text="bb")
            self.assertFalse(visible(scene, key=f"sql-selected-0-{other}"))
        for scene in (before, wrapped, first, last):
            self.assertFalse(visible(scene, key="result-workspace"))
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
        self.assertEqual(self.sql_customer_rows(), original_rows)
        self.assertEqual(self.db.read_bytes(), original_db)

    def test_editor_find_close_corner_stays_clickable_at_wide_and_narrow_width_in_both_themes(self):
        original_rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original_db = self.db.read_bytes()

        sql = "SELECT 'xcat' AS prefix, 'cat' AS standalone;"
        enter = query_steps(sql)[:-1]
        enter[-1] = {"t": 4.8, "type": "paste", "text": sql}
        find = [*enter, key(6.35, "f", cmd=True),
                {"t": 6.85, "type": "paste", "text": "cat"}]
        settings = self.root / "settings.json"
        dock_colors = {}
        for theme in ("light", "dark"):
            settings.write_text(json.dumps({"ui": {"language": "en", "themeMode": theme}}), encoding="utf-8")
            settings.chmod(0o600)
            for size in ("1440x900", "720x640"):
                with self.subTest(theme=theme, size=size):
                    opened = self.replay(find, until=7.4, size=size)
                    dock = self._settled_rect(opened, "sql-find-bar")
                    dock_colors[theme, size] = next(n["fill"] for n in opened["elements"]
                                                    if n.get("key") == "sql-find-bar" and not n.get("exiting"))
                    first = self._settled_rect(opened, "sql-find-conditions-scroll")
                    field = self._settled_rect(opened, "sql-find-input")
                    replace = self._settled_rect(opened, "sql-replace-input")
                    close = self._settled_rect(opened, "sql-find-close")
                    button = next(n for n in opened["elements"]
                                  if n.get("key") == "sql-find-close" and not n.get("exiting"))
                    self.assertEqual(button["role"], "button")
                    self.assertEqual(button["label"], "Close find")
                    self.assertTrue(button["focusable"])
                    self.expect_rendered(opened, text="×", within="sql-find-close")
                    self.expect_rendered(opened, text="cat", within="sql-find-input")
                    self.assertEqual(close[2:], [20.0, 20.0])
                    self.assertAlmostEqual(dock[0] + dock[2] - close[0] - close[2], 6.0, delta=.5)
                    self.assertGreaterEqual(close[0] - first[0] - first[2], 5.5)
                    self.assertAlmostEqual(close[1] + close[3] / 2, field[1] + field[3] / 2, delta=1.01)
                    self.assertLess(close[1] + close[3], replace[1])
                    self.assertFalse(visible(opened, key="result-workspace"))

                    if size == "720x640":
                        right = first[0] + first[2]
                        # Raw shape, since it is allowed to begin outside the
                        # visible scroll viewport before a wheel.
                        whole = next(n["rect"] for n in opened["elements"]
                                     if n.get("key") == "sql-find-whole-word" and not n.get("exiting"))
                        self.assertGreater(whole[0] + whole[2], right)
                        wheel = {"t": 7.65, "type": "wheel", "key": "sql-find-conditions-scroll", "dx": -500}
                        physical_close = {"t": 8.3, "type": "tap", "x": close[0] + close[2] / 2,
                                          "y": close[1] + close[3] / 2}
                        closed = self.replay([*find, wheel, physical_close], until=8.95, size=size)
                        self.assertFalse(visible(closed, key="sql-find-bar"))
                        self.assertFalse(visible(closed, key="result-workspace"))
                        self.assertTrue(any(n.get("key") == "sql-editor-scroll" and n.get("focused")
                                            and not n.get("exiting") for n in closed["elements"]))

        for size in ("1440x900", "720x640"):
            self.assertNotEqual(dock_colors["light", size], dock_colors["dark", size])
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_bytes()))
        self.assertEqual(self.sql_customer_rows(), original_rows)
        self.assertEqual(self.db.read_bytes(), original_db)

    def test_editor_literal_escape_search_selects_across_lines_and_replaces_real_sql(self):
        original_rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original_bytes = self.db.read_bytes()
        sql = "SELECT '猫' AS first_tag,\n\t'猫' AS second_tag;"
        enter = query_steps(sql)[:-1]
        enter[-1] = {"t": 4.8, "type": "paste", "text": sql}
        needle = r"猫' AS first_tag,\n\t'猫"
        find = [
            *enter, tap(8.3, "sql-find-open"),
            *fill(8.9, "sql-find-input", needle),
            # fill starts type_text at 9.350; the final scalar arrives at 9.917.
            # A pointer press before then swaps focus to Next and drops that input.
            tap(10.2, "sql-find-next"),
        ]
        selected = self.replay(find, until=10.6)
        self.expect_rendered(selected, text=needle, within="sql-find-input")
        self.expect_rendered(selected, key="sql-selected-0-8", text="猫' AS first_tag,")
        self.expect_rendered(selected, key="sql-selected-1-0", text="\t'猫")
        self.assertFalse(visible(selected, key="result-workspace"))

        run = self.replay([
            *find, *fill(10.7, "sql-replace-input", r"狗' AS first_tag,\r\n\t'猫"),
            tap(12.1, "sql-replace-current"),
            key(12.6, "Escape"), key(13.0, "Left"), tap(13.5, "sql-run"),
        ], until=15.4)
        expected_sql = sql.replace("猫", "狗", 1)
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(expected_sql).fetchone()
        self.assertEqual(expected, ("狗", "猫"))
        for index, value in enumerate(expected):
            self.expect_rendered(run, text=value, within=f"result-cell-0-{index}")
        self.assertEqual(self.sql_customer_rows(), original_rows)
        self.assertEqual(self.db.read_bytes(), original_bytes)
        history = json.loads((self.root / "query-history.json").read_bytes())
        self.assertEqual([row["sql"].rstrip(";") for row in history], [expected_sql.rstrip(";")])

    def test_editor_literal_match_case_replaces_the_original_ligature_only(self):
        original_rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original_bytes = self.db.read_bytes()
        sql = "SELECT 'FI' AS upper_tag, 'ﬃ' AS ligature_tag;"
        enter = query_steps(sql)[:-1]
        enter[-1] = {"t": 4.8, "type": "paste", "text": sql}
        find = [
            *enter, tap(8.3, "sql-find-open"), *fill(8.9, "sql-find-input", "fi"),
            tap(9.8, "sql-find-next"),
        ]
        folded = self.replay(find, until=10.4)
        self.expect_rendered(folded, key="sql-find-match-case")
        self.expect_rendered(folded, key="sql-selected-0-8", text="FI")
        self.assertFalse(visible(folded, key="result-workspace"))

        exact = [*find, tap(10.8, "sql-find-match-case"), tap(11.3, "sql-find-next")]
        matched = self.replay(exact, until=11.9)
        offset = len(sql.split("ﬃ", 1)[0].encode("utf-8"))
        self.expect_rendered(matched, key=f"sql-selected-0-{offset}", text="ﬃ")
        self.assertFalse(visible(matched, key="sql-selected-0-8"))

        run = self.replay([
            *exact, *fill(12.0, "sql-replace-input", "safe"),
            tap(13.2, "sql-replace-current"),
            key(13.8, "Escape"), key(14.2, "Left"), tap(14.6, "sql-run"),
        ], until=16.5)
        edited = sql.replace("ﬃ", "safe")
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as db:
            expected = db.execute(edited).fetchone()
        self.assertEqual(expected, ("FI", "safe"))
        for index, value in enumerate(expected):
            self.expect_rendered(run, text=value, within=f"result-cell-0-{index}")
        self.assertEqual(self.sql_customer_rows(), original_rows)
        self.assertEqual(self.db.read_bytes(), original_bytes)
        history = json.loads((self.root / "query-history.json").read_bytes())
        self.assertEqual([row["sql"].rstrip(";") for row in history], [edited.rstrip(";")])

    def test_editor_literal_whole_word_tabs_replace_all_and_undo_on_read_only_sqlite(self):
        original_rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        original_db = self.db.read_bytes()
        sql = ("SELECT 'xcat' AS ascii_prefix, '猫cat' AS cjk_prefix, "
               "'cat_suffix' AS underline, 'cat' AS exact, 'cat-cat' AS punctuated, 'cat猫' AS cjk_suffix;")
        edited = sql.replace("'cat'", "'dog'").replace("'cat-cat'", "'dog-dog'")

        def byte_offset(phrase, inner=0):
            return len(sql[:sql.index(phrase) + inner].encode("utf-8"))

        ordinary = f"sql-selected-0-{byte_offset('xcat', 1)}"
        standalone_offset = byte_offset("'cat'", 1)
        whole = f"sql-selected-0-{standalone_offset}"
        underscore = f"sql-selected-0-{byte_offset('cat_suffix')}"
        cjk = f"sql-selected-0-{byte_offset('猫cat', 1)}"
        enter = query_steps(sql)[:-1]
        enter[-1] = {"t": 4.8, "type": "paste", "text": sql}
        find = [
            *enter, tap(8.3, "sql-find-open"), *fill(8.9, "sql-find-input", "cat"),
            tap(9.8, "sql-find-next"),
        ]
        substring = self.replay(find, until=10.4)
        self.expect_rendered(substring, key="sql-find-whole-word", within="sql-find-bar")
        self.expect_rendered(substring, key=ordinary, text="cat")
        checked = [*find, tap(10.8, "sql-find-whole-word"), tap(11.3, "sql-find-next")]
        first = self.replay(checked, until=12)
        self.expect_rendered(first, key=whole, text="cat")
        for rejected in (ordinary, underscore, cjk):
            self.assertFalse(visible(first, key=rejected))
        tabs = [n["key"] for n in first["elements"] if not n.get("exiting")
                and n.get("role") == "tab" and (n.get("key") or "").startswith("tab-")]
        self.assertEqual(len(tabs), 1)
        ax, ay, aw, ah = self._settled_rect(first, tabs[0])

        # Grow a second real Query draft while A's Find dock is open.
        other = [
            *checked, tap(12.4, "conn-toolbar-new-query"), tap(13.25, "sql-editor-scroll"),
            {"t": 13.7, "type": "paste", "text": sql}, tap(14.3, "sql-find-open"),
            *fill(14.9, "sql-find-input", "cat"), tap(15.8, "sql-find-next"),
        ]
        other_draft = self.replay(other, until=16.4)
        self.expect_rendered(other_draft, key=ordinary, text="cat")
        self.assertFalse(visible(other_draft, key=whole))

        returned = [
            *other, {"t": 16.8, "type": "tap", "x": ax + min(8, aw/4), "y": ay+ah/2},
            tap(17.4, "sql-editor-scroll"), key(17.75, "Home"),
            tap(18.15, "sql-find-open"), tap(18.8, "sql-find-next"),
        ]
        return_a = self.replay(returned, until=19.3)
        self.expect_rendered(return_a, key=whole, text="cat")
        self.assertFalse(visible(return_a, key=ordinary))
        self.assertFalse(visible(return_a, key="result-workspace"))
        history_path = self.root / "query-history.json"
        self.assertFalse(history_path.exists() and json.loads(history_path.read_text(encoding="utf-8")))

        replace = [
            *checked, *fill(12.1, "sql-replace-input", "dog"),
            tap(13.2, "sql-replace-all"), key(13.8, "Escape"),
        ]
        changed = self.replay([*replace, key(14.25, "Left"), tap(14.7, "sql-run")], until=16.7)
        undone = self.replay([
            *replace, key(14.25, "z", cmd=True), key(14.65, "Left"), tap(15.1, "sql-run"),
        ], until=17.1)
        with closing(sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True)) as independent:
            expected_changed = independent.execute(edited).fetchone()
            expected_undone = independent.execute(sql).fetchone()
        self.assertEqual(expected_changed, ("xcat", "猫cat", "cat_suffix", "dog", "dog-dog", "cat猫"))
        self.assertEqual(expected_undone, ("xcat", "猫cat", "cat_suffix", "cat", "cat-cat", "cat猫"))
        for tree, expected in ((changed, expected_changed), (undone, expected_undone)):
            for col, text in enumerate(expected):
                self.expect_rendered(tree, text=text, within=f"result-cell-0-{col}")
        history = json.loads(history_path.read_text(encoding="utf-8"))
        self.assertCountEqual([item["sql"].rstrip(";") for item in history],
                              [edited.rstrip(";"), sql.rstrip(";")])
        self.assertTrue(all(item["success"] for item in history))
        self.assertEqual(self.db.read_bytes(), original_db)
        self.assertEqual(self.sql_customer_rows(), original_rows)
        for suffix in ("-wal", "-shm", "-journal"):
            self.assertFalse(self.db.with_name(self.db.name + suffix).exists())

    def test_native_sql_editor_manual_completion_from_empty_real_sqlite_prefix(self):
        original = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        before = self.db.read_bytes()
        # The actual window's named Space is text with Control. Preserve its
        # explicit physical code in the script; never synthesize text to open.
        ctrl_space = {"type": "key", "name": " ", "code": "Space", "base": " ", "ctrl": True}
        prefix = "SELECT customer_name FROM "
        candidate = '"main"."customers"'
        route = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": prefix.rstrip()},
                 {"t": 5.35, "type": "key", "name": " ", "code": "Space"}]

        def draft(tree, expected):
            row = next(node for node in tree["elements"]
                       if node.get("key") == "sql-line-0" and not node.get("exiting"))
            actual = "".join(node.get("text") or "" for node in tree["elements"]
                             if node["path"].startswith(row["path"] + "/") and not node.get("exiting"))
            self.assertEqual(actual, expected)
            self.assertFalse(visible(tree, key="result-workspace"))

        idle = self.replay(route, until=5.9)
        draft(idle, prefix)
        self.assertFalse(visible(idle, key="sql-completion"))

        offered = [*route, {"t": 6.05, **ctrl_space}]
        popup = self.replay(offered, until=6.65)
        draft(popup, prefix)
        self.expect_rendered(popup, text=candidate, within="sql-completion-item-0")
        accepted = [*offered, key(7.0, "Tab")]
        inserted = self.replay(accepted, until=7.65)
        draft(inserted, prefix + candidate)
        self.assertFalse(visible(inserted, key="sql-completion"))
        restored = self.replay([*accepted, key(7.85, "z", cmd=True)], until=8.45)
        draft(restored, prefix)
        self.assertFalse(visible(restored, key="sql-completion"))

        blocked = self.replay([*route, key(6.05, "f", cmd=True),
                               {"t": 6.65, **ctrl_space}], until=7.4)
        draft(blocked, prefix)
        self.expect_rendered(blocked, key="sql-find-input")
        self.assertFalse(visible(blocked, key="sql-completion"))

        self.assertEqual(self.sql_customer_rows(), original)
        self.assertEqual(self.db.read_bytes(), before)
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))

    def test_editor_select_all_literal_matches_edits_only_selected_ranges_after_dock_close(self):
        rows = self.sql_fixture_before()
        profile = json.loads(self.store.read_text(encoding="utf-8"))
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf-8")
        before = self.db.read_bytes()
        lines = ["SELECT 'cat' AS first_v,", "  'CAT' AS upper_v, 'xcat' AS embedded_v,", "  'cat' AS last_v;"]
        sql = "\n".join(lines)
        select_all = "sql-find-select-all-matches"
        first_match = f"sql-selected-0-{lines[0].index('cat')}"
        upper_match = f"sql-selected-1-{lines[1].index('CAT')}"
        embedded = f"sql-selected-1-{lines[1].index('xcat') + 1}"
        last_match = f"sql-selected-2-{lines[2].index('cat')}"

        def current_draft(tree):
            def line_text(index):
                row = next(n for n in tree["elements"]
                           if n.get("key") == f"sql-line-{index}" and not n.get("exiting"))
                return "".join(n.get("text") or "" for n in tree["elements"]
                               if n["path"].startswith(row["path"] + "/") and not n.get("exiting"))
            return "\n".join(line_text(index) for index in range(len(lines)))

        opened = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": sql},
                  tap(6.0, "sql-find-open"), *fill(6.65, "sql-find-input", "cat"),
                  tap(7.75, "sql-find-whole-word")]
        all_matches = [*opened, tap(8.4, select_all)]
        picked = self.replay(all_matches, until=9.1)
        self.expect_rendered(picked, key=first_match, text="cat")
        self.expect_rendered(picked, key=upper_match, text="CAT")
        self.expect_rendered(picked, key=last_match, text="cat")
        self.assertFalse(visible(picked, key=embedded))
        self.expect_rendered(picked, key="sql-add-cursor", within="sql-find-bar")
        self.assertAlmostEqual(self._settled_rect(picked, select_all)[1],
                               self._settled_rect(picked, "sql-find-prev")[1], delta=.2)
        self.assertEqual(current_draft(picked), sql)

        case_selected = [*all_matches, tap(9.3, "sql-find-match-case"), tap(9.8, select_all)]
        strict = self.replay(case_selected, until=10.5)
        self.expect_rendered(strict, key=first_match, text="cat")
        self.expect_rendered(strict, key=last_match, text="cat")
        self.assertFalse(visible(strict, key=upper_match))
        self.assertFalse(visible(strict, key=embedded))
        self.assertEqual(current_draft(strict), sql)

        edited = [*case_selected, key(10.65, "Escape"),
                  {"t": 11.15, "type": "key", "name": "z", "code": "z"}]
        changed = self.replay(edited, until=11.8)
        self.assertEqual(current_draft(changed), sql.replace("'cat'", "'z'"))
        self.assertFalse(visible(changed, key="sql-find-bar"))
        undone = self.replay([*edited, key(11.95, "z", cmd=True)], until=12.6)
        self.assertEqual(current_draft(undone), sql)
        self.expect_rendered(undone, key=first_match, text="cat")
        self.expect_rendered(undone, key=last_match, text="cat")
        for tree in (picked, strict, changed, undone):
            self.assertFalse(visible(tree, key="result-workspace"))
        self.assertEqual(self.db.read_bytes(), before)
        self.assertEqual(self.sql_customer_rows(), rows)
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
