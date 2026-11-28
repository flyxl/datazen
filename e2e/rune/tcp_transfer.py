"""Rune UI transfer from an exact PostgreSQL namespace to real MySQL tables."""
from concurrent.futures import ThreadPoolExecutor
import json
from time import monotonic, sleep
import uuid

from scenarios import tap, visible, open_tool_steps
from tcp_sql import DisposableSql


def wait_for_transfer_review(trace, replay):
    """Read ordinary Rune motion frames until the actual review button is painted."""
    required = ("native-transfer-confirm", "native-transfer-yes")
    ids, shown = {}, set()
    offset = 0
    deadline = monotonic() + 60
    while not replay.done() and monotonic() < deadline:
        if not trace.is_file():
            sleep(.1)
            continue
        with trace.open("rb") as stream:
            stream.seek(offset)
            while line := stream.readline():
                if not line.endswith(b"\n"):
                    # This frame is still being written; retry it next poll.
                    break
                offset = stream.tell()
                frame = json.loads(line)
                for meta in frame.get("m", []):
                    key = meta["nm"].rsplit("/", 1)[-1]
                    if key in required:
                        ids[meta["id"]] = key
                gone = set(frame.get("g", []))
                for item in frame.get("e", []):
                    element, x, y, width, height, opacity = item[:6]
                    if element in ids:
                        if x + width > 0 and y + height > 0 and x < 1440 and y < 900 and (
                            width > 0 and height > 0 and opacity > .5
                        ):
                            shown.add(element)
                        else:
                            shown.discard(element)
                shown.difference_update(gone)
                # Refuse to start external DDL if a delayed dialog appears
                # close to the scripted confirm at t=40.
                if frame.get("t", 0) >= 19_000:
                    return False
                if {ids[n] for n in shown} == set(required):
                    return True
        sleep(.1)
    return False


class TcpTransferJourneys:
    def write_transfer_profiles(self, source, target, *, source_options=None, target_options=None):
        labels = {"postgresql": "PostgreSQL fixture", "mysql": "MySQL fixture"}
        def profile(server, identifier, caption, **extra):
            return dict(id=identifier, name=caption, databaseType=server.engine,
                        host="127.0.0.1", port=server.port, database=server.database,
                        username=server.user, password=server.password, sslMode="disable",
                        connectionTimeout=8, **extra)
        self.write_tcp_profiles([
            profile(source, "xfer-src", labels[source.engine], **(source_options or {})),
            profile(target, "xfer-dst", labels[target.engine], **(target_options or {})),
        ])

    def sql_transfer_fixture(self, configs, scale, *, engine="InnoDB"):
        pg = DisposableSql("postgresql", configs["postgresql"])
        mysql = DisposableSql("mysql", configs["mysql"])
        tag = uuid.uuid4().hex[:10]
        schema = f"rune_xfer_{tag}"
        table = f"rune_xfer_{tag}"
        src = f"{schema}.{table}"
        decoy = f"public.{table}"
        pg.query(f"CREATE SCHEMA {schema};")
        self.addCleanup(pg.query, f"DROP SCHEMA {schema};")
        for relation in (src, decoy):
            pg.query(f"CREATE TABLE {relation}(id INT PRIMARY KEY, amount NUMERIC(30,6), note VARCHAR(8));")
            self.addCleanup(pg.query, f"DROP TABLE {relation};")
        pg.query(f"""INSERT INTO {src} VALUES
            (1,9007199254740993.010001,NULL),(2,9007199254740993.010002,'');
            INSERT INTO {decoy} VALUES (77,77,'decoy');""")
        mysql.query(f"""CREATE TABLE {table}(id INT PRIMARY KEY, amount DECIMAL(30,{scale}),
            note VARCHAR(80), sticky VARCHAR(20) DEFAULT 'from-default')
            ENGINE={engine} DEFAULT CHARSET=utf8mb4;
            INSERT INTO {table} VALUES (8,8,'keep','sentinel');""")
        self.addCleanup(mysql.query, f"DROP TABLE {table};")

        self.write_transfer_profiles(pg, mysql, source_options={"schema": schema})

        pg_rows = f"""
            SELECT id::text || '|' || amount::text || '|' ||
              CASE WHEN note IS NULL THEN '<NULL>' WHEN note='' THEN '<EMPTY>' ELSE note END
            FROM {src} ORDER BY id;"""
        target_rows = f"""
            SELECT CONCAT(id,'|',CAST(amount AS CHAR),'|',
              CASE WHEN note IS NULL THEN '<NULL>' WHEN note='' THEN '<EMPTY>' ELSE note END,
              '|',sticky) FROM {table} ORDER BY id;"""
        steps = [
            *open_tool_steps(.5, 1.0, "data-transfer"),
            tap(1.5, "native-transfer-source-profile-xfer-src"),
            tap(2.0, "native-transfer-target-profile-xfer-dst"),
            tap(4.2, f"native-transfer-target-pick-table-{table}"),
            tap(6.0, "native-transfer-map"),
            tap(6.6, "native-transfer-source-column"),
            tap(7.0, "native-transfer-target-column"),
            tap(7.5, "native-transfer-map"),
            tap(8.1, "native-transfer-source-column"),
            tap(8.5, "native-transfer-target-column"),
            tap(9.0, "native-transfer-map"),
            tap(9.7, "native-transfer-review"),
        ]
        expected = ["1|9007199254740993.010001|<NULL>", "2|9007199254740993.010002|<EMPTY>"]
        self.assertEqual(pg.query(pg_rows).splitlines(), expected)
        return pg, mysql, schema, table, decoy, pg_rows, target_rows, steps, expected

    def run_sql_transfer_exact(self, configs):
        pg, mysql, schema, table, decoy, source, target, steps, expected = self.sql_transfer_fixture(configs, 6)
        confirmation = self.replay(steps, until=12.4)
        self.expect_rendered(confirmation, key="native-transfer-confirm",
                             text=f"From PostgreSQL fixture / datazen_e2e / {schema} / {table} to MySQL fixture / datazen_e2e / {table}")
        self.expect_rendered(confirmation, key="native-transfer-confirm-pair-1", text="amount → amount")
        self.expect_rendered(confirmation, key="native-transfer-confirm-pair-2", text="note → note")
        self.assertEqual(mysql.query(target), "8|8.000000|keep|sentinel")

        cancelled = self.replay([*steps, tap(12.5, "native-transfer-no")], until=13.8)
        self.assertFalse(visible(cancelled, key="native-transfer-confirm"))
        self.assertEqual(mysql.query(target), "8|8.000000|keep|sentinel")

        result = self.replay([*steps, tap(12.5, "native-transfer-yes")], until=17.0)
        self.expect_rendered(result, key="native-transfer-success", text="Inserted 2 rows.")
        self.assertEqual(mysql.query(target).splitlines(),
                         [*(f"{row}|from-default" for row in expected), "8|8.000000|keep|sentinel"])
        self.assertEqual(pg.query(source).splitlines(), expected)
        self.assertEqual(pg.query(f"SELECT id||'|'||note FROM {decoy};"), "77|decoy")

    def run_sql_transfer_rejects_stale_collation(self, configs):
        """Native drift regression: a changed MySQL collation invalidates the painted review."""
        pg, mysql, _, table, decoy, source, target, steps, expected = self.sql_transfer_fixture(configs, 6)
        before, after = "utf8mb4_0900_ai_ci", "utf8mb4_0900_as_cs"

        def set_note_collation(collation):
            mysql.query(
                f"ALTER TABLE {table} MODIFY note VARCHAR(80) CHARACTER SET utf8mb4 "
                f"COLLATE {collation} NULL DEFAULT NULL;"
            )

        set_note_collation(before)
        column_oracle = f"""
            SELECT CONCAT(COLUMN_NAME,'|',COLUMN_TYPE,'|',IS_NULLABLE,'|',
                          COALESCE(COLUMN_DEFAULT,'<NULL>'),'|',
                          COALESCE(CHARACTER_SET_NAME,'-'),'|',COALESCE(COLLATION_NAME,'-'))
            FROM information_schema.columns
            WHERE TABLE_SCHEMA=DATABASE() AND TABLE_NAME='{table}'
            ORDER BY ORDINAL_POSITION;
        """
        # Compare the actual stored column, not an expression with explicit
        # COLLATE that could mask the fixture's real sorting/equality semantics.
        equalities = f"""
            SELECT CONCAT(note='KEEP','|',note=_utf8mb4 X'6BC3A96570')
            FROM {table} WHERE id=8;
        """
        columns = mysql.query(column_oracle).splitlines()
        note = f"note|varchar(80)|YES|<NULL>|utf8mb4|{before}"
        self.assertIn(note, columns)
        self.assertEqual(mysql.query(equalities), "1|1")
        self.assertEqual(mysql.query(target), "8|8.000000|keep|sentinel")

        trace = self.root / "transfer-collation-review.jsonl"
        with ThreadPoolExecutor(max_workers=1) as pool:
            replay = pool.submit(self.replay, [*steps, tap(40, "native-transfer-yes")],
                                 until=45, timeout=110, trace=trace)
            self.assertTrue(wait_for_transfer_review(trace, replay),
                            "Native confirmation never painted before the intervening DDL window")
            self.assertEqual(mysql.query(target), "8|8.000000|keep|sentinel")
            self.assertFalse(replay.done(), "Native process exited before the independent DDL")
            set_note_collation(after)
            self.assertEqual(mysql.query(column_oracle).splitlines(), [
                note.removesuffix(before) + after if row == note else row for row in columns
            ])
            self.assertEqual(mysql.query(equalities), "0|0")
            result = replay.result()

        self.expect_rendered(result, key="native-transfer-error",
                             text="A table structure changed. Review the transfer again.")
        self.assertFalse(visible(result, key="native-transfer-success"))
        self.assertFalse(visible(result, key="native-transfer-yes"))
        self.assertEqual(mysql.query(target), "8|8.000000|keep|sentinel")
        self.assertEqual(mysql.query(equalities), "0|0")
        self.assertEqual(pg.query(source).splitlines(), expected)
        self.assertEqual(pg.query(f"SELECT id||'|'||note FROM {decoy};"), "77|decoy")

    def run_sql_transfer_rejects_narrow(self, configs):
        pg, mysql, _, table, _, source, target, steps, expected = self.sql_transfer_fixture(configs, 4)
        result = self.replay(steps, until=12.5)
        self.expect_rendered(
            result, key="native-transfer-error",
            text=f"cannot map source column '{table}.amount' to '{table}.amount': "
                 "target type 'decimal(30,4)' is narrower than source numeric type 'numeric(30,6)'",
        )
        self.assertFalse(visible(result, key="native-transfer-yes"))
        self.assertFalse(visible(result, key="native-transfer-success"))
        self.assertEqual(mysql.query(target), "8|8.0000|keep|sentinel")
        self.assertEqual(pg.query(source).splitlines(), expected)


    def run_sql_transfer_rejects_nontransactional(self, configs):
        pg, mysql, _, _, _, source, target, steps, expected = self.sql_transfer_fixture(
            configs, 6, engine="MyISAM",
        )
        result = self.replay(steps, until=12.5)
        self.expect_rendered(result, key="native-transfer-error",
                             text="The selected MySQL table is not proven transactional.")
        self.assertFalse(visible(result, key="native-transfer-yes"))
        self.assertEqual(mysql.query(target), "8|8.000000|keep|sentinel")
        self.assertEqual(pg.query(source).splitlines(), expected)

    def reverse_fixture(self, configs, scale, *, identity=False):
        mysql = DisposableSql("mysql", configs["mysql"])
        pg = DisposableSql("postgresql", configs["postgresql"])
        table = f"rune_rev_{uuid.uuid4().hex[:10]}"
        schema = table
        dst, decoy = f"{schema}.{table}", f"public.{table}"
        mysql.query(f"""CREATE TABLE {table}
            (id INT PRIMARY KEY, amount DECIMAL(30,6), note VARCHAR(80))
            ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
            INSERT INTO {table} VALUES (1,9007199254740993.010001,NULL),
            (2,9007199254740993.010002,''),(3,42.125000,'雪');""")
        self.addCleanup(mysql.query, f"DROP TABLE {table};")
        pg.query(f"CREATE SCHEMA {schema};")
        self.addCleanup(pg.query, f"DROP SCHEMA {schema};")
        for relation in (dst, decoy):
            id_definition = (
                "INT GENERATED BY DEFAULT AS IDENTITY (START WITH 1 CACHE 1)"
                if identity and relation == dst else "INT"
            )
            pg.query(f"""CREATE TABLE {relation}
                (id {id_definition} PRIMARY KEY, amount NUMERIC(30,{scale}), note TEXT,
                 sticky TEXT NOT NULL DEFAULT 'default-before');
                INSERT INTO {relation} VALUES (8,8,'keep','sentinel');""")
            self.addCleanup(pg.query, f"DROP TABLE {relation};")
        self.write_transfer_profiles(mysql, pg, target_options={"schema": schema})
        steps = [
            *open_tool_steps(.5, 1.0, "data-transfer"),
            tap(1.5, "native-transfer-source-profile-xfer-src"),
            tap(2.0, "native-transfer-target-profile-xfer-dst"),
            tap(4.2, f"native-transfer-source-pick-table-{table}"),
            tap(6.0, "native-transfer-map"),
            tap(6.6, "native-transfer-source-column"), tap(7.0, "native-transfer-target-column"),
            tap(7.5, "native-transfer-map"),
            tap(8.1, "native-transfer-source-column"), tap(8.5, "native-transfer-target-column"),
            tap(9.0, "native-transfer-map"), tap(9.7, "native-transfer-review"),
        ]
        mysql_rows = f"""
            SELECT CONCAT(id,'|',CAST(amount AS CHAR),'|',
              CASE WHEN note IS NULL THEN '<NULL>' WHEN note='' THEN '<EMPTY>' ELSE note END)
            FROM {table} ORDER BY id;"""
        def pg_rows(relation):
            return f"""SELECT id||'|'||amount::text||'|'||
              CASE WHEN note IS NULL THEN '<NULL>' WHEN note='' THEN '<EMPTY>' ELSE note END
              ||'|'||sticky FROM {relation} ORDER BY id;"""
        values = ["1|9007199254740993.010001|<NULL>", "2|9007199254740993.010002|<EMPTY>",
                  "3|42.125000|雪"]
        self.assertEqual(mysql.query(mysql_rows).splitlines(), values)
        return (mysql, pg, schema, table, steps, mysql_rows, pg_rows(dst), pg_rows(decoy), values)

    def run_reverse_exact(self, configs):
        mysql, pg, schema, table, steps, source, target, decoy, values = self.reverse_fixture(configs, 6)
        review = self.replay(steps, until=12.4)
        self.expect_rendered(review, key="native-transfer-confirm",
                             text=f"From MySQL fixture / datazen_e2e / {table} "
                                  f"to PostgreSQL fixture / datazen_e2e / {schema} / {table}")
        self.assertEqual(pg.query(target), "8|8.000000|keep|sentinel")
        result = self.replay([*steps, tap(12.5, "native-transfer-yes")], until=17.0)
        self.expect_rendered(result, key="native-transfer-success", text="Inserted 3 rows.")
        self.assertEqual(pg.query(target).splitlines(),
                         [*(f"{row}|default-before" for row in values), "8|8.000000|keep|sentinel"])
        self.assertEqual(pg.query(decoy), "8|8.000000|keep|sentinel")
        self.assertEqual(mysql.query(source).splitlines(), values)

    def run_reverse_identity_advances(self, configs):
        """Migrate the R3 data-only explicit identity import and generated-ID follow-up."""
        mysql, pg, schema, table, steps, source, target, decoy, values = self.reverse_fixture(
            configs, 6, identity=True,
        )
        relation = f"{schema}.{table}"
        mysql.query(f"UPDATE {table} SET id=101 WHERE id=3;")
        source_rows = [*values[:2], "101|" + values[2].split("|", 1)[1]]
        self.assertEqual(mysql.query(source).splitlines(), source_rows)
        # Advance this real sequence once before transfer, then remove the
        # control row. Without reseeding, the next generated value would be
        # 2, which the native import is explicitly about to occupy.
        self.assertEqual(pg.query(
            f"INSERT INTO {relation}(amount,note) VALUES (1,'control') RETURNING id;"
        ), "1")
        pg.query(f"DELETE FROM {relation} WHERE id=1;")
        self.assertEqual(pg.query(target), "8|8.000000|keep|sentinel")

        result = self.replay([*steps, tap(12.5, "native-transfer-yes")], until=17.0)
        self.expect_rendered(result, key="native-transfer-success", text="Inserted 3 rows.")
        rows_after_transfer = [
            *(f"{row}|default-before" for row in source_rows[:2]),
            "8|8.000000|keep|sentinel",
            f"{source_rows[2]}|default-before",
        ]
        self.assertEqual(pg.query(target).splitlines(), rows_after_transfer)

        # Independent ordinary INSERT omits id; prove the target itself
        # generates usable next IDs above both existing and imported rows.
        self.assertEqual(pg.query(
            f"INSERT INTO {relation}(amount,note) VALUES (19.5,'new-first') RETURNING id;"
        ), "102")
        self.assertEqual(pg.query(
            f"INSERT INTO {relation}(amount,note) VALUES (20.5,'new-second') RETURNING id;"
        ), "103")
        self.assertEqual(pg.query(target).splitlines(), [
            *rows_after_transfer, "102|19.500000|new-first|default-before",
            "103|20.500000|new-second|default-before",
        ])
        self.assertEqual(pg.query(decoy), "8|8.000000|keep|sentinel")
        self.assertEqual(mysql.query(source).splitlines(), source_rows)

    def run_reverse_rejects_scale_loss(self, configs):
        mysql, pg, _, _, steps, source, target, decoy, values = self.reverse_fixture(configs, 4)
        result = self.replay(steps, until=12.5)
        self.expect_rendered(result, key="native-transfer-error",
                             text="Cannot prove a lossless conversion from amount (decimal(30,6)) to amount (numeric(30,4)).")
        self.assertFalse(visible(result, key="native-transfer-yes"))
        self.assertEqual(pg.query(target), "8|8.0000|keep|sentinel")
        self.assertEqual(pg.query(decoy), "8|8.0000|keep|sentinel")
        self.assertEqual(mysql.query(source).splitlines(), values)

    def run_reverse_duplicate_conflict(self, configs):
        mysql, pg, schema, table, steps, source, target, decoy, values = self.reverse_fixture(configs, 6)
        pg.query(f"INSERT INTO {schema}.{table} VALUES (3,3,'clash','existing');")
        rows_before = ["3|3.000000|clash|existing", "8|8.000000|keep|sentinel"]
        self.assertEqual(pg.query(target).splitlines(), rows_before)
        result = self.replay([*steps, tap(12.5, "native-transfer-yes")], until=17.0)
        self.expect_rendered(result, key="native-transfer-error",
                             text="The transfer did not commit. Check the mappings and table constraints.")
        self.assertFalse(visible(result, key="native-transfer-success"))
        self.assertEqual(pg.query(target).splitlines(), rows_before)
        self.assertEqual(pg.query(decoy), "8|8.000000|keep|sentinel")
        self.assertEqual(mysql.query(source).splitlines(), values)


    def run_reverse_deferred_commit_conflict(self, configs):
        mysql, pg, schema, table, steps, source, target, decoy, values = self.reverse_fixture(configs, 6)
        relation = f"{schema}.{table}"
        pg.query(f"""ALTER TABLE {relation} ADD UNIQUE (amount) DEFERRABLE INITIALLY DEFERRED;
            UPDATE {relation} SET amount=42.125000 WHERE id=8;""")
        sentinel = "8|42.125000|keep|sentinel"

        # Distinct IDs and a deferred UNIQUE violation: prove INSERT succeeded
        # inside the transaction, then inspect the SQLSTATE from COMMIT itself.
        proof = pg.query(rf"""
            BEGIN;
            INSERT INTO {relation}(id, amount, note) VALUES(91,42.125000,'proof');
            SELECT 'visible-in-transaction' FROM {relation} WHERE id=91;
            \set ON_ERROR_STOP off
            COMMIT;
            \echo :SQLSTATE
        """)
        self.assertEqual(proof.splitlines(), ["visible-in-transaction", "23505"])
        self.assertEqual(pg.query(target), sentinel)

        result = self.replay([*steps, tap(12.5, "native-transfer-yes")], until=17.0)
        self.expect_rendered(result, key="native-transfer-error",
                             text="The commit outcome is unknown. Check the target before retrying.")
        self.assertFalse(visible(result, key="native-transfer-success"))
        self.assertEqual(pg.query(target), sentinel)
        self.assertEqual(pg.query(decoy), "8|8.000000|keep|sentinel")
        self.assertEqual(mysql.query(source).splitlines(), values)


def with_sql_transfer_journeys(suite, configs):
    if {"mysql", "postgresql"} <= configs.keys():
        suite.test_postgresql_to_mysql_preserves_exact_reviewed_values = lambda self: self.run_sql_transfer_exact(configs)
        suite.test_postgresql_to_mysql_review_refuses_collation_drift = lambda self: self.run_sql_transfer_rejects_stale_collation(configs)
        suite.test_postgresql_to_mysql_review_refuses_narrow_decimal = lambda self: self.run_sql_transfer_rejects_narrow(configs)
        suite.test_postgresql_to_mysql_review_refuses_nontransactional_table = lambda self: self.run_sql_transfer_rejects_nontransactional(configs)
        suite.test_mysql_to_postgresql_preserves_values_and_namespace = lambda self: self.run_reverse_exact(configs)
        suite.test_mysql_to_postgresql_advances_identity_after_explicit_ids = lambda self: self.run_reverse_identity_advances(configs)
        suite.test_mysql_to_postgresql_review_refuses_scale_loss = lambda self: self.run_reverse_rejects_scale_loss(configs)
        suite.test_mysql_to_postgresql_duplicate_conflict_keeps_target_unchanged = lambda self: self.run_reverse_duplicate_conflict(configs)
        suite.test_mysql_to_postgresql_deferred_conflict_fails_at_commit = lambda self: self.run_reverse_deferred_commit_conflict(configs)
