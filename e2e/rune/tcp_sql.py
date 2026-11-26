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

    def run_legacy_history_scope_and_search(self, config):
        """Migrate e2e/specs/query-history.ts QH-001 through QH-005."""
        database = DisposableSql("postgresql", config)
        self.assertEqual(database.query("SELECT current_database();"), database.database)
        database.write_profile(self)
        first = "SELECT current_database(), 'qh_marker_a';"
        second = "SELECT current_database(), 'qh_marker_b';"
        executed = self.replay([
            *query_steps(first, "e2e-tcp-sql"), *fill(10.2, "sql-editor-scroll", second),
            tap(13.5, "sql-run"), tap(15.4, "query-sidebar-tab-history"),
        ], until=18)
        self.expect_rendered(executed, text="qh_marker_b", within="result-workspace")
        path = self.root / "query-history.json"
        persisted = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(
            [(r["connectionId"], r["database"], r["sql"].rstrip(";"), r["success"])
             for r in persisted],
            [("e2e-tcp-sql", database.database, sql.rstrip(";"), True) for sql in (first, second)],
        )

        panel = "query-history-panel"
        current, all_db = "history-scope-current", "history-scope-all"
        selector = "query-history-search"
        opened = [tap(.5, "conn-open-e2e-tcp-sql"), tap(3.8, "conn-toolbar-new-query"),
                  tap(5.0, "query-sidebar-tab-history")]

        def selected(root, control):
            def find(node):
                if node.get("key") == control:
                    return node
                for child in node.get("children", ()):
                    match = find(child)
                    if match is not None:
                        return match
                return None
            node = find(root["accessibility"])
            self.assertIsNotNone(node, control)
            return "checked" in node.get("states", ())

        def markers(tree):
            return tuple(visible(tree, text=sql.rstrip(";"), within=panel)
                         for sql in (first, second))

        default = self.replay(opened, until=6.7)
        self.expect_rendered(default, key=current, within=panel)
        self.expect_rendered(default, key=all_db, within=panel)
        self.assertTrue(selected(default, current))
        self.assertFalse(selected(default, all_db))
        self.expect_rendered(default, key="history-current-db", within=panel)
        self.assertFalse(visible(default, key="history-scope-fallback-hint", within=panel))
        self.assertFalse(visible(default, key="history-group-label", within=panel))
        self.assertEqual(markers(default), (True, True))

        choose_all = [*opened, tap(6.8, all_db)]
        all_view = self.replay(choose_all, until=8.0)
        self.assertTrue(selected(all_view, all_db))
        self.expect_rendered(all_view, key="history-group-label", text=database.database, within=panel)
        self.assertEqual(markers(all_view), (True, True))

        matched = [*choose_all, *fill(8.0, selector, "QH_MARKER_B")]
        one = self.replay(matched, until=10.2)
        self.assertEqual(markers(one), (False, True))
        no_match = [*matched, *fill(10.4, selector, "__no_such_marker__")]
        empty = self.replay(no_match, until=12.6)
        self.expect_rendered(empty, key="query-no-history-match", within=panel)
        self.assertEqual(markers(empty), (False, False))

        restored = self.replay([
            *no_match, tap(12.8, selector), key(13.0, "a", cmd=True), key(13.3, "Backspace"),
            tap(14.1, current),
        ], until=15.6)
        self.assertTrue(selected(restored, current))
        self.expect_rendered(restored, key="history-current-db", within=panel)
        self.assertFalse(visible(restored, key="history-group-label", within=panel))
        self.assertEqual(markers(restored), (True, True))
        self.assertEqual(json.loads(path.read_text(encoding="utf-8")), persisted)

    def run_legacy_close_running_query(self, config, *, confirm):
        """Real server observation for unified-tab-bar.ts UTB-007 and UTB-008."""
        database = DisposableSql("postgresql", config)
        database.write_profile(self)
        tag = f"rune_tab_close_{uuid.uuid4().hex}"
        # Observe the identified statement independently of the Rune host.
        from concurrent.futures import ThreadPoolExecutor
        from time import monotonic, sleep

        predicate = (f"pid <> pg_backend_pid() AND state = 'active' "
                     f"AND datname = current_database() AND query LIKE '%/* {tag} */%'")
        self.addCleanup(database.query, f"SELECT pg_cancel_backend(pid) FROM pg_stat_activity WHERE {predicate};")
        def opening(marker):
            # Leave headroom for software-rendered replays on shared CI.
            # The observer still requires the explicit stop well before expiry.
            statement = f"SELECT pg_sleep(60) /* {marker} */;"
            run = query_steps(statement, "e2e-tcp-sql")
            run[-2] = {"t": 5, "type": "paste", "text": statement}
            return [*run, tap(11.3, "tab-close-1")]

        if confirm:
            preview_tag = f"{tag}_preview"
            self.addCleanup(database.query, f"""
                SELECT pg_cancel_backend(pid) FROM pg_stat_activity
                WHERE pid <> pg_backend_pid() AND state = 'active'
                  AND datname = current_database() AND query LIKE '%/* {preview_tag} */%';
            """)
            prompt = self.replay(opening(preview_tag), until=13.0)
            self.expect_rendered(prompt, key="tab-1")
            self.expect_rendered(prompt, key="sql-cancel")
            self.expect_rendered(prompt, key="query-loading")
            self.expect_rendered(prompt, key="running-close-dialog")
            self.expect_rendered(prompt, key="confirm-dialog-ok")
        actions = [*opening(tag),
                   tap(14.4, "confirm-dialog-ok" if confirm else "confirm-dialog-cancel")]
        observations = []
        with ThreadPoolExecutor(max_workers=1) as pool:
            replay = pool.submit(self.replay, actions, until=17.5)
            deadline = monotonic() + 65
            while not replay.done() and monotonic() < deadline:
                seen = database.query(f"SELECT pid FROM pg_stat_activity WHERE {predicate};")
                observations.append((monotonic(), seen))
                sleep(.25)
            tree = replay.result()
        active = [(at, pid) for at, pid in observations if pid]
        self.assertTrue(active, "the server never observed the identified live statement")
        pids = {pid for _, pid in active}
        self.assertEqual(len(pids), 1, "one opened query must keep one backend identity")
        self.assertFalse(visible(tree, key="confirm-dialog-ok"))
        if confirm:
            self.assertFalse(visible(tree, key="tab-1"))
            self.assertFalse(visible(tree, key="sql-cancel"))
            self.expect_rendered(tree, key="workspace-empty")
            stopped = [at for at, pid in observations if not pid and at > active[-1][0]]
            self.assertTrue(stopped, "the server statement did not stop while the native window was alive")
            self.assertLess(stopped[0] - active[0][0], 50, "natural sleep expiry must not count as a stop")
        else:
            self.expect_rendered(tree, key="tab-1")
            self.expect_rendered(tree, key="sql-cancel")
            self.expect_rendered(tree, key="query-loading")
            self.assertTrue(all(pid for at, pid in observations
                                if active[0][0] <= at < active[-1][0]))
            self.assertLess(observations[-1][0] - active[-1][0], 1.25,
                            "declining must preserve the running backend until replay teardown")
            self.assertGreaterEqual(active[-1][0] - active[0][0], 4,
                                    "the backend must still be active through the prompt and decline")


def with_tcp_journeys(suite, configs):
    # Do not discover unprovisioned engines as skipped/pass. Opt-in fixtures
    # become named tests and failures are never silently ignored.
    for engine, config in configs.items():
        def journey(self, engine=engine, config=config):
            return self.run_server_journey(engine, config)

        setattr(suite, f"test_{engine}_native_catalog_query_edit", journey)
        if engine == "postgresql":
            def confirm_close(self, config=config):
                return self.run_legacy_close_running_query(config, confirm=True)

            def decline_close(self, config=config):
                return self.run_legacy_close_running_query(config, confirm=False)

            setattr(suite, "test_postgresql_confirm_tab_close_stops_exact_running_query", confirm_close)
            setattr(suite, "test_postgresql_decline_tab_close_keeps_exact_running_query", decline_close)

            def history_scope(self, config=config):
                return self.run_legacy_history_scope_and_search(config)

            setattr(suite, "test_postgresql_legacy_history_scope_and_search", history_scope)

            def collision(self, config=config):
                return self.run_postgres_collision(config)

            setattr(suite, "test_postgresql_refuses_ambiguous_namespace", collision)
