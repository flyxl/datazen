"""Real, insert-only, mapped transfer between two private SQLite files."""
import json
import sqlite3

from scenarios import tap, visible


class TransferJourneys:
    def test_sqlite_mapped_transfer_preserves_nulls_rows_and_safety_guards(self):
        target = self.root / "transfer-target.sqlite"
        with sqlite3.connect(self.db) as source:
            source.executescript("""
                CREATE TABLE source_records(source_id INTEGER PRIMARY KEY, source_text TEXT);
                INSERT INTO source_records VALUES (1, NULL), (2, ''), (3, 'copied');
            """)
        with sqlite3.connect(target) as dest:
            dest.executescript("""
                CREATE TABLE target_records(target_id INTEGER PRIMARY KEY, target_text TEXT,
                                             target_note TEXT DEFAULT 'untouched-default');
                INSERT INTO target_records VALUES (8, 'keep', 'pre-existing');
            """)
        source_original = self.db.read_bytes()
        target_original = target.read_bytes()

        def profile(identifier, name, path, read_only=False):
            return {"id": identifier, "name": name, "databaseType": "sqlite",
                    "database": str(path), "readOnly": read_only, "connectionTimeout": 8}

        self.store.write_text(json.dumps({"connections": [
            profile("transfer-src", "Source fixture", self.db),
            profile("transfer-dst", "Target fixture", target),
            profile("transfer-lock", "Read only target", target, True),
            profile("transfer-alias", "Source alias", self.db),
        ]}), encoding="utf-8")

        def setup(target_id):
            return [
                tap(.5, "rail-tools"),
                tap(1.6, "native-transfer-source-profile-transfer-src"),
                tap(2.1, f"native-transfer-target-profile-{target_id}"),
                tap(3.7, "native-transfer-map"),
                tap(4.4, "native-transfer-source-column"),
                tap(4.9, "native-transfer-target-column"),
                tap(5.4, "native-transfer-map"),
                tap(6.2, "native-transfer-review"),
            ]

        def target_rows():
            with sqlite3.connect(target) as connection:
                return connection.execute("""
                    SELECT target_id, target_text, typeof(target_text), target_note
                    FROM target_records ORDER BY target_id
                """).fetchall()

        initial = [(8, "keep", "text", "pre-existing")]
        review = setup("transfer-dst")
        pending = self.replay(review, until=7.8)
        self.expect_rendered(pending, key="native-transfer-confirm",
                             text="From Source fixture / source_records to Target fixture / target_records")
        self.expect_rendered(pending, key="native-transfer-confirm-pair-0", text="source_id → target_id")
        self.expect_rendered(pending, key="native-transfer-confirm-pair-1", text="source_text → target_text")
        self.assertEqual(target_rows(), initial)
        self.assertEqual(target.read_bytes(), target_original)

        cancelled = self.replay([*review, tap(8, "native-transfer-no")], until=9.5)
        self.assertFalse(visible(cancelled, key="native-transfer-confirm"))
        self.assertFalse(visible(cancelled, key="native-transfer-success"))
        self.assertEqual(target_rows(), initial)
        self.assertEqual(target.read_bytes(), target_original)

        readonly = self.replay(setup("transfer-lock"), until=8)
        self.expect_rendered(readonly, key="native-transfer-error", text="The target is read-only.")
        self.assertFalse(visible(readonly, key="native-transfer-yes"))
        self.assertEqual(target_rows(), initial)
        self.assertEqual(target.read_bytes(), target_original)

        alias = self.replay(setup("transfer-alias"), until=8)
        self.expect_rendered(alias, key="native-transfer-error", text="Select two different SQLite files.")
        self.assertFalse(visible(alias, key="native-transfer-yes"))
        self.assertEqual(self.db.read_bytes(), source_original)

        confirm = [*review, tap(8, "native-transfer-yes")]
        finished = self.replay(confirm, until=10.5)
        self.expect_rendered(finished, key="native-transfer-success", text="Inserted 3 rows.")
        expected = [(1, None, "null", "untouched-default"),
                    (2, "", "text", "untouched-default"),
                    (3, "copied", "text", "untouched-default"), *initial]
        self.assertEqual(target_rows(), expected)
        self.assertEqual(self.db.read_bytes(), source_original)

        duplicate = self.replay(confirm, until=10.5)
        self.expect_rendered(duplicate, key="native-transfer-error",
                             text="The transfer did not commit. Check the mappings and table constraints.")
        self.assertFalse(visible(duplicate, key="native-transfer-success"))
        self.assertEqual(target_rows(), expected)
