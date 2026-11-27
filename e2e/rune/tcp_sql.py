"""Opt-in native SQL journeys against disposable loopback database servers."""
import json
import os
from pathlib import Path
import subprocess
import uuid

from offline import fill, key
from pg_editor_context import register_pg_editor_context_cases
from scenarios import query_steps, tap, visible


class DisposableSql:
    def __init__(self, engine, config_path, *, alternate=False):
        self.engine = engine
        config = json.loads(config_path.read_text(encoding="utf-8"))
        primary = config["database"]
        if alternate and (engine != "postgresql" or config.get("alternateDatabase") != "datazen_e2e_alt"):
            raise ValueError("SQL E2E requires the allowlisted PostgreSQL alternate")
        self.database = config["alternateDatabase"] if alternate else primary
        self.user = config["user"]
        self.port = config["port"]
        self.password = config["password"]
        self.cli = str(Path(config["client"]).resolve(strict=True))
        if (config["host"] != "127.0.0.1" or primary != "datazen_e2e"
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

    def write_profile(self, journey, *, read_only=False, default_database=True):
        journey.write_tcp_profiles([{
            "id": "e2e-tcp-sql", "name": "Rune disposable SQL E2E",
            "databaseType": self.engine, "host": "127.0.0.1",
            **({"database": self.database} if default_database else {}),
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

    def run_mysql_editor_explain_without_writing(self, config):
        """Check the same retained Explain command against real MySQL."""
        database = DisposableSql("mysql", config)
        table = "aaa_rune_mysql_explain_" + uuid.uuid4().hex[:12]
        database.query(f"CREATE TABLE {table} (id INT PRIMARY KEY, label VARCHAR(80) NOT NULL);")
        self.addCleanup(database.query, f"DROP TABLE {table};")
        database.query(f"INSERT INTO {table} VALUES (1, 'keep first'), (2, 'keep second');")
        database.write_profile(self)
        updated = f"UPDATE {table} SET label='must not execute' WHERE id=1;"
        original = f"SELECT id, label FROM {table} ORDER BY id;"
        expected = database.query(original).splitlines()
        self.assertEqual(expected, ["1\tkeep first", "2\tkeep second"])
        server = database.query("EXPLAIN " + updated).split("\t")
        table_index = server.index(table)
        # In the classic MySQL plan, partitions and access type follow table.
        access_type = server[table_index + 2]
        self.assertNotIn(access_type, ("", "NULL"))

        actions = query_steps(updated, "e2e-tcp-sql")
        actions[-2] = {"t": 5, "type": "paste", "text": updated}
        actions[-1] = tap(8.5, "sql-explain")
        tree = self.replay(actions, until=12)
        self.expect_rendered(tree, key="query-explain-panel")
        self.expect_rendered(tree, key="query-explain-plan")
        self.assertFalse(visible(tree, key="query-explain-error"))
        lines = [node["text"] for node in tree["elements"]
                 if (node.get("key") or "").startswith("query-explain-plan-line-")]
        self.assertTrue(any(f"table: {table}" in line and f" | type: {access_type} |" in line
                            for line in lines))
        self.assertEqual(database.query(original).splitlines(), expected)

    def run_legacy_mysql_probe_authentication(self, config):
        """Migrate TC-CONN-007 using real authentication on the disposable MySQL fixture."""
        database = DisposableSql("mysql", config)
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        invalid = "incorrect-rune-probe-" + uuid.uuid4().hex
        base = [
            tap(.5, "connections-create-first"), tap(1.4, "connection-engine-option-mysql"),
            *fill(2.1, "connection-address-input", f"127.0.0.1:{database.port}"),
            *fill(3.4, "connection-database-input", database.database),
            *fill(4.8, "connection-username-input", database.user),
        ]

        def password(value):
            # Paste like an actual secure editor, but never print either value.
            return [tap(6, "connection-password-input"),
                    {"t": 6.4, "type": "paste", "text": value},
                    tap(7, "connection-test")]

        denied = self.replay([*base, *password(invalid)], until=13.5)
        self.expect_rendered(denied, text="Authentication failed. Check the username and password.",
                             within="connection-probe-status")
        self.expect_rendered(denied, key="connection-dialog")
        self.assertFalse(invalid in json.dumps(denied), "wrong password appeared in the app tree")
        self.assertFalse(self.store.exists())

        connected = self.replay([*base, *password(database.password)], until=13.5)
        self.expect_rendered(connected, text="Connection test succeeded.",
                             within="connection-probe-status")
        self.expect_rendered(connected, key="connection-dialog")
        self.assertFalse(database.password in json.dumps(connected), "password appeared in the app tree")
        self.assertFalse(self.store.exists())

    def run_legacy_postgres_affected_rows(self, config):
        """Migrate sql-query.ts SQ-012 against a real, disposable PostgreSQL server."""
        database = DisposableSql("postgresql", config)
        relation = f"public.aaa_rune_affected_{uuid.uuid4().hex[:12]}"
        database.query(f"CREATE TABLE {relation} (id INTEGER PRIMARY KEY, val TEXT NOT NULL);")
        self.addCleanup(database.query, f"DROP TABLE {relation};")
        database.query(f"INSERT INTO {relation} VALUES (1, 'first'), (2, 'second'), (3, 'untouched');")
        database.write_profile(self)
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")

        def execute(sql):
            actions = [
                tap(.5, "conn-open-e2e-tcp-sql"),
                tap(3.6, "conn-toolbar-new-query"),
                tap(4.5, "sql-editor-scroll"),
                {"t": 4.8, "type": "key", "name": "a", "cmd": True},
                {"t": 5.2, "type": "paste", "text": sql},
                tap(7, "sql-run"),
            ]
            return self.replay(actions, until=11)

        def check_outcome(tree, affected):
            self.expect_rendered(tree, key="execution-summary-card")
            self.expect_rendered(tree, text=f"{affected} rows affected", within="execution-summary-rows")
            duration = next((node.get("text", "") for node in tree["elements"]
                             if (node.get("text") or "").endswith(" ms")
                             and visible(tree, text=node["text"], within="execution-summary-elapsed")), "")
            self.assertRegex(duration, r"^\d+ ms$")
            self.expect_rendered(tree, text=duration, within="execution-summary-elapsed")
            self.assertFalse(visible(tree, key="result-grid"))

        def rows():
            return database.query(f"SELECT id, val FROM {relation} ORDER BY id;").splitlines()

        self.assertEqual(rows(), ["1|first", "2|second", "3|untouched"])
        changed = execute(f"UPDATE {relation} SET val='updated' WHERE id IN (1, 2);")
        check_outcome(changed, 2)
        expected = ["1|updated", "2|updated", "3|untouched"]
        self.assertEqual(rows(), expected)

        unchanged = execute(f"UPDATE {relation} SET val='unexpected' WHERE id=-1;")
        check_outcome(unchanged, 0)
        self.assertEqual(rows(), expected)

        no_results = execute(f"SELECT val FROM {relation} WHERE id=-1;")
        self.assertFalse(visible(no_results, key="execution-summary-rows"))
        self.expect_rendered(no_results, text="val", within="result-data-grid-header")
        self.assertEqual(rows(), expected)

    def run_legacy_postgres_limit_preserves_authored_query(self, config):
        """Migrate QLIMIT-003/004 for authored PostgreSQL FETCH/LIMIT/OFFSET."""
        db = DisposableSql("postgresql", config)
        name = "aaa_rune_query_limit_" + uuid.uuid4().hex[:12]
        table = f"public.{name}"
        db.query(f"CREATE TABLE {table} (id INT PRIMARY KEY, score INT NOT NULL);")
        self.addCleanup(db.query, f"DROP TABLE {table};")
        db.query(f"INSERT INTO {table} VALUES (1,10),(2,10),(3,10),(4,20),(5,30),(6,40);")
        snapshot = f"SELECT id, score FROM {table} ORDER BY id;"
        initial = db.query(snapshot)
        self.assertEqual(initial.splitlines(), ["1|10", "2|10", "3|10", "4|20", "5|30", "6|40"])
        db.write_profile(self)
        cap = 2
        settings = self.root / "settings.json"
        settings.write_text(json.dumps({"ui": {"language": "en", "queryMaxRows": cap}}),
                            encoding="utf-8")

        sample = (
            # Same unique ordered visible prefix, distinct true overflow.
            ("equal FETCH FIRST", f"SELECT id AS probe FROM {table} ORDER BY id FETCH FIRST 2 ROWS ONLY;",
             ["1", "2"]),
            ("adjacent overflowing FETCH FIRST", f"SELECT id AS probe FROM {table} ORDER BY id FETCH FIRST 3 ROWS ONLY;",
             ["1", "2", "3"]),
            ("authored LIMIT and OFFSET", f"SELECT id AS probe FROM {table} ORDER BY id LIMIT 1 OFFSET 2;", ["3"]),
            ("authored OFFSET/FETCH", f"SELECT id AS probe FROM {table} ORDER BY id OFFSET 4 ROWS FETCH NEXT 2 ROWS ONLY;", ["5", "6"]),
            ("WITH TIES", f"SELECT score AS probe FROM {table} ORDER BY score FETCH FIRST 1 ROW WITH TIES;", ["10", "10", "10"]),
            ("LIMIT only in nested SELECT",
             f"SELECT id AS probe FROM (SELECT id FROM {table} ORDER BY id DESC LIMIT 3) s ORDER BY id;",
             ["4", "5", "6"]),
            ("authored top-level LIMIT above UI cap", f"SELECT id AS probe FROM {table} ORDER BY id LIMIT 5;",
             ["1", "2", "3", "4", "5"]),
        )
        for name, sql, reference in sample:
            with self.subTest(clause=name):
                independently_returned = db.query(sql).splitlines()
                self.assertEqual(independently_returned, reference)
                kept = independently_returned[:cap]
                actions = query_steps(sql, "e2e-tcp-sql")
                actions[-2] = {"t": 5.2, "type": "paste", "text": sql}
                tree = self.replay(actions, until=11.8)
                self.assertFalse(visible(tree, key="query-error-message"))
                self.expect_rendered(tree, text="probe", within="result-data-grid-header")
                self.expect_rendered(tree, key="result-grid-status",
                                     text=f"result · {len(kept)} rows × 1 cols")
                for index, value in enumerate(kept):
                    self.expect_rendered(tree, text=value, within=f"result-cell-{index}-0")
                self.assertFalse(visible(tree, key=f"result-cell-{len(kept)}-0"))
                noticed = visible(tree, key="result-truncated", within="result-workspace")
                self.assertEqual(noticed, len(independently_returned) > cap)
                if noticed:
                    self.assertTrue(any(str(cap) in node.get("text", "") for node in tree["elements"]
                                        if node.get("key") == "result-truncated"))
        self.assertEqual(db.query(snapshot), initial)
        histories = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
        self.assertEqual([(r["sql"].rstrip(";"), r["success"]) for r in histories],
                         [(sql.rstrip(";"), True) for _name, sql, _reference in sample])

    def run_legacy_postgres_stream_result_status(self, config):
        """Migrate sql-query.ts SQ-002/SQ-004 and the over-one-batch result."""
        database = DisposableSql("postgresql", config)
        database.write_profile(self)
        base = ("SELECT n, 'legacy-pg-batch-' || n AS batch_marker "
                "FROM generate_series(1, 600) AS generated(n)")
        expected = database.query(base + " ORDER BY n;").splitlines()
        self.assertEqual((len(expected), expected[0], expected[-1]),
                         (600, "1|legacy-pg-batch-1", "600|legacy-pg-batch-600"))
        marker = expected[-1].partition("|")[2]
        sql = base + " CROSS JOIN (SELECT pg_sleep(0.075)) AS delay ORDER BY n;"
        actions = query_steps(sql, "e2e-tcp-sql")
        actions[-2] = {"t": 5, "type": "paste", "text": sql}
        tree = self.replay([
            *actions, *fill(11.0, "result-find-input", marker),
            tap(12.5, "result-find-apply"),
        ], until=14.2)

        self.expect_rendered(tree, key="result-grid-status", text="result · 600 rows × 2 cols")
        self.expect_rendered(tree, text="batch_marker", within="result-data-grid-header")
        self.expect_rendered(tree, key="result-find-count", text="1 of 1")
        selection = next(node["text"] for node in tree["elements"]
                         if node.get("key") == "result-status-text")
        self.assertTrue(selection.endswith(" · 600 rows × 2 cols · selection r599c1..r599c1"))
        duration = next((node.get("text", "") for node in tree["elements"]
                         if node.get("key") == "query-result-duration"), "")
        self.assertRegex(duration, r"^\d+ ms$")
        # Do not accept a fabricated zero duration for a server query that really waits.
        self.assertGreaterEqual(int(duration.split()[0]), 75)
        self.expect_rendered(tree, key="query-result-duration", text=duration)
        self.assertFalse(visible(tree, key="execution-summary-rows"))

    def run_legacy_postgres_select_script(self, config):
        """Migrate PostgreSQL SQ-011 using the real ordered result artifacts."""
        database = DisposableSql("postgresql", config)
        before = "SELECT $str$first;result$str$ AS first_value"
        after = "SELECT 29 AS second_value"
        empty = "SELECT 77 AS empty_value WHERE FALSE"
        self.assertEqual([database.query(sql) for sql in (before, after, empty)],
                         ["first;result", "29", ""])
        database.write_profile(self)

        def execute(sql):
            actions = [
                tap(.5, "conn-open-e2e-tcp-sql"),
                tap(3.4, "conn-toolbar-new-query"),
                tap(4.5, "sql-editor-scroll"),
                {"t": 4.8, "type": "key", "name": "a", "cmd": True},
                {"t": 5.2, "type": "paste", "text": sql},
                tap(7, "sql-run-script"),
            ]
            return actions

        actions = execute(";\n".join((before, after, empty)) + ";")
        labels = ("first_value", "second_value", "empty_value")
        values = ("first;result", "29", None)

        def expect_page(tree, index):
            for ident in (1, 2, 3):
                self.expect_rendered(tree, key=f"query-result-tab-{ident}")
            self.expect_rendered(tree, text=labels[index], within="result-data-grid-header")
            for peer in range(len(labels)):
                if peer != index:
                    self.assertFalse(visible(tree, text=labels[peer], within="result-data-grid-header"))
            if values[index] is None:
                self.expect_rendered(tree, key="result-grid-status", text="result · 0 rows × 1 cols")
                self.assertFalse(visible(tree, key="result-cell-0-0"))
            else:
                self.expect_rendered(tree, text=values[index], within="result-cell-0-0")
                self.expect_rendered(tree, key="result-grid-status", text="result · 1 rows × 1 cols")

        expect_page(self.replay(actions, until=11), 0)
        second = [*actions, tap(10, "query-result-label-2")]
        expect_page(self.replay(second, until=11.5), 1)
        third = [*second, tap(11.5, "query-result-label-3")]
        expect_page(self.replay(third, until=12.7), 2)
        expect_page(self.replay([*third, tap(12.8, "query-result-label-1")], until=14), 0)

        table = "aaa_rune_script_rejected_" + uuid.uuid4().hex[:12]
        self.addCleanup(database.query, f"DROP TABLE IF EXISTS public.{table}")
        mixed = before + f"; SELECT 1 INTO public.{table};"
        blocked = self.replay(execute(mixed), until=10)
        self.expect_rendered(blocked,
                             text="The PostgreSQL script may contain only non-mutating SELECT statements.",
                             within="query-notice")
        self.assertFalse(visible(blocked, key="result-workspace"))
        self.assertEqual(database.query(f"SELECT to_regclass('public.{table}') IS NULL;"), "t")

    def run_legacy_postgres_query_error_and_recovery(self, config):
        """Migrate PostgreSQL SQ-002's live error and copy feedback."""
        database = DisposableSql("postgresql", config)
        database.write_profile(self)
        (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
        missing = f"aaa_rune_missing_{uuid.uuid4().hex[:12]}"
        self.assertEqual(database.query(f"SELECT to_regclass('public.{missing}') IS NULL;"), "t")
        error = "The database rejected the SQL statement."
        bad = f"SELECT * FROM public.{missing};"
        actions = query_steps(bad, "e2e-tcp-sql")
        actions[-2] = {"t": 5, "type": "paste", "text": bad}
        captured = self.replay(actions, until=10.5)
        self.expect_rendered(captured, text=error, key="query-error-message-text")
        self.expect_rendered(captured, text="Copy", within="query-error-message-copy")
        self.assertFalse(visible(captured, key="result-grid"))

        copied = [*actions, tap(10, "query-error-message-copy")]
        self.expect_rendered(self.replay(copied, until=10.7),
                             text="Copied", within="query-error-message-copy")
        self.expect_rendered(self.replay(copied, until=12.5),
                             text="Copy", within="query-error-message-copy")

        fixed = "SELECT 1703 AS corrected_value;"
        recovered = self.replay([
            *actions, tap(10.6, "sql-editor-scroll"),
            {"t": 11, "type": "key", "name": "a", "cmd": True},
            {"t": 11.5, "type": "paste", "text": fixed},
            tap(12, "sql-run"),
        ], until=14)
        self.expect_rendered(recovered, text="corrected_value", within="result-data-grid-header")
        self.expect_rendered(recovered, text=database.query(fixed), within="result-cell-0-0")
        self.assertFalse(visible(recovered, key="query-error-message"))

    def run_legacy_postgres_selected_sql(self, config, *, shortcut):
        """Migrate SQ-013 and SQ-014 with the real toolbar and keyboard paths."""
        database = DisposableSql("postgresql", config)
        database.write_profile(self)
        relation = f"public.aaa_rune_selected_{uuid.uuid4().hex[:12]}"
        database.query(f"CREATE TABLE {relation} (id INTEGER PRIMARY KEY, label TEXT NOT NULL);")
        self.addCleanup(database.query, f"DROP TABLE {relation};")
        database.query(f"INSERT INTO {relation} VALUES (1, 'unchanged');")
        snapshot = f"SELECT id, label FROM {relation} ORDER BY id;"
        old = database.query(snapshot)
        self.assertEqual(old, "1|unchanged")

        payload = "键盘;选中" if shortcut else "按钮;选中"
        fragment = f"SELECT '{payload}'::text AS picked_only"
        sql = (f"UPDATE {relation} SET label='outside-selection' WHERE id=1;\n"
               + fragment + " UNION ALL SELECT 'unselected sibling';")
        expected = database.query(fragment)
        self.assertEqual(expected, payload)
        editor = query_steps(sql, "e2e-tcp-sql")[:-1]
        editor[-1] = {"t": 5, "type": "paste", "text": sql}
        invoke = key(12, "Enter", cmd=True) if shortcut else tap(12, "sql-run")
        tree = self.replay([
            *editor, tap(8.0, "sql-find-open"), *fill(8.5, "sql-find-input", fragment),
            tap(10.8, "sql-find-next"), key(11.3, "Escape"), invoke,
        ], until=15)
        self.expect_rendered(tree, text="picked_only", within="result-data-grid-header")
        self.expect_rendered(tree, text=expected, within="result-cell-0-0")
        self.expect_rendered(tree, key="result-grid-status", text="result · 1 rows × 1 cols")
        self.assertFalse(visible(tree, text="unselected sibling", within="result-workspace"))
        self.assertEqual(database.query(snapshot), old)

    def run_legacy_postgres_stop_query(self, config):
        """Migrate SQ-001: Stop cancels the real active statement without closing the editor."""
        from concurrent.futures import ThreadPoolExecutor
        from time import monotonic, sleep

        database = DisposableSql("postgresql", config)
        database.write_profile(self)
        marker = f"rune_toolbar_stop_{uuid.uuid4().hex}"

        def predicate(tag):
            return (f"pid <> pg_backend_pid() AND state='active' "
                    f"AND datname=current_database() AND query LIKE '%/* {tag} */%'")

        def run(tag):
            statement = f"SELECT pg_sleep(60) /* {tag} */;"
            steps = query_steps(statement, "e2e-tcp-sql")
            steps[-2] = {"t": 5, "type": "paste", "text": statement}
            return steps

        preview_tag = marker + "_preview"
        for tag in (preview_tag, marker):
            self.addCleanup(database.query, "SELECT pg_cancel_backend(pid) FROM pg_stat_activity WHERE "
                            + predicate(tag) + ";")

        preview = self.replay(run(preview_tag), until=10.4)
        self.expect_rendered(preview, key="sql-cancel")
        self.expect_rendered(preview, key="query-loading")
        self.expect_rendered(preview, text="Command+Enter runs the statement at the cursor",
                             within="query-main")
        database.query("SELECT pg_cancel_backend(pid) FROM pg_stat_activity WHERE "
                       + predicate(preview_tag) + ";")

        follow_up = "SELECT 151 AS after_stop;"
        actions = [
            *run(marker), tap(11.2, "sql-cancel"),
            tap(14, "sql-editor-scroll"), key(14.3, "a", cmd=True),
            {"t": 14.7, "type": "paste", "text": follow_up}, tap(15.6, "sql-run"),
        ]

        active_since = None
        stopped_before_replay_ended = False
        with ThreadPoolExecutor(max_workers=1) as pool:
            replay = pool.submit(self.replay, actions, until=18.3)
            deadline = monotonic() + 58
            while not replay.done() and monotonic() < deadline:
                seen = database.query("SELECT pid FROM pg_stat_activity WHERE " + predicate(marker) + ";")
                if seen and active_since is None:
                    active_since = monotonic()
                elif not seen and active_since is not None:
                    # Ended before the replay closed the window and well before
                    # a naturally completed 60-second query could fool the test.
                    stopped_before_replay_ended = not replay.done() and monotonic() - active_since < 35
                    break
                sleep(.13)
            final = replay.result()

        self.assertIsNotNone(active_since, "the original statement never started on PostgreSQL")
        self.assertTrue(stopped_before_replay_ended, "Stop did not end the active statement before teardown")
        self.expect_rendered(final, key="tab-1")
        self.assertFalse(visible(final, key="sql-cancel"))
        self.assertFalse(visible(final, key="query-loading"))
        self.assertFalse(visible(final, key="running-close-dialog"))
        self.expect_rendered(final, text="after_stop", within="result-data-grid-header")
        self.expect_rendered(final, text=database.query(follow_up), within="result-cell-0-0")

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
            tap(13.5, "sql-run"), tap(15.4, "query-library-history-toggle"),
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
                  tap(5.0, "query-library-history-toggle")]

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
        if engine == "mysql":
            def probe_authentication(self, config=config):
                return self.run_legacy_mysql_probe_authentication(config)

            setattr(suite, "test_mysql_legacy_connection_probe_authentication", probe_authentication)

            def explain(self, config=config):
                return self.run_mysql_editor_explain_without_writing(config)

            setattr(suite, "test_mysql_editor_explain_without_writing", explain)
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

            def affected_rows(self, config=config):
                return self.run_legacy_postgres_affected_rows(config)

            setattr(suite, "test_postgresql_legacy_dml_reports_affected_rows", affected_rows)

            def stream_result_status(self, config=config):
                return self.run_legacy_postgres_stream_result_status(config)

            setattr(suite, "test_postgresql_legacy_stream_result_status", stream_result_status)

            def authored_limit(self, config=config):
                return self.run_legacy_postgres_limit_preserves_authored_query(config)

            setattr(suite, "test_postgresql_legacy_query_limit_preserves_authored_fetch_and_offset", authored_limit)

            def select_script(self, config=config):
                return self.run_legacy_postgres_select_script(config)

            setattr(suite, "test_postgresql_legacy_select_script_result_tabs", select_script)

            def error_and_recovery(self, config=config):
                return self.run_legacy_postgres_query_error_and_recovery(config)

            setattr(suite, "test_postgresql_legacy_query_error_and_recovery", error_and_recovery)

            def selected_by_button(self, config=config):
                return self.run_legacy_postgres_selected_sql(config, shortcut=False)

            def selected_by_keyboard(self, config=config):
                return self.run_legacy_postgres_selected_sql(config, shortcut=True)

            setattr(suite, "test_postgresql_legacy_selected_sql_button", selected_by_button)
            setattr(suite, "test_postgresql_legacy_selected_sql_keyboard", selected_by_keyboard)

            def stop_query(self, config=config):
                return self.run_legacy_postgres_stop_query(config)

            setattr(suite, "test_postgresql_legacy_stop_query_keeps_editor_open", stop_query)

            register_pg_editor_context_cases(suite, config, DisposableSql)

            def collision(self, config=config):
                return self.run_postgres_collision(config)

            setattr(suite, "test_postgresql_refuses_ambiguous_namespace", collision)
