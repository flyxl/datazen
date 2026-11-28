#!/usr/bin/env python3
"""Optional Light-only Native Prompt draft resizing journey."""
import argparse
import json
import unittest
from pathlib import Path

import run as native
from offline import key
from scenarios import tap, visible
from sql_formatter import node


class PromptResizeJourney(native.NativeJourneys):
    def test_prompt_template_resizes_in_light_without_saving_or_losing_focus(self):
        prefs = self.root / "settings.json"
        prefs.write_text(json.dumps({"ui": {"language": "en", "themeMode": "light"}}), encoding="utf-8")
        prefs_before = prefs.read_bytes()
        overrides = self.root / "prompt_overrides.json"
        card = "prompt-card-nl2sql"
        field = "prompt-edit-system"
        viewport = "prompt-edit-system-scroll"
        grip = "prompt-edit-system-resize"
        path = [tap(.55, "rail-settings"), tap(1.45, "settings-nav-prompts"), tap(3, card + "-edit")]

        initial = self.replay(path, until=3.8)
        x, y, width, height = self._settled_rect(initial, viewport)
        self.assertAlmostEqual(height, 146, delta=.1)
        original = node(initial, field)["text"]
        self.expect_rendered(initial, key=grip)
        edit = {"t": 4, "type": "tap", "x": x + 30, "y": y + 30}
        upsized = [*path, edit, {"t": 4.5, "type": "drag", "key": grip, "dy": 96, "dur": .45}]
        enlarged = self.replay(upsized, until=5.4)
        self.assertEqual(self._settled_rect(enlarged, viewport), [x, y, width, 242])
        self.assertEqual(node(enlarged, field)["text"], original)
        self.assertTrue(node(enlarged, field)["focused"])
        save_y = self._settled_rect(initial, "prompt-edit-save")[1]
        self.assertAlmostEqual(self._settled_rect(enlarged, "prompt-edit-save")[1], save_y + 96, delta=.1)
        self.assertFalse(overrides.exists(), "drag does not create an override")

        # An actual focused draft edit is still delivered after the drag.
        # Shrinking the viewport must not truncate or save it; Cancel and
        # re-open must restore the default height *and* original text.
        custom = "Light fixture, first line.\nSecond line: 模板."
        downsized = [*upsized, key(5.65, "a", cmd=True),
                     {"t": 5.9, "type": "paste", "text": custom},
                     {"t": 6.45, "type": "drag", "key": grip, "dy": -190, "dur": .45}]
        smaller = self.replay(downsized, until=7.35)
        self.assertEqual(self._settled_rect(smaller, viewport), [x, y, width, 120])
        self.assertEqual(node(smaller, field)["text"], custom)
        self.assertTrue(node(smaller, field)["focused"])
        self.assertFalse(overrides.exists())
        cancelled = self.replay([*downsized, tap(7.55, "prompt-edit-cancel")], until=8.15)
        self.assertFalse(visible(cancelled, key=grip))
        self.expect_rendered(cancelled, key=card + "-preview")
        restored = self.replay([*downsized, tap(7.55, "prompt-edit-cancel"), tap(8.35, card + "-edit")],
                               until=9.15)
        self.assertEqual(self._settled_rect(restored, viewport), [x, y, width, 146])
        self.assertEqual(node(restored, field)["text"], original)
        self.assertFalse(overrides.exists())
        self.assertEqual(prefs.read_bytes(), prefs_before)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", nargs="?", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--list-tests", action="store_true")
    args = parser.parse_args()
    name = "test_prompt_template_resizes_in_light_without_saving_or_losing_focus"
    if args.list_tests:
        print(name)
        return 0
    if not args.binary:
        parser.error("a Native executable is required")
    native.BINARY = args.binary.resolve(strict=True)
    if args.artifacts:
        native.ARTIFACTS = args.artifacts.resolve()
        native.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    suite = unittest.TestSuite([PromptResizeJourney(name)])
    return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
