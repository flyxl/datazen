"""Opt-in native SQL journeys against disposable loopback database servers."""
import json
import os
from pathlib import Path
import subprocess
import uuid

from offline import fill, key
from scenarios import query_steps, tap, visible


class DisposableSql:
    def __init__(self, engine, config_path):
        self.engine = engine
        config = json.loads(config_path.read_text(encoding="utf-8"))
        self.database = config["database"]
        self.user = config["user"]
        self.port = config["port"]
        self.password = config["password"]
        self.cli = str(Path(config["client"]).resolve(strict=True))
        if (config["host"] != "127.0.0.1" or self.database != "datazen_e2e"
                or type(self.port) is not int or not 1 <= self.port <= 65535):
            raise ValueError("SQL E2E requires a private 127.0.0.1 datazen_e2e server")
        marker = self.query("SELECT marker FROM rune_e2e_fixture;")
        if marker != "datazen-rune-disposable":
            raise ValueError("SQL E2E refused: disposable database marker is absent or invalid")

    def query(self, sql):
        env = {**os.environ}
        if self.engine == "postgresql":
            env["PGPASSWORD"] = self.password
            args = [self.cli, "-X", "-qAt", "-w", "-v", "ON_ERROR_STOP=1",
                    "-h", "127.0.0.1", "-p", str(self.port), "-U", self.user, "-d", self.database]
        else:
            env["MYSQL_PWD"] = self.password
            args = [self.cli, "--no-defaults", "--protocol=TCP", "--batch", "--skip-column-names",
                    "-h", "127.0.0.1", "-P", str(self.port), "-u", self.user, self.database]
        result = subprocess.run(args, env=env, input=sql, text=True, capture_output=True, timeout=10)
        if result.returncode:
            # Do not copy connection diagnostics or passwords into artifacts.
            raise AssertionError(f"{self.engine} independent fixture query failed (exit {result.returncode})")
        return result.stdout.strip()

    def write_profile(self, journey, *, read_only=False):
        journey.write_tcp_profiles([{
            "id": "e2e-tcp-sql", "name": "Rune disposable SQL E2E",
            "databaseType": self.engine, "database": self.database, "host": "127.0.0.1",
            "port": self.port, "username": self.user, "password": self.password,
            "readOnly": read_only, "connectionTimeout": 8, "maxPoolSize": 4,
        }])


class TcpSqlJourneys:
    def run_server_journey(self, engine, config):
        database = DisposableSql(engine, config)
        tag = uuid.uuid4().hex[:12]
        table = f"aaa_rune_e2e_{tag}"
        relation = table
        if engine == "postgresql":
            schema = f"aaa_rune_e2e_{tag}"
            database.query(f"CREATE SCHEMA {schema};")
            self.addCleanup(database.query, f"DROP SCHEMA {schema};")
            relation = f"{schema}.{table}"
        database.query(f"CREATE TABLE {relation} (id INTEGER PRIMARY KEY, label VARCHAR(80) NOT NULL);")
        self.addCleanup(database.query, f"DROP TABLE {relation};")
        database.query(f"INSERT INTO {relation} VALUES (1, 'server-before'), (2, 'server-control');")
        database.write_profile(self)
        browse = [tap(.5, "conn-open-e2e-tcp-sql"), tap(4, f"tree-table-{table}")]
        catalog = self.replay(browse, until=6)
        self.expect_rendered(catalog, key=f"tree-table-{table}")
        self.expect_rendered(catalog, text="server-before", within="cell-0-1")
        self.expect_rendered(catalog, text="server-control", within="cell-1-1")

        sql = f"SELECT label FROM {relation} WHERE id=2;"
        # Use a paste event instead of racing the typing timer for qualified
        # names. The real SQL editor handles it like interactive clipboard input.
        actions = query_steps(sql, "e2e-tcp-sql")
        actions[-2] = {"t": 5, "type": "paste", "text": sql}
        result = self.replay(actions, until=11.5)
        self.expect_rendered(result, text=database.query(sql), within="result-workspace")
        self.assertFalse(visible(result, text="server-before", within="result-workspace"))

        edited = self.replay([
            *browse, tap(6.5, "cell-0-1"), key(6.85, "Enter"),
            *fill(7.2, "grid-editor", "server-after"), key(8.6, "Enter"),
            tap(9.2, "commit-staged"),
        ], until=12)
        self.expect_rendered(edited, text="server-after", within="cell-0-1")
        self.assertEqual(
            database.query(f"SELECT label FROM {relation} ORDER BY id;").splitlines(),
            ["server-after", "server-control"],
        )

        database.write_profile(self, read_only=True)
        locked = self.replay([*browse, tap(6.5, "cell-0-1"), key(6.85, "Enter")], until=8)
        self.expect_rendered(locked, text="server-after", within="cell-0-1")
        self.assertFalse(visible(locked, key="grid-editor"))
        self.assertEqual(
            database.query(f"SELECT label FROM {relation} ORDER BY id;").splitlines(),
            ["server-after", "server-control"],
        )

    def run_postgres_collision(self, config):
        database = DisposableSql("postgresql", config)
        tag = uuid.uuid4().hex[:12]
        schema = f"aaa_rune_alt_{tag}"
        table = f"aaa_rune_collision_{tag}"
        database.query(f"CREATE SCHEMA {schema};")
        self.addCleanup(database.query, f"DROP SCHEMA {schema};")
        for relation, value in ((f"{schema}.{table}", "schema-specific"), (f"public.{table}", "public-specific")):
            database.query(f"CREATE TABLE {relation} (id INTEGER PRIMARY KEY, label VARCHAR(80) NOT NULL);")
            self.addCleanup(database.query, f"DROP TABLE {relation};")
            database.query(f"INSERT INTO {relation} VALUES (1, '{value}');")
        database.write_profile(self)
        tree = self.replay([tap(.5, "conn-open-e2e-tcp-sql"), tap(4, f"tree-table-{table}")],
                           until=6.5)
        self.expect_rendered(tree, text="This table name is ambiguous across namespaces.",
                             within="table-read-error")
        self.assertFalse(visible(tree, text="schema-specific"))
        self.assertFalse(visible(tree, text="public-specific"))
        self.assertFalse(visible(tree, key="commit-staged"))
        self.assertEqual(database.query(f"SELECT label FROM {schema}.{table};"), "schema-specific")
        self.assertEqual(database.query(f"SELECT label FROM public.{table};"), "public-specific")


def with_tcp_journeys(suite, configs):
    # Do not discover unprovisioned engines as skipped/pass. Opt-in fixtures
    # become named tests and failures are never silently ignored.
    for engine, config in configs.items():
        def journey(self, engine=engine, config=config):
            return self.run_server_journey(engine, config)

        setattr(suite, f"test_{engine}_native_catalog_query_edit", journey)
        if engine == "postgresql":
            def collision(self, config=config):
                return self.run_postgres_collision(config)

            setattr(suite, "test_postgresql_refuses_ambiguous_namespace", collision)
