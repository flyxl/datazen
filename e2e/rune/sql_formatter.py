#!/usr/bin/env python3
"""Strict opt-in Format acceptance for a Native executable with the local engine.

Reuses the normal Native SQLite replay harness, but not its default sweep:
packages without the optional formatter should not run this gate. A missing
or unavailable provider on the executable supplied to this gate is a failure.
"""
import argparse
import json
import re
import unittest
from pathlib import Path

from offline import key
from scenarios import fixture, query_steps, tap, visible


def node(tree, stable_key):
    matches = [row for row in tree["elements"]
               if row.get("key") == stable_key and not row.get("exiting")]
    if len(matches) != 1:
        raise AssertionError(f"expected one live {stable_key}, got {len(matches)}")
    return matches[0]


def editor_rows(tree):
    root = node(tree, "sql-editor-field")["path"] + "/"
    rows = []
    for element in tree["elements"]:
        path = element["path"]
        if not path.startswith(root) or element.get("exiting"):
            continue
        leaf = path.removeprefix(root)
        found = re.fullmatch(r"sql-line-(\d+)", leaf)
        if found:
            rows.append((int(found[1]), path))
    rows.sort()
    if not rows or [index for index, _ in rows] != list(range(len(rows))):
        raise AssertionError("missing or nonconsecutive Native Query document rows")
    return rows


def row_text_nodes(tree, path):
    # Use the real text fragments emitted by Rune document rows, including
    # whitespace and selected fragments. Exclude grammar/Find paint marks,
    # editor carets, and old rows retained during a transition.
    return [item for item in tree["elements"]
            if item["path"].rsplit("/", 1)[0] == path and item.get("role") == "text"
            and item.get("text") is not None and not item.get("exiting")]


def draft(tree):
    return "\n".join("".join(item["text"] for item in row_text_nodes(tree, path))
                     for _, path in editor_rows(tree))


def selection(tree):
    return [(index, "".join(leaf["text"] for leaf in leaves))
            for index, path in editor_rows(tree)
            if (leaves := [leaf for leaf in row_text_nodes(tree, path)
                          if (leaf.get("key") or "").startswith("sql-selected-")])]


def formatted_chord(t, name):
    return {**key(t, name, cmd=True), "code": f"Key{name.upper()}", "base": name, "shift": True}


class SqlFormatterJourneys:
    def _formatter_fixture(self, prefs):
        fixture(self.store, self.db)
        store = json.loads(self.store.read_text(encoding="utf-8"))
        store["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(store), encoding="utf-8")
        self.store.chmod(0o600)
        self.addCleanup(self.db.chmod, 0o600)
        self.db.chmod(0o400)
        path = self.root / "settings.json"
        path.write_text(json.dumps({"ui": {"language": "en"}, **prefs}), encoding="utf-8")
        before_settings = path.read_bytes()
        before_db = self.db.read_bytes()
        return path, before_settings, before_db

    def _no_query_or_settings_write(self, scenes, baseline):
        settings, before_settings, before_db = baseline
        for tree in scenes:
            self.assertFalse(visible(tree, key="result-workspace"), "Format must not run SQL")
        history = self.root / "query-history.json"
        self.assertFalse(history.exists() and json.loads(history.read_text(encoding="utf-8")))
        self.assertEqual(self.db.read_bytes(), before_db)
        self.assertEqual(settings.read_bytes(), before_settings)

    def test_native_formatter_toolbar_keeps_reversed_selected_block_and_one_undo(self):
        baseline = self._formatter_fixture({"sqlFormatOptions": {
            "keywordCase": "lower", "indentStyle": "4spaces",
            "breakBeforeBooleanOperators": False, "linesBetweenQueries": 2,
        }})
        # Exact selected output from the installed React sql-formatter 15.8.2
        # command oracle, not an approximate SQL pretty-print expectation.
        indented = "select 'q;z', lower('M') where true and false;"
        original = f"select 1;\n    {indented}\nselect 3;"
        after_lines = ["select", "        'q;z',", "        lower('M')", "    where",
                       "        true and", "        false;"]
        expected = "\n".join(["select 1;", "    " + after_lines[0], *after_lines[1:], "select 3;"])
        route = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": original},
                 key(5.55, "Home"), key(5.76, "Up"), key(5.97, "End"),
                 {**key(6.18, "Home"), "shift": True},
                 *({**key(6.40 + i * .15, "Right"), "shift": True} for i in range(4)),
                 tap(7.22, "sql-format")]
        formatted = self.replay(route, until=8.82)
        self.assertEqual(draft(formatted), expected)
        self.assertEqual(selection(formatted), list(enumerate(after_lines, start=1)))
        caret = node(formatted, "sql-caret")
        last = node(formatted, "sql-selected-6-0")
        self.assertTrue(caret["path"].startswith(node(formatted, "sql-line-6")["path"] + "/"))
        self.assertGreaterEqual(caret["rect"][0] + .2, last["rect"][0] + last["rect"][2])
        editor = node(formatted, "sql-editor-scroll")
        self.assertTrue(editor.get("focused"), "toolbar returns to its real editor after invocation")
        self.assertGreaterEqual(caret["rect"][1], editor["rect"][1] - .2)
        self.assertLessEqual(caret["rect"][1] + caret["rect"][3], editor["rect"][1] + editor["rect"][3] + .2)

        undone = self.replay([*route, key(8.94, "z", cmd=True)], until=9.57)
        self.assertEqual(draft(undone), original, "one Undo restores precisely the input SQL")
        self.assertEqual(selection(undone), [(1, indented)])
        caret = node(undone, "sql-caret")
        restored = node(undone, "sql-selected-1-4")
        self.assertTrue(caret["path"].startswith(node(undone, "sql-line-1")["path"] + "/"))
        self.assertLessEqual(caret["rect"][0] + caret["rect"][2], restored["rect"][0] + .2,
                             "the original backward selection's caret is again at its start")
        self._no_query_or_settings_write((formatted, undone), baseline)

    def test_native_formatter_custom_chord_respects_find_scope_and_statement_anchor(self):
        baseline = self._formatter_fixture({
            "keymapPreset": "dbeaver", "customKeymap": {"formatSql": "Mod-Shift-k"},
        })
        original = "select 'a;b' as glyph; -- ; comment\nselect 2 as n; --keep;"
        expected = "SELECT\n  'a;b' AS glyph;\n\n-- ; comment\nSELECT\n  2 AS n;\n\n--keep;"
        route = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": original},
                 key(5.55, "Home"),
                 *(key(5.79 + i * .11, "Right") for i in range(8)),
                 formatted_chord(6.88, "f"),  # persisted custom replaces the preset
                 key(7.56, "f", cmd=True),
                 formatted_chord(8.16, "k")]
        blocked = self.replay(route, until=9.36)
        self.assertEqual(draft(blocked), original)
        input_path = node(blocked, "sql-find-input")["path"] + "/"
        self.assertTrue(any(item.get("focused") and item.get("role") == "textbox"
                            and item["path"].startswith(input_path)
                            for item in blocked["elements"]), "the real Find textbox owns its key")

        formatted = self.replay([*route, key(8.74, "Escape"), formatted_chord(9.18, "k")], until=10.77)
        self.assertEqual(draft(formatted), expected)
        self.assertEqual(selection(formatted), [], "whole document Format leaves a collapsed caret")
        self.assertFalse(visible(formatted, key="sql-find-bar"))
        caret = node(formatted, "sql-caret")
        # The original statement index leads to the preceding inline-comment
        # content after reflow. This exact position is from the pinned oracle.
        anchored = node(formatted, "sql-line-3")
        self.assertTrue(caret["path"].startswith(anchored["path"] + "/"))
        self.assertAlmostEqual(caret["rect"][0], anchored["rect"][0], delta=.3)
        self._no_query_or_settings_write((blocked, formatted), baseline)


def main():
    import run as native

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if native.ARTIFACTS:
        native.ARTIFACTS = native.ARTIFACTS.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)

    class FormatAcceptance(SqlFormatterJourneys, native.NativeJourneys):
        pass

    suite = unittest.TestSuite(FormatAcceptance(name)
                               for name in unittest.defaultTestLoader.getTestCaseNames(SqlFormatterJourneys))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
