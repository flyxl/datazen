"""Rune UI transfer from an exact PostgreSQL namespace to real MySQL tables."""
import json
import os
import uuid

from scenarios import tap, visible
from tcp_sql import DisposableSql


class TcpTransferJourneys:
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

        def profile(server, identifier, caption, **extra):
            return dict(id=identifier, name=caption, databaseType=server.engine,
                        host="127.0.0.1", port=server.port, database=server.database,
                        username=server.user, password=server.password, sslMode="disable",
                        connectionTimeout=8, **extra)
        fd = os.open(self.store, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, mode=0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as store:
            json.dump({"connections": [
                profile(pg, "xfer-src", "Source PostgreSQL", schema=schema),
                profile(mysql, "xfer-dst", "Target MySQL"),
            ]}, store)

        pg_rows = f"""
            SELECT id::text || '|' || amount::text || '|' ||
              CASE WHEN note IS NULL THEN '<NULL>' WHEN note='' THEN '<EMPTY>' ELSE note END
            FROM {src} ORDER BY id;"""
        target_rows = f"""
            SELECT CONCAT(id,'|',CAST(amount AS CHAR),'|',
              CASE WHEN note IS NULL THEN '<NULL>' WHEN note='' THEN '<EMPTY>' ELSE note END,
              '|',sticky) FROM {table} ORDER BY id;"""
        steps = [
            tap(.5, "rail-tools"),
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
                             text=f"From Source PostgreSQL / datazen_e2e / {schema} / {table} to Target MySQL / datazen_e2e / {table}")
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


def with_sql_transfer_journeys(suite, configs):
    if {"mysql", "postgresql"} <= configs.keys():
        suite.test_postgresql_to_mysql_preserves_exact_reviewed_values = lambda self: self.run_sql_transfer_exact(configs)
        suite.test_postgresql_to_mysql_review_refuses_narrow_decimal = lambda self: self.run_sql_transfer_rejects_narrow(configs)
        suite.test_postgresql_to_mysql_review_refuses_nontransactional_table = lambda self: self.run_sql_transfer_rejects_nontransactional(configs)
