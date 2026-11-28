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


class SqlFormatterLimitJourney:
    def test_native_formatter_worker_bound_and_short_selection_recovery(self):
        baseline = self._formatter_fixture({"keymapPreset": "default"})
        # Unlike a lone comment, these are two valid SQLite statements.
        # Keep lines reasonably short for the actual renderer; exact input
        # sizing is ASCII bytes, after the formatter command's trim/LF step.
        prefix = "select  1;\n/* sized note\n"
        suffix = "\n*/\nselect  2;"

        def make(size):
            filler = size - len(prefix) - len(suffix)
            rows, tail = divmod(filler - 1, 511)
            comment = ("x" * 510 + "\n") * rows + "x" * (tail + 1)
            sql = prefix + comment + suffix
            self.assertEqual(len(sql.encode("utf-8")), size)
            return sql, comment

        limit = 256 * 1024
        below, comment = make(limit)
        above, _ = make(limit + 1)

        def enter(sql):
            return [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": sql},
                    tap(6.3, "sql-format")]

        # The exact long-comment transform was checked against the pinned
        # React/sql-formatter 15.8.2 command. The local Rust provider must
        # accept at its published limit; never relax the assertion because
        # rendering/layout/worker startup is slow on a particular machine.
        small = self.replay(enter(below), until=9.55, timeout=150)
        self.assertEqual(draft(small), "SELECT\n  1;\n\n/* sized note\n" + comment
                         + "\n*/\nSELECT\n  2;")
        refused = self.replay(enter(above), until=9.55, timeout=150)
        self.assertEqual(draft(refused), above, "over-limit Format must leave the full draft exact")
        self.expect_rendered(refused, text="This SQL is too large to format.", within="query-notice")
        self.assertEqual(selection(refused), [])
        undone = self.replay([*enter(above), key(9.53, "z", cmd=True)], until=10.23, timeout=150)
        self.assertEqual(draft(undone), "", "one Undo removes the initial paste, not a phantom Format edit")

        # Retry a small selected statement without changing the rest of the
        # above-limit draft or reopening this subprocess's own Query tab.
        # If the first whole request had changed the draft, the other
        # statement/comment layout would no longer match this exact result.
        recovered = self.replay([*enter(above), key(9.45, "Home", cmd=True),
                                 {**key(9.83, "End"), "shift": True},
                                 formatted_chord(10.27, "f")], until=13.6, timeout=150)
        expected = "SELECT\n  1;" + above[len("select  1;"):]
        self.assertEqual(draft(recovered), expected)
        self.assertEqual(selection(recovered), [(0, "SELECT"), (1, "  1;")])
        self.assertFalse(visible(recovered, key="query-notice"), "successful recovery clears the limit notice")
        self._no_query_or_settings_write((small, refused, undone, recovered), baseline)


class SqlFormatterDmlJourney:
    def test_native_formatter_dml_and_protected_text_never_runs_sql(self):
        baseline = self._formatter_fixture({})
        case = json.loads(Path(__file__).with_name("sql-formatter-dml-15.8.2.json")
                          .read_text(encoding="utf-8"))
        self.assertEqual(case["formatter"], "15.8.2")
        self.assertEqual(len(case["scenarios"]), 6)
        route = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": case["input"]},
                 tap(6.3, "sql-format")]
        formatted = self.replay(route, until=8.25)
        self.assertEqual(draft(formatted), case["expected"])
        self.assertIn("  SELECT   a,   b FROM t;   ", draft(formatted))
        restored = self.replay([*route, key(8.43, "z", cmd=True)], until=9.22)
        self.assertEqual(draft(restored), case["input"])
        self._no_query_or_settings_write((formatted, restored), baseline)


class SqlFormatterDdlJourney:
    def test_native_formatter_definitions_explain_and_savepoints_never_execute(self):
        baseline = self._formatter_fixture({})
        case = json.loads(Path(__file__).with_name("sql-formatter-ddl-15.8.2.json")
                          .read_text(encoding="utf-8"))
        self.assertEqual(case["formatter"], "15.8.2")
        self.assertEqual(len(case["scenarios"]), 7)
        route = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": case["input"]},
                 tap(6.3, "sql-format")]
        formatted = self.replay(route, until=8.3)
        self.assertEqual(draft(formatted), case["expected"])
        undone = self.replay([*route, key(8.45, "z", cmd=True)], until=9.25)
        self.assertEqual(draft(undone), case["input"])
        self._no_query_or_settings_write((formatted, undone), baseline)


def main():
    import run as native
    from sql_formatter_selections import SelectionRepairJourney

    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--size-bound", action="store_true",
                        help="run only the separate real 256 KiB pure Rust input and selected recovery check")
    parser.add_argument("--statements", action="store_true",
                        help="run only the multi-statement DML/PRAGMA/disabled region gate")
    parser.add_argument("--definitions", action="store_true",
                        help="run only the multi-statement DDL/EXPLAIN/savepoint gate")
    parser.add_argument("--selection-repair", action="store_true", help="range-local refusal and recovery")
    parser.add_argument("--dialect", choices=("sqlite", "postgresql", "mysql", "mariadb"),
                        help="run the independent dialect suite for an actual connected engine")
    parser.add_argument("--fixture", type=Path, help="marked disposable TCP configuration for --dialect")
    parser.add_argument("--case", help="named case for the chosen dialect suite")
    args = parser.parse_args()
    if args.dialect:
        if any((args.size_bound, args.statements, args.definitions, args.selection_repair)):
            parser.error("choose one specialized acceptance group at a time")
        import sql_formatter_dialects
        argv = [str(args.binary), args.dialect]
        for flag, value in (("--fixture", args.fixture), ("--case", args.case), ("--artifacts", args.artifacts)):
            if value is not None:
                argv.extend([flag, str(value)])
        return sql_formatter_dialects.main(argv)
    if args.fixture or args.case:
        parser.error("--fixture and --case require --dialect")
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if native.ARTIFACTS:
        native.ARTIFACTS = native.ARTIFACTS.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)

    class FormatAcceptance(SelectionRepairJourney, SqlFormatterLimitJourney, SqlFormatterDmlJourney, SqlFormatterDdlJourney, SqlFormatterJourneys, native.NativeJourneys):
        pass

    if sum((args.size_bound, args.statements, args.definitions, args.selection_repair)) > 1:
        parser.error("choose one specialized acceptance group at a time")
    group = (SelectionRepairJourney if args.selection_repair else SqlFormatterLimitJourney if args.size_bound else
             SqlFormatterDmlJourney if args.statements else
             SqlFormatterDdlJourney if args.definitions else SqlFormatterJourneys)
    suite = unittest.TestSuite(FormatAcceptance(name)
                               for name in unittest.defaultTestLoader.getTestCaseNames(group))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
