"""Native replacement for the legacy query-favorites persistence journey."""
import sqlite3

from offline import fill
from scenarios import fixture, query_steps, tap, visible


class FavoritesJourney:
    def test_query_favorite_real_file_restart_execute_rename_and_trash(self):
        fixture(self.store, self.db)
        statement = "SELECT customer_name FROM customers WHERE id=2;"
        title = "SQLite favorite"
        renamed = "SQLite renamed"
        favorites = self.root / "favorites"
        form = [*query_steps(statement)[:-1], tap(7.4, "rail-settings")]
        invalid = self.replay([*form, tap(8.8, "favorite-save")], until=10)
        self.expect_rendered(invalid, key="favorite-error")
        self.expect_rendered(invalid, key="favorite-empty")
        self.assertFalse(list(favorites.glob("*.sql")))

        saved = self.replay([
            *form, *fill(8.8, "favorite-title-input", title), tap(10.5, "favorite-save"),
        ], until=12)
        self.expect_rendered(saved, text=title, within="favorite-row-0")
        self.assertFalse(visible(saved, key="favorite-error"))
        paths = list(favorites.glob("*.sql"))
        self.assertEqual(len(paths), 1)
        path = paths[0]
        original = path.read_text(encoding="utf-8")
        self.assertIn(f"-- title: {title}\n", original)
        self.assertIn("-- connectionId: e2e-sqlite\n", original)
        self.assertTrue(original.endswith(f"{statement}\n"))

        load = [tap(.5, "conn-open-e2e-sqlite"), tap(3, "conn-toolbar-new-query"),
                tap(4.3, "query-library-favorites-toggle"), tap(5.1, "query-favorite-0")]
        reopened = self.replay(load, until=6)
        self.expect_rendered(reopened, text=title, within="query-favorites-panel")
        self.assertFalse(visible(reopened, text="fixture-lin", within="result-workspace"))
        self.assertFalse((self.root / "query-history.json").exists())
        with sqlite3.connect(self.db) as database:
            expected = database.execute(statement).fetchone()[0]
        result = self.replay([*load, tap(6.2, "sql-run")], until=8.7)
        self.expect_rendered(result, text=expected, within="result-workspace")
        self.assertFalse(visible(result, text="fixture-ada", within="result-workspace"))

        rename = self.replay([
            tap(.5, "rail-settings"), *fill(1.8, "favorite-title-input", renamed),
            tap(3.5, "favorite-rename-0"),
        ], until=5)
        self.expect_rendered(rename, text=renamed, within="favorite-row-0")
        self.assertEqual(list(favorites.glob("*.sql")), [path])
        updated = path.read_text(encoding="utf-8")
        self.assertIn(f"-- title: {renamed}\n", updated)
        self.assertIn("-- connectionId: e2e-sqlite\n", updated)
        self.assertTrue(updated.endswith(f"{statement}\n"))

        deleted = self.replay([tap(.5, "rail-settings"), tap(2, "favorite-delete-0")], until=3.5)
        self.expect_rendered(deleted, key="favorite-empty")
        self.assertFalse(visible(deleted, key="favorite-row-0"))
        self.assertFalse(path.exists())
        trash = list((favorites / ".trash").glob(f"*__{path.name}"))
        self.assertEqual(len(trash), 1)
        self.assertEqual(trash[0].read_text(encoding="utf-8"), updated)
