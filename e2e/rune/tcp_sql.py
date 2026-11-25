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

    def write_profile(self, file, *, read_only=False):
        # TempDirectory confines this legacy plaintext profile to its owner.
        # The suite never copies it into artifacts.
        fd = os.open(file, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, mode=0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as store:
            json.dump({"connections": [{
                "id": "e2e-tcp-sql", "name": "Rune disposable SQL E2E",
                "databaseType": self.engine, "database": self.database, "host": "127.0.0.1",
                "port": self.port, "username": self.user, "password": self.password,
                "readOnly": read_only, "connectionTimeout": 8, "maxPoolSize": 4,
            }]}, store)


class TcpSqlJourneys:
    def run_server_journey(self, engine, config):
        database = DisposableSql(engine, config)
        table = f"aaa_rune_e2e_{uuid.uuid4().hex[:12]}"
        database.query(f"CREATE TABLE {table} (id INTEGER PRIMARY KEY, label VARCHAR(80) NOT NULL);")
        self.addCleanup(database.query, f"DROP TABLE {table};")
        database.query(f"INSERT INTO {table} VALUES (1, 'server-before'), (2, 'server-control');")
        database.write_profile(self.store)
        browse = [tap(.5, "conn-open-e2e-tcp-sql"), tap(4, f"tree-table-{table}")]
        catalog = self.replay(browse, until=6)
        self.expect_rendered(catalog, key=f"tree-table-{table}")
        self.expect_rendered(catalog, text="server-before", within="cell-0-1")
        self.expect_rendered(catalog, text="server-control", within="cell-1-1")

        sql = f"SELECT label FROM {table} WHERE id=2;"
        result = self.replay(query_steps(sql, "e2e-tcp-sql"), until=11.5)
        self.expect_rendered(result, text=database.query(sql), within="result-workspace")
        self.assertFalse(visible(result, text="server-before", within="result-workspace"))

        edited = self.replay([
            *browse, tap(6.5, "cell-0-1"), key(6.85, "Enter"),
            *fill(7.2, "grid-editor", "server-after"), key(8.6, "Enter"),
            tap(9.2, "commit-staged"),
        ], until=12)
        self.expect_rendered(edited, text="server-after", within="cell-0-1")
        self.assertEqual(
            database.query(f"SELECT label FROM {table} ORDER BY id;").splitlines(),
            ["server-after", "server-control"],
        )


def with_tcp_journeys(suite, configs):
    # Do not discover unprovisioned engines as skipped/pass. Opt-in fixtures
    # become named tests and failures are never silently ignored.
    for engine, config in configs.items():
        def journey(self, engine=engine, config=config):
            return self.run_server_journey(engine, config)

        setattr(suite, f"test_{engine}_native_catalog_query_edit", journey)
