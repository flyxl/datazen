"""Run against a packaged Native host; no separate build, server or external requests."""
import argparse
import json
from pathlib import Path
import sys
import unittest

import run
from scenarios import fixture, tap


class ShellFooterJourneys(run.NativeJourneys):
    def test_connection_landing_panel_and_other_routes_own_their_footer(self):
        fixture(self.store, self.db)
        profile = json.loads(self.store.read_bytes())
        profile["connections"][0]["readOnly"] = True
        self.store.write_text(json.dumps(profile), encoding="utf8")
        original = self.db.read_bytes()
        (self.root / "settings.json").write_text(json.dumps({
            "ui": {"language": "en", "themeMode": "light"},
            "rowCountMode": "exact", "customKeymap": {"formatSql": "Mod-Alt-f"},
        }), encoding="utf8")

        def absent(tree, *keys):
            present = {n.get("key") for n in tree["elements"] if not n.get("exiting")}
            self.assertFalse(present.intersection(keys), present.intersection(keys))

        home = self.replay([], until=1.8)
        self.expect_rendered(home, key="home-mcp-promo-bar")
        self.expect_rendered(home, key="connections-shortcut-format-label", text="Format SQL")
        self.expect_rendered(home, key="connections-shortcut-format-kbd",
                             text="Cmd+Option+F" if sys.platform == "darwin" else "Ctrl+Alt+F")
        absent(home, "content-status-bar", "status-connection-dot", "status-shortcuts")
        x, y, w, h = self._settled_rect(home, "connections-shortcut-footer")
        mx, my, mw, mh = self._settled_rect(home, "home-mcp-promo-bar")
        chips = [n["rect"] for n in home["elements"] if n.get("key") in (
            "connections-shortcut-format", "connections-shortcut-save", "connections-shortcut-key")
            and not n.get("exiting")]
        self.assertTrue(chips)
        left, right = min(c[0] for c in chips), max(c[0] + c[2] for c in chips)
        self.assertGreaterEqual(y, my + mh)
        self.assertAlmostEqual((left + right) / 2, x + w / 2, delta=1)
        self.assertAlmostEqual(x + w / 2, mx + mw / 2, delta=1)

        opened = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "tree-table-customers")]
        def panel(tree):
            self.expect_rendered(tree, within="cell-0-1", text="fixture-ada")
            self.expect_rendered(tree, key="status-connection-label", text="Connected")
            context = next(n["text"] for n in tree["elements"]
                           if n.get("key") == "status-context" and not n.get("exiting"))
            for s in ["SQLite", "Rune SQLite E2E", "customers", "2 rows"]:
                self.assertIn(s, context)
            absent(tree, "connections-shortcut-footer")
            bar = self._settled_rect(tree, "content-status-bar")
            self.assertAlmostEqual(bar[3], 40, delta=.5)
            self.assertAlmostEqual(bar[1] + bar[3], tree["viewport"][1], delta=.5)
            label = self._settled_rect(tree, "status-connection-label")
            ctx = self._settled_rect(tree, "status-context")
            hint = self._settled_rect(tree, "status-shortcuts")
            self.assertLessEqual(label[0] + label[2], ctx[0] + 1)
            self.assertLessEqual(ctx[0] + ctx[2], hint[0] + 1)
        panel(self.replay(opened, until=5.1))
        path = list(opened)
        for i, (route, surface) in enumerate([
            ("rail-workflows", "workflow-workspace"), ("rail-dashboards", "dashboards-page"),
            ("rail-wapps", "extension-management-page"), ("rail-tools", "native-transfer"),
            ("rail-settings", "settings-page"),
        ]):
            at = 5.4 + 1.15 * i
            path.append(tap(at, route))
            snap = self.replay(path, until=at + .95)
            absent(snap, "content-status-bar", "datazen-navigator", "connections-shortcut-footer")
            sx, sy, sw, sh = self._settled_rect(snap, surface)
            self.assertAlmostEqual(sy + sh, snap["viewport"][1], delta=1)
            if route == "rail-tools":
                self.expect_rendered(snap, key="native-tool-data-sync")
                absent(snap, "content-view", "content-toolbar", "content-view-drawers")
        back = [*path, tap(11.7, "settings-back"), tap(12.9, "rail-workspace")]
        panel(self.replay(back, until=14.1))
        self.assertEqual(self.db.read_bytes(), original)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--artifacts", type=Path)
    args = parser.parse_args()
    run.BINARY = args.binary.resolve(strict=True)
    run.ARTIFACTS = args.artifacts
    if run.ARTIFACTS:
        run.ARTIFACTS.mkdir(parents=True, exist_ok=True)
    check = ShellFooterJourneys("test_connection_landing_panel_and_other_routes_own_their_footer")
    result = unittest.TextTestRunner(verbosity=2).run(unittest.TestSuite([check]))
    raise SystemExit(0 if result.wasSuccessful() else 1)
