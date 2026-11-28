#!/usr/bin/env python3
"""Opt-in Linux Native Query regexp gate, never implicit in the off-feature sweep.

Pass a real datazen-native built with --features native-query-regex. A missing
checkbox, sandbox, or match is a failure, not a skipped test.
"""
import argparse
import json
from pathlib import Path
import unittest

from offline import fill, key
from scenarios import fixture, query_steps, tap, visible
from sql_formatter import draft, selection


class RegexJourneys:
    def route(self, sql, pattern):
        fixture(self.store, self.db)
        store = json.loads(self.store.read_text(encoding="utf-8"))
        store["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(store), encoding="utf-8")
        settings = self.root / "settings.json"
        settings.write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        initial = self.db.read_bytes(), settings.read_bytes()
        enter = query_steps(sql)[:-1]
        enter[-1] = {"t": 4.8, "type": "paste", "text": sql}
        route = [*enter, tap(6.2, "sql-find-open"), *fill(7.0, "sql-find-input", pattern),
                 tap(8.2, "sql-find-regex")]
        return initial, route

    def untouched(self, initial, *scenes):
        for tree in scenes:
            self.assertFalse(visible(tree, key="result-workspace"))
        self.assertEqual((self.db.read_bytes(), (self.root/"settings.json").read_bytes()), initial)
        history = self.root/"query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))

    def test_capture_replace_all_undo(self):
        sql = "SELECT '猫-A1', '猫-B2', 'A1';"
        initial, route = self.route(sql, r"(?<=猫-)([AB])(\d)")
        find = [*route, tap(9.0, "sql-find-next")]
        found = self.replay(find, until=10.3)
        self.expect_rendered(found, key="sql-find-regex", within="sql-find-bar")
        self.assertEqual(selection(found), [(0,"A1")])
        self.assertEqual(draft(found), sql)
        edit = [*find, *fill(10.6, "sql-replace-input", "$2:$1"), tap(11.8,"sql-replace-all")]
        changed = self.replay(edit, until=13.0)
        self.assertEqual(draft(changed), "SELECT '猫-1:A', '猫-2:B', 'A1';")
        restored = self.replay([*edit, tap(13.1, "sql-find-close"), key(13.75, "z", cmd=True)], until=14.6)
        self.assertEqual(draft(restored), sql)
        self.assertEqual(selection(restored), [(0,"A1")])
        self.untouched(initial, found, changed, restored)

    def test_invalid_pattern_does_not_fall_back_to_literal(self):
        sql = "SELECT '(';"
        initial, route = self.route(sql,"(")
        edit = [*route, *fill(9.0,"sql-replace-input","bad"), tap(10.25, "sql-replace-all")]
        refused = self.replay(edit, until=11.7)
        self.assertEqual(draft(refused),sql)
        self.expect_rendered(refused,key="sql-find-regex-status",text="Invalid regular expression.")
        fixed = self.replay([*edit, *fill(12.0, "sql-find-input", r"\("),
                             key(13.25,"Enter")], until=14.3)
        self.assertEqual(selection(fixed),[(0,"(")])
        self.assertEqual(draft(fixed),sql)
        self.untouched(initial, refused, fixed)

    def test_same_frame_canvas_and_tab_change_retire_replace_all(self):
        sql = "SELECT 'cat';"
        initial, route = self.route(sql, "(cat)")
        pending = [*route, *fill(9.0, "sql-replace-input", "dog"),
                   {**tap(10.5, "sql-replace-all"), "hold": 0}]
        # The pinned Rune script runner dispatches every equal-time input in
        # stable order before rt.frame() polls the real app's worker. An
        # immediate second click races delivery deterministically, regardless
        # of whether the process has already put its answer into the mailbox.
        committed = self.replay(pending, until=11.7)
        self.assertEqual(draft(committed), "SELECT 'dog';", "this exact press must submit a real edit")
        clicked = self.replay([*pending, {**tap(10.5, "sql-editor-scroll"), "hold": 0}], until=11.7)
        self.assertEqual(draft(clicked), sql)
        self.assertTrue(next(e for e in clicked["elements"] if e.get("key") == "sql-editor-scroll")["focused"])
        tabs = [e for e in clicked["elements"] if e.get("role") == "tab"
                and (e.get("key") or "").startswith("tab-") and not e.get("exiting")]
        self.assertEqual(len(tabs), 1)
        x, y, w, h = tabs[0]["rect"]
        returned = self.replay([*pending, {**tap(10.5, "conn-toolbar-new-query"), "hold": 0},
                                tap(11.45, "sql-editor-scroll"),
                                {"t": 11.8, "type": "paste", "text": "distinct second draft"},
                                {"t": 12.65, "type": "tap", "x": x + min(w / 4, 8), "y": y + h / 2}],
                               until=13.7)
        new_tabs = [e for e in returned["elements"] if e.get("role") == "tab"
                    and (e.get("key") or "").startswith("tab-") and not e.get("exiting")]
        self.assertEqual(len(new_tabs), 2)
        self.assertEqual(draft(returned), sql)
        self.untouched(initial, committed, clicked, returned)


def main():
    import run as native
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary",type=Path)
    parser.add_argument("--artifacts",type=Path)
    args = parser.parse_args()
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if native.ARTIFACTS:
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    class RegexAcceptance(RegexJourneys, native.NativeJourneys):
        pass
    suite = unittest.TestSuite(RegexAcceptance(n) for n in unittest.defaultTestLoader.getTestCaseNames(RegexJourneys))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
