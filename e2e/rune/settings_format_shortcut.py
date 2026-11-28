#!/usr/bin/env python3
"""Opt-in Native Editor Settings → local Format SQL acceptance.

Run against a stock Native binary built with the local SQL formatter. The
ordinary Native sweep also supports binaries without this optional capability.
"""
import argparse
import json
import sys
import unittest
from pathlib import Path

import run as native
from offline import key
from scenarios import query_steps, tap, visible
from sql_formatter import SqlFormatterJourneys, draft, formatted_chord, node


FIELD = "settings-keymap-input-formatSql"
CHIP = "settings-keymap-kbd-formatSql"
EDITOR_SETTINGS = [tap(.5, "rail-settings"), tap(1.4, "settings-nav-editor")]
MACOS = sys.platform == "darwin"


def edit(value):
    steps = [*EDITOR_SETTINGS, tap(2.3, FIELD), key(2.55, "a", cmd=True)]
    if value:
        steps.append({"t": 2.8, "type": "paste", "text": value})
    else:
        steps.append(key(2.8, "Backspace"))
    return steps


def saved_chord(t):
    return {**key(t, "F8", cmd=True), "alt": True}


class FormatShortcutJourney(SqlFormatterJourneys, native.NativeJourneys):
    def test_settings_format_shortcut_persists_and_only_formats_the_focused_query(self):
        # The baseline custom action is different from both the new binding
        # and the preset. Preserve the execute override, a future action,
        # and a root sibling through the real settings repository.
        baseline = self._formatter_fixture({
            "keymapPreset": "dbeaver",
            "customKeymap": {"execute": "Mod-r", "formatSql": "Mod-Shift-k", "futureAction": "Alt-F7"},
            "unrelated": {"retained": [1, 2]},
        })
        settings, _, db_before = baseline
        original_settings = json.loads(settings.read_bytes())
        primary, option = ("Cmd", "Option") if MACOS else ("Ctrl", "Alt")
        old_display = f"{primary}+Shift+K"
        saved_display = f"{primary}+{option}+F8"

        changed = self.replay([*edit("Mod+Alt+F8"), key(3.2, "Enter")], until=4.45)
        self.expect_rendered(changed, text=saved_display, within=CHIP)
        self.expect_rendered(changed, key="input", text=saved_display, within=FIELD)
        self.assertFalse(visible(changed, key="settings-error"))
        expected = {
            **original_settings,
            "customKeymap": {**original_settings["customKeymap"], "formatSql": "Mod-Alt-F8"},
        }
        committed = settings.read_bytes()
        self.assertEqual(json.loads(committed), expected)

        # Each replay below starts a fresh Native subprocess. Neither an
        # invalid commit nor a valid draft canceled with Escape may write.
        invalid = self.replay([*edit("Mod+NotAKey"), key(3.2, "Enter")], until=4.45)
        self.expect_rendered(invalid, text=saved_display, within=CHIP)
        self.expect_rendered(invalid, key="input", text="Mod+NotAKey", within=FIELD)
        self.expect_rendered(invalid, text="Enter a valid shortcut (up to 64 bytes). The previous binding was kept.",
                             within="settings-error")
        self.assertEqual(settings.read_bytes(), committed)

        cancelled = self.replay([*edit(old_display), key(3.2, "Escape")], until=4.45)
        self.expect_rendered(cancelled, text=saved_display, within=CHIP)
        self.expect_rendered(cancelled, key="input", text=saved_display, within=FIELD)
        self.assertFalse(visible(cancelled, key="settings-error"))
        self.assertEqual(settings.read_bytes(), committed)

        # No Run action in these replays. Focus the editor in a fresh
        # read-only SQLite session, prove the old binding is retired, then
        # issue only the new chord and compare the entire rendered draft.
        sql = "select  1;"
        formatted_sql = "SELECT\n  1;"
        query = [*query_steps("")[:4], {"t": 4.8, "type": "paste", "text": sql}]
        old_chord = formatted_chord(5.5, "k")
        unchanged = self.replay([*query, old_chord], until=7.15)
        self.assertEqual(draft(unchanged), sql)
        self.assertTrue(node(unchanged, "sql-editor-scroll").get("focused"))
        formatted = self.replay([*query, old_chord, saved_chord(7.3)], until=9.1)
        self.assertEqual(draft(formatted), formatted_sql)
        self.assertTrue(node(formatted, "sql-editor-scroll").get("focused"))
        self._no_query_or_settings_write((unchanged, formatted), (settings, committed, db_before))

        # Clearing only this field inherits DBeaver's Format binding. The
        # execute override remains active/preserved; no other keys change.
        cleared = self.replay([*edit(""), key(3.2, "Enter")], until=4.45)
        self.expect_rendered(cleared, text="Ctrl+Shift+F", within=CHIP)
        self.assertFalse(visible(cleared, key="settings-error"))
        del expected["customKeymap"]["formatSql"]
        inherited_settings = settings.read_bytes()
        self.assertEqual(json.loads(inherited_settings), expected)
        retired = self.replay([*query, saved_chord(5.5)], until=7.15)
        self.assertEqual(draft(retired), sql)
        # DBeaver uses literal Ctrl, not platform Mod (Command on macOS).
        inherited_chord = {**key(7.3, "f", cmd=not MACOS), "ctrl": True, "shift": True,
                           "code": "KeyF", "base": "f"}
        inherited = self.replay([*query, saved_chord(5.5), inherited_chord], until=9.1)
        self.assertEqual(draft(inherited), formatted_sql)
        self._no_query_or_settings_write((retired, inherited), (settings, inherited_settings, db_before))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--list-tests", action="store_true")
    args = parser.parse_args()
    name = "test_settings_format_shortcut_persists_and_only_formats_the_focused_query"
    if args.list_tests:
        print(name)
        return 0
    if not args.binary:
        parser.error("a Native executable with the local SQL formatter is required")
    native.BINARY = args.binary.resolve(strict=True)
    if args.artifacts:
        native.ARTIFACTS = args.artifacts.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    suite = unittest.TestSuite([FormatShortcutJourney(name)])
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
