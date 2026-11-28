"""Real, insert-only, mapped transfer between two private SQLite files."""
import json
import sqlite3

from scenarios import tap, visible, open_tool_steps


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
                *open_tool_steps(.5, 1.0, "data-transfer"),
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

    def test_sqlite_mapped_transfer_only_writes_reviewed_nondefault_table(self):
        target = self.root / "transfer-selected.sqlite"
        with sqlite3.connect(self.db) as connection:
            connection.executescript("""
                CREATE TABLE a_source_decoy(decoy_id INTEGER PRIMARY KEY, decoy_text TEXT);
                INSERT INTO a_source_decoy VALUES (40, 'decoy source');
                CREATE TABLE z_source_selected(item_id INTEGER PRIMARY KEY, item_label TEXT);
                INSERT INTO z_source_selected VALUES (7, 'verified source');
            """)
        with sqlite3.connect(target) as connection:
            connection.executescript("""
                CREATE TABLE a_target_decoy(decoy_id INTEGER PRIMARY KEY, decoy_text TEXT);
                INSERT INTO a_target_decoy VALUES (50, 'decoy target');
                CREATE TABLE z_target_selected(target_id INTEGER PRIMARY KEY, target_label TEXT);
                INSERT INTO z_target_selected VALUES (80, 'pre-existing');
            """)
        original = target.read_bytes()
        source_original = self.db.read_bytes()
        self.store.write_text(json.dumps({"connections": [
            {"id": "transfer-src", "name": "Source fixture", "databaseType": "sqlite",
             "database": str(self.db), "connectionTimeout": 8},
            {"id": "transfer-dst", "name": "Target fixture", "databaseType": "sqlite",
             "database": str(target), "connectionTimeout": 8},
        ]}), encoding="utf-8")

        wrong = [
            *open_tool_steps(.5, 1.0, "data-transfer"),
            tap(1.6, "native-transfer-source-profile-transfer-src"),
            tap(2.1, "native-transfer-target-profile-transfer-dst"),
            tap(3.6, "native-transfer-source-table"), tap(4.1, "native-transfer-target-table"),
            tap(5.2, "native-transfer-target-column"), tap(5.6, "native-transfer-map"),
            tap(6.2, "native-transfer-map"),
        ]
        rejected = self.replay(wrong, until=7.3)
        self.expect_rendered(rejected, key="native-transfer-pair-0", text="item_id → target_label")
        self.expect_rendered(rejected, key="native-transfer-error", text="Each column can be mapped once.")
        self.assertFalse(visible(rejected, key="native-transfer-pair-1"))
        self.assertEqual(target.read_bytes(), original)

        repaired = [
            *wrong, tap(7.6, "native-transfer-remove-0"),
            tap(8.15, "native-transfer-target-column"), tap(8.7, "native-transfer-map"),
            tap(9.25, "native-transfer-source-column"), tap(9.75, "native-transfer-target-column"),
            tap(10.3, "native-transfer-map"), tap(11.2, "native-transfer-review"),
        ]
        reviewed = self.replay(repaired, until=12.6)
        self.expect_rendered(reviewed, key="native-transfer-confirm",
                             text="From Source fixture / z_source_selected to Target fixture / z_target_selected")
        self.expect_rendered(reviewed, key="native-transfer-confirm-pair-0", text="item_id → target_id")
        self.expect_rendered(reviewed, key="native-transfer-confirm-pair-1", text="item_label → target_label")
        self.assertFalse(visible(reviewed, text="item_id → target_label"))
        self.assertEqual(target.read_bytes(), original)

        result = self.replay([*repaired, tap(13.1, "native-transfer-yes")], until=15.5)
        self.expect_rendered(result, key="native-transfer-success", text="Inserted 1 rows.")
        with sqlite3.connect(target) as connection:
            self.assertEqual(connection.execute("SELECT * FROM a_target_decoy").fetchall(),
                             [(50, "decoy target")])
            self.assertEqual(connection.execute("SELECT * FROM z_target_selected ORDER BY target_id").fetchall(),
                             [(7, "verified source"), (80, "pre-existing")])
        self.assertEqual(self.db.read_bytes(), source_original)

    def test_sqlite_mapping_editor_auto_match_and_manual_pairs_use_catalog(self):
        target = self.root / "transfer-mapping-editor.sqlite"
        with sqlite3.connect(self.db) as source:
            source.executescript("""
                CREATE TABLE incoming(id INTEGER PRIMARY KEY, incoming_note TEXT, source_only TEXT);
                INSERT INTO incoming VALUES (1, NULL, 'not copied'), (2, '明细', 'not copied');
            """)
        with sqlite3.connect(target) as dest:
            dest.executescript("""
                CREATE TABLE receiving(id INTEGER PRIMARY KEY, note TEXT, sticky TEXT DEFAULT 'retained');
                INSERT INTO receiving VALUES (8, 'sentinel', 'existing');
            """)
        self.store.write_text(json.dumps({"connections": [
            {"id": identifier, "name": identifier, "databaseType": "sqlite",
             "database": str(path), "connectionTimeout": 8}
            for identifier, path in (("source", self.db), ("target", target))
        ]}), encoding="utf-8")
        source_original = self.db.read_bytes()
        target_original = target.read_bytes()
        setup = [
            *open_tool_steps(.5, 1.0, "data-transfer"),
            tap(1.6, "native-transfer-source-profile-source"),
            tap(2.1, "native-transfer-target-profile-target"),
            tap(3.4, "native-transfer-toggle-column-editor"),
            tap(4.5, "data-transfer-auto-match"),
        ]
        open_manual_row = "data-transfer-target-select-incoming_note-field-trigger"
        unmatched = self.replay(setup, until=5.5)
        for column in ("id", "incoming_note", "source_only"):
            self.expect_rendered(unmatched, key=f"data-transfer-column-row-{column}")
        for unsupported in ("data-transfer-target-table-input", "data-transfer-create-new-toggle",
                            "data-transfer-recordset", "data-transfer-source-filter"):
            self.assertFalse(visible(unmatched, key=unsupported))

        duplicate = self.replay([
            *setup, tap(5.5, open_manual_row), tap(6.5, "option1"),
        ], until=7.8)
        self.expect_rendered(duplicate, key="native-transfer-error", text="Each column can be mapped once.")
        self.assertEqual(target.read_bytes(), target_original)

        review = [
            *setup, tap(5.5, open_manual_row), tap(6.5, "option2"),
            tap(7.6, "native-transfer-review"),
        ]
        preview = self.replay(review, until=9.2)
        self.expect_rendered(preview, key="native-transfer-confirm",
                             text="From source / incoming to target / receiving")
        self.expect_rendered(preview, key="native-transfer-confirm-pair-0", text="id → id")
        self.expect_rendered(preview, key="native-transfer-confirm-pair-1", text="incoming_note → note")
        self.assertFalse(visible(preview, key="native-transfer-confirm-pair-2"))
        self.assertEqual(target.read_bytes(), target_original)

        finished = self.replay([*review, tap(9.7, "native-transfer-yes")], until=11.5)
        self.expect_rendered(finished, key="native-transfer-success", text="Inserted 2 rows.")
        with sqlite3.connect(target) as dest:
            self.assertEqual(dest.execute("SELECT * FROM receiving ORDER BY id").fetchall(),
                             [(1, None, "retained"), (2, "明细", "retained"), (8, "sentinel", "existing")])
        self.assertEqual(self.db.read_bytes(), source_original)

    def test_sqlite_mapping_editor_returns_to_quick_mapping_and_resets_on_table_change(self):
        target = self.root / "transfer-mapping-table-change.sqlite"
        with sqlite3.connect(self.db) as source:
            source.executescript("""
                CREATE TABLE incoming(id INTEGER PRIMARY KEY, note TEXT);
                INSERT INTO incoming VALUES (1, 'mapped');
            """)
        with sqlite3.connect(target) as dest:
            dest.executescript("""
                CREATE TABLE a_previous(id INTEGER PRIMARY KEY, note TEXT);
                INSERT INTO a_previous VALUES (8, 'previous');
                CREATE TABLE z_current(id INTEGER PRIMARY KEY, note TEXT DEFAULT 'default');
                INSERT INTO z_current VALUES (9, 'current');
            """)
        self.store.write_text(json.dumps({"connections": [
            {"id": identifier, "name": identifier, "databaseType": "sqlite",
             "database": str(path), "connectionTimeout": 8}
            for identifier, path in (("source", self.db), ("target", target))
        ]}), encoding="utf-8")
        old_source = self.db.read_bytes()
        old_target = target.read_bytes()
        setup = [
            *open_tool_steps(.5, 1.0, "data-transfer"),
            tap(1.6, "native-transfer-source-profile-source"),
            tap(2.1, "native-transfer-target-profile-target"),
            tap(3.5, "native-transfer-map"),
            tap(4.4, "native-transfer-toggle-column-editor"),
        ]
        via_editor = self.replay(setup, until=5.45)
        self.expect_rendered(via_editor, text="id", within="data-transfer-target-select-id-field-trigger")
        self.expect_rendered(via_editor, key="data-transfer-column-row-note")

        quick_then_switch = [
            *setup, tap(5.3, "native-transfer-toggle-column-editor"),
            tap(6.3, "native-transfer-target-table"),
        ]
        cleared = self.replay([*quick_then_switch, tap(7.6, "native-transfer-review")], until=9)
        self.expect_rendered(cleared, text="z_current", within="native-transfer-target")
        self.expect_rendered(cleared, key="native-transfer-error", text="Add at least one column mapping.")
        self.assertFalse(visible(cleared, key="native-transfer-pair-0"))
        self.assertFalse(visible(cleared, key="native-transfer-yes"))
        self.assertEqual(target.read_bytes(), old_target)

        refreshed = [
            *quick_then_switch, tap(7.6, "native-transfer-toggle-column-editor"),
            tap(8.7, "data-transfer-auto-match"), tap(9.65, "data-transfer-skip-note"),
            tap(10.6, "native-transfer-toggle-column-editor"),
        ]
        quick = self.replay(refreshed, until=11.5)
        self.expect_rendered(quick, key="native-transfer-pair-0", text="id → id")
        self.assertFalse(visible(quick, key="native-transfer-pair-1"))
        self.assertFalse(visible(quick, key="data-transfer-column-editor"))
        self.assertEqual(target.read_bytes(), old_target)

        finished = self.replay([
            *refreshed, tap(11.5, "native-transfer-review"), tap(13.1, "native-transfer-yes"),
        ], until=15)
        self.expect_rendered(finished, key="native-transfer-success", text="Inserted 1 rows.")
        with sqlite3.connect(target) as dest:
            self.assertEqual(dest.execute("SELECT * FROM a_previous").fetchall(), [(8, "previous")])
            self.assertEqual(dest.execute("SELECT * FROM z_current ORDER BY id").fetchall(),
                             [(1, "default"), (9, "current")])
        self.assertEqual(self.db.read_bytes(), old_source)
