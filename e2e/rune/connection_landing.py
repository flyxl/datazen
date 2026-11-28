"""Real native replay for the legacy populated connections landing."""
import sqlite3
import sys

from offline import fill, key
from scenarios import fixture, tap, visible


class ConnectionLandingJourney:
    def test_light_shell_caption_tracks_live_sqlite_and_settings_back(self):
        fixture(self.store, self.db)
        (self.root / "settings.json").write_text(
            '{"ui":{"language":"en","themeMode":"light"}}', encoding="utf-8"
        )

        def caption(snapshot, text, chromium_reference_x):
            self.expect_rendered(snapshot, key="datazen-titlebar-label", text=text)
            x, _, width, _ = self._settled_rect(snapshot, "datazen-titlebar-label")
            sx, _, sw, _ = self._settled_rect(snapshot, "datazen-titlebar-title")
            self.assertAlmostEqual(x + width / 2, sx + sw / 2, delta=1)
            if sys.platform.startswith("linux"):
                # Fixed 1440x900 Chromium reference. A 5pt allowance keeps
                # the existing native menu and theme-control metrics intact.
                self.assertAlmostEqual(x + width / 2, chromium_reference_x, delta=5)

        home = self.replay([], until=2)
        self.expect_rendered(home, key="connections-home-title")
        caption(home, "Connections - DataZen", 723.5)
        actions = [tap(.5, "conn-open-e2e-sqlite")]
        active = self.replay(actions, until=3)
        self.expect_rendered(active, key="tree-table-customers")
        caption(active, "Rune SQLite E2E - SQLite - DataZen", 723.5)
        actions.append(tap(3.1, "rail-settings"))
        settings = self.replay(actions, until=4.3)
        self.expect_rendered(settings, key="settings-page")
        caption(settings, "Settings - DataZen", 739.5)
        actions.append(tap(4.4, "settings-back"))
        restored = self.replay(actions, until=5.8)
        self.expect_rendered(restored, key="tree-table-customers")
        caption(restored, "Rune SQLite E2E - SQLite - DataZen", 723.5)

    def test_legacy_connection_landing_actions_only_expose_query_after_real_connection(self):
        """Migrate EMPTY-001–004 through the ordinary Native connections landing."""
        fixture(self.store, self.db)
        saved_before = self.store.read_bytes()
        data_before = self.db.read_bytes()
        sql = "SELECT customer_name AS landing_row FROM customers WHERE id = 2;"
        def read_fixture():
            with sqlite3.connect(f"{self.db.as_uri()}?mode=ro", uri=True) as db:
                return (
                    db.execute(sql).fetchone()[0],
                    db.execute("SELECT * FROM customers ORDER BY id").fetchall(),
                    db.execute("SELECT name, sql FROM sqlite_schema ORDER BY name").fetchall(),
                )

        original = read_fixture()
        expected = original[0]

        waiting = self.replay([], until=1.0)
        self.expect_rendered(waiting, key="connections-home-title")
        self.expect_rendered(waiting, key="connections-home-subtitle")
        self.expect_rendered(waiting, key="conn-open-e2e-sqlite")
        self.expect_rendered(waiting, key="connection-new")
        self.expect_rendered(waiting, key="import-connections")
        self.assertFalse(visible(waiting, key="connections-empty"))
        self.assertFalse(visible(waiting, key="conn-toolbar-new-query"))

        new = [tap(1.0, "connection-new")]
        creating = self.replay(new, until=2.4)
        self.expect_rendered(creating, key="connection-name-input")
        self.expect_rendered(creating, key="connection-test")
        self.expect_rendered(creating, key="connection-save")
        self.assertFalse(visible(creating, key="import-dialog-fields"))

        open_import = [*new, tap(2.7, "dialog-cancel"), tap(3.8, "import-connections")]
        importing = self.replay(open_import, until=5.3)
        self.expect_rendered(importing, key="import-dialog-fields")
        self.expect_rendered(importing, key="import-formats-hint")
        self.expect_rendered(importing, key="import-choose-file")
        self.assertFalse(visible(importing, key="connection-name-input"))

        dismissed = [*open_import, key(5.5, "Escape")]
        idle_again = self.replay(dismissed, until=7.0)
        self.expect_rendered(idle_again, key="connections-home-title")
        self.assertFalse(visible(idle_again, key="import-dialog-fields"))
        self.assertFalse(visible(idle_again, key="conn-toolbar-new-query"))
        self.assertEqual(self.store.read_bytes(), saved_before)
        self.assertEqual(self.db.read_bytes(), data_before)

        connected = [*dismissed, tap(7.4, "conn-open-e2e-sqlite")]
        active = self.replay(connected, until=10.4)
        self.expect_rendered(active, key="conn-toolbar-new-query")
        self.expect_rendered(active, key="tree-table-customers")
        self.assertFalse(visible(active, key="connections-home-title"))
        self.assertFalse(visible(active, key="desktop-adapter-message"))

        result = self.replay([
            *connected, tap(10.8, "conn-toolbar-new-query"),
            *fill(11.8, "sql-editor-scroll", sql), tap(14.4, "sql-run"),
        ], until=16.2)
        self.expect_rendered(result, text="landing_row", within="result-workspace")
        self.expect_rendered(result, text=expected, within="result-workspace")
        # Opening a writable SQLite connection can change the journal-mode
        # header; assert no logical schema or data changes after the query.
        self.assertEqual(read_fixture(), original)
