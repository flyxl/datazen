#!/usr/bin/env python3
"""Opt-in replay of the real Light Editor SQL Syntax Theme menu."""
import argparse
import json
import unittest
from pathlib import Path

import run as native
from offline import SETTINGS, key
from scenarios import tap, visible


TRIGGER = "settings-sql-syntax-theme-trigger"
MENU = "settings-sql-syntax-theme-content"


class SyntaxMenuJourney(native.NativeJourneys):
    def test_light_editor_syntax_menu_tracks_its_field_and_return_reopens_without_saving(self):
        prefs = self.root / "settings.json"
        preset = {"ui": {"themeMode": "light", "language": "en"},
                  "sqlSyntaxTheme": "default", "unrelated": {"keep": 1}}
        prefs.write_text(json.dumps(preset, indent=2), encoding="utf-8")
        before = prefs.read_bytes()
        route = [SETTINGS, tap(1.4, "settings-nav-editor"), tap(2.4, TRIGGER)]

        def require_menu(scene):
            x, y, width, height = self._settled_rect(scene, TRIGGER)
            mx, my, menu_width, menu_height = self._settled_rect(scene, MENU)
            self.assertAlmostEqual(mx, x, delta=.25)
            self.assertAlmostEqual(my - (y + height), 4.0, delta=.25)
            self.assertAlmostEqual(menu_width, width, delta=.25)
            self.assertTrue(0 < menu_height <= 240.0)
            self.assertLessEqual(my + menu_height, scene["viewport"][1])
            self.expect_rendered(scene, text="Default (DataZen)", within="option0")
            self.expect_rendered(scene, text="Dracula", within="option5")
            self.assertEqual(prefs.read_bytes(), before)

        require_menu(self.replay(route, until=3.1))
        dismiss = [*route, key(3.25, "Escape")]
        dismissed = self.replay(dismiss, until=3.95, size="960x640")
        self.assertFalse(visible(dismissed, key=MENU))
        self.expect_rendered(dismissed, text="Default (DataZen)", within=TRIGGER)
        self.assertTrue(any(e.get("key") == TRIGGER and e.get("focused") and not e.get("exiting")
                            for e in dismissed["elements"]))
        self.assertEqual(prefs.read_bytes(), before)

        keyboard = [*dismiss, key(4.1, "Enter")]
        require_menu(self.replay(keyboard, until=4.8, size="960x640"))
        chosen = self.replay([*keyboard, tap(5.0, "option5")], until=5.85, size="960x640")
        self.assertFalse(visible(chosen, key=MENU))
        self.expect_rendered(chosen, text="Dracula", within=TRIGGER)
        self.assertEqual(json.loads(prefs.read_bytes()), {**preset, "sqlSyntaxTheme": "dracula"})
        [preview] = [e for e in chosen["elements"]
                     if e.get("key") == "settings-sql-syntax-preview" and not e.get("exiting")]
        self.assertEqual(preview["fill"], "#f8f8f2ff")
        self.assertFalse(self.db.exists(), "theme changes are separate from database activity")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--list-tests", action="store_true")
    args = parser.parse_args()
    name = "test_light_editor_syntax_menu_tracks_its_field_and_return_reopens_without_saving"
    if args.list_tests:
        print(name)
        return 0
    if not args.binary:
        parser.error("a Native executable is required")
    native.BINARY = args.binary.resolve(strict=True)
    if args.artifacts:
        native.ARTIFACTS = args.artifacts.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    return 0 if unittest.TextTestRunner(verbosity=2).run(
        unittest.TestSuite([SyntaxMenuJourney(name)])
    ).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
