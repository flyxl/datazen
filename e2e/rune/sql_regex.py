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
from sql_formatter import draft, editor_rows, row_text_nodes, selection


class RegexJourneys:
    def route(self, sql, pattern, *, top=False, light=False, before_find=()):
        fixture(self.store, self.db)
        store = json.loads(self.store.read_text(encoding="utf-8"))
        store["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(store), encoding="utf-8")
        settings = self.root / "settings.json"
        ui = {"language": "en", **({"themeMode": "light"} if light else {})}
        settings.write_text(json.dumps({"ui": ui}), encoding="utf-8")
        initial = self.db.read_bytes(), settings.read_bytes()
        enter = query_steps(sql)[:-1]
        enter[-1] = {"t": 4.8, "type": "paste", "text": sql}
        if top:
            enter.append(key(5.65, "Home", cmd=True))
        # fill() uses the physical typing route (one key event every 27ms).
        # Do not toggle the mode until a longer pattern has landed.
        toggle = max(8.2, 7.75 + .027 * len(pattern))
        route = [*enter, *before_find, tap(6.2, "sql-find-open"), *fill(7.0, "sql-find-input", pattern),
                 tap(toggle, "sql-find-regex")]
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

    @staticmethod
    def marks(tree):
        return [(n["key"], n.get("text"), n.get("fill")) for n in tree["elements"]
                if n.get("role") == "mark" and (n.get("key") or "").startswith("sql-find-hit-")
                and not n.get("exiting") and n.get("opacity", 0) > .95]

    @staticmethod
    def hit_key(sql, token, start=0):
        at = sql.index(token, start)
        first = len(sql[:at].encode("utf-8"))
        return f"sql-find-hit-{first}-{first + len(token.encode('utf-8'))}-{first}"

    def test_passive_regex_marks_use_utf8_keys_and_only_exact_selection_is_current(self):
        sql = "-- 猫 cat\n-- 😀 cat\nSELECT '(';"
        # Select just the final 't' of the first hit using the real canvas.
        # A passive mark spans that selection boundary but is not "current".
        partial = [key(5.15, "Home", cmd=True), key(5.5, "End"),
                   {**key(5.88, "Left"), "shift": True}]
        initial, route = self.route(sql, "(cat)", light=True, before_find=partial)
        ordinary, current = "#ffff0054", "#ff6a0054"
        first = self.hit_key(sql, "cat")
        first_byte = len(sql[:sql.index("cat")].encode("utf-8"))
        split = f"sql-find-hit-{first_byte}-{first_byte + 3}-{first_byte + 2}"
        second = self.hit_key(sql, "cat", sql.index("cat") + 3)
        literal = self.hit_key(sql, "(")

        passive = self.replay(route, until=9.45)
        self.assertEqual(self.marks(passive),
                         [(first, "ca", ordinary), (split, "t", ordinary), (second, "cat", ordinary)])
        self.assertEqual(selection(passive), [(0, "t")])

        step = [*route, tap(9.55, "sql-find-next")]
        selected = self.replay(step, until=10.65)
        self.assertEqual(selection(selected), [(1, "cat")])
        self.assertEqual(self.marks(selected), [(first, "cat", ordinary), (second, "cat", current)])

        malformed = [*step, *fill(10.85, "sql-find-input", "(")]
        stale = self.replay(malformed, until=11.95)
        self.assertEqual(self.marks(stale), [], "invalid regex must not reuse earlier marks or fall back")
        self.assertEqual(selection(stale), [(1, "cat")])

        # If the checkbox was merely hidden, disabled, or left on, this real
        # literal '(' in the draft could not take over from the stale regex.
        toggled = self.replay([*malformed, tap(12.15, "sql-find-regex")], until=13.1)
        self.assertEqual(self.marks(toggled), [(literal, "(", ordinary)])
        self.assertEqual(selection(toggled), [(1, "cat")])
        for tree in (passive, selected, stale, toggled):
            self.assertEqual(draft(tree), sql)
        self.untouched(initial, passive, selected, stale, toggled)

    def test_passive_regex_requeries_physical_scroll_without_evicting_next(self):
        head = "SELECT '猫 Needle0' AS first_tag;\n"
        gap = "BEGIN\n" + "".join(f"-- gap line {i:02}\n" for i in range(90)) + "END\n"
        sql = head + gap + "SELECT '😀 Needle9' AS last_tag;"
        pattern = r"Needle[09]|BEGIN[\s\S]*?END"
        initial, route = self.route(sql, pattern, top=True, light=True)
        first = self.hit_key(sql, "Needle0")
        last = self.hit_key(sql, "Needle9")
        ordinary = "#ffff0054"
        # Prove stale global results cannot masquerade as a viewport refresh:
        # even at the final visible END, the global-only match beginning at
        # the distant BEGIN is outside the real +/-250-unit search window.
        begin = len(head.encode("utf-8"))
        end = len((head + gap[:-1]).encode("utf-8"))
        global_only_tail = f"sql-find-hit-{begin}-{end}-{end - 3}"
        self.assertGreater(len(gap.encode("utf-16-le")) // 2, 2 * 250)

        # Keep physical focus in the real Find textbox before wheeling the
        # canvas. No query, document, selection, or panel input follows it.
        ready = [*route, tap(9.25, "sql-find-input")]
        before = self.replay(ready, until=10.15, size="960x640")
        self.assertEqual(self.marks(before), [(first, "Needle0", ordinary)])
        self.expect_rendered(before, key=first, text="Needle0")

        wheel = {"t": 10.35, "type": "wheel", "key": "sql-editor-scroll", "dy": -4800}
        after = self.replay([*ready, wheel], until=11.75, size="960x640")
        self.assertEqual(self.marks(after), [(last, "Needle9", ordinary)])
        self.expect_rendered(after, key=last, text="Needle9")
        self.expect_rendered(after, text="END", within=f"sql-line-{len(gap.splitlines())}")
        self.assertFalse(any(n.get("key") == global_only_tail for n in after["elements"]))
        self._expect_focused_find_textbox(after, pattern)
        self.assertFalse(any((n.get("key") or "").startswith("sql-selected-")
                             for n in after["elements"]))

        queue_next = {**tap(wheel["t"], "sql-find-next"), "hold": 0}
        raced = self.replay([*ready, queue_next, wheel], until=11.75, size="960x640")
        # These equal-time inputs are dispatched before the same Rune frame.
        # Next claims the active command slot just as physical scroll changes
        # the passive viewport. The worker's first active result must survive.
        self.assertEqual(selection(raced), [(0, "Needle0")])
        self.untouched(initial, before, after, raced)

    def test_mixed_crlf_original_offsets_replace_and_undo(self):
        sql = "猫\r\nleft\r\nright\n😀 cat!"
        initial, route = self.route(sql, r"(left)\n(right)")
        def row_bytes(tree):
            return [b"".join(n["text"].encode("utf-8") for n in row_text_nodes(tree, path))
                    for _, path in editor_rows(tree)]

        # Native splits only on LF, so Rune's actual row text still contains
        # the preceding CR. Join exact UTF-8 text, not rendered whitespace or
        # Python splitlines(), which would lose that witness.
        original = self.replay(route, until=8.9)
        expected_rows = [row.encode("utf-8") for row in sql.split("\n")]
        self.assertEqual(row_bytes(original), expected_rows)

        cmd = [*route, *fill(9.0, "sql-replace-input", "$2/$1"), tap(10.2, "sql-replace-all")]
        changed = self.replay(cmd, until=11.7)
        self.assertEqual(row_bytes(changed), ["猫\r".encode(), b"right/left", "😀 cat!".encode()])
        self.assertEqual(draft(changed), "猫\r\nright/left\n😀 cat!")
        restored = self.replay([*cmd, tap(12.1, "sql-find-close"), key(12.75, "z", cmd=True)], until=13.6)
        self.assertEqual(row_bytes(restored), expected_rows)
        self.assertEqual(draft(restored), sql)
        self.untouched(initial, original, changed, restored)

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
    available = unittest.defaultTestLoader.getTestCaseNames(RegexJourneys)
    parser.add_argument("--case", action="append", choices=available,
                        help="Run one exact replay case (repeatable; default: entire opt-in suite).")
    args = parser.parse_args()
    native.BINARY = args.binary.resolve(strict=True)
    native.ARTIFACTS = args.artifacts
    if native.ARTIFACTS:
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    class RegexAcceptance(RegexJourneys, native.NativeJourneys):
        pass
    suite = unittest.TestSuite(RegexAcceptance(n) for n in (args.case or available))
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
