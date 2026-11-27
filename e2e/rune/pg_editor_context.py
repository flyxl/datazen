"""Native PostgreSQL editor routing and connection lifecycle E2E."""
from concurrent.futures import ThreadPoolExecutor
import json
from time import monotonic, sleep
import uuid

from offline import key
from scenarios import tap, type_text, visible


def run_legacy_postgres_editor_database_context(self, primary, alternate):
    """Migrate SQ-CTX-001/002 with a marker-verified pair of real databases."""
    ordered = primary.query("SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname;").splitlines()
    self.assertEqual(ordered[0], primary.database)
    self.assertIn(alternate.database, ordered)
    relation = "public.aaa_rune_context_" + uuid.uuid4().hex[:12]
    table = {primary.database: "primary marker", alternate.database: "alternate marker"}
    for connection in (primary, alternate):
        connection.query(f"CREATE TABLE {relation} (marker TEXT NOT NULL);")
        self.addCleanup(connection.query, f"DROP TABLE {relation};")
        connection.query(f"INSERT INTO {relation} VALUES ('{table[connection.database]}');")
        self.assertEqual(connection.query(f"SELECT current_database(), marker FROM {relation};"),
                         connection.database + "|" + table[connection.database])
    (self.root / "settings.json").write_text('{"ui":{"language":"en"}}', encoding="utf-8")
    primary.write_profile(self, default_database=False)
    trigger = "query-context-database-trigger"
    opened = [tap(.5, "conn-open-e2e-tcp-sql"), tap(5, "conn-toolbar-new-query")]
    menu = self.replay([*opened, tap(6.5, trigger)], until=8.0)
    self.expect_rendered(menu, text=primary.database, within=trigger)
    self.expect_rendered(menu, key="sql-run")
    for database in table:
        self.expect_rendered(menu, text=database, within=f"option{ordered.index(database)}")
    controls = {n["key"]: n["rect"] for n in menu["elements"] if n.get("key") in (trigger, "sql-run")}
    tx, ty, tw, th = controls[trigger]
    rx, ry, rw, rh = controls["sql-run"]
    self.assertGreater(tw, 100.0)
    self.assertTrue(tx >= rx + rw or rx >= tx + tw, "database selector covers the Run button")

    def sql_guarded(expected_db):
        expected_marker = table[expected_db]
        return (f"SELECT current_database() AS actual_database, marker, "
                f"1 / (CASE WHEN current_database()='{expected_db}' AND marker='{expected_marker}' "
                f"THEN 1 ELSE 0 END) AS route_proof FROM {relation};")

    def run_sql(at, statement, *, script=False):
        return [tap(at, "sql-editor-scroll"), key(at+.25, "a", cmd=True),
                {"t": at+.5, "type": "paste", "text": statement},
                tap(at+1.2, "sql-run-script" if script else "sql-run")]

    choose_alt = [tap(6.5, trigger), tap(7.6, f"option{ordered.index(alternate.database)}")]
    before = sql_guarded(alternate.database)
    qualified = "SELECT namespace.nspname AS qualified_schema FROM pg_catalog.pg_namespace AS namespace WHERE namespace.nspname='pg_catalog';"
    self.assertEqual(alternate.query(qualified), "pg_catalog")
    script = before + " " + qualified
    actions = [*opened, *choose_alt, *run_sql(8.5, before), *run_sql(13, qualified),
               *run_sql(17.5, before), *run_sql(22, script, script=True),
               tap(26, "query-sidebar-tab-history")]
    # Instrumented parallel replays can render slowly; preserve the UI
    # timeline while allowing a bounded wall-clock margin.
    final = self.replay(actions, until=28.5, timeout=110)
    self.expect_rendered(final, key="query-result-tab-4")
    self.expect_rendered(final, key="query-result-tab-5")
    self.expect_rendered(final, text=f"Database: {alternate.database}", key="history-current-db")
    self.expect_rendered(final, text=alternate.database, within=trigger)
    self.expect_rendered(final, text=alternate.database, within="result-cell-0-0")
    self.expect_rendered(final, text=table[alternate.database], within="result-cell-0-1")
    histories = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
    self.assertEqual([(r["database"], r["success"]) for r in histories],
                     [(alternate.database, True)] * 4)
    self.assertEqual([r["sql"].rstrip(";") for r in histories],
                     [s.rstrip(";") for s in (before, qualified, before, script)])
    self.assertEqual(primary.query(f"SELECT marker FROM {relation};"), table[primary.database])
    self.assertEqual(alternate.query(f"SELECT marker FROM {relation};"), table[alternate.database])

    # A fresh editor in a profile pinned to a database has no selector.
    alternate.write_profile(self)
    pinned = self.replay([*opened, *run_sql(7.2, before)], until=11.3)
    self.assertFalse(visible(pinned, key=trigger))
    self.expect_rendered(pinned, text=alternate.database, within="result-cell-0-0")
    self.expect_rendered(pinned, text=table[alternate.database], within="result-cell-0-1")

def run_legacy_postgres_database_context_lifecycle(self, primary, alternate):
    """Migrate six real SQ-LIFECYCLE-001 switches, watching exact PostgreSQL backends."""
    primary.write_profile(self, default_database=False)
    keys = primary.query("SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname;").splitlines()
    self.assertEqual(keys[0], primary.database)
    self.assertIn(alternate.database, keys)
    selected = [alternate.database, primary.database] * 3
    run_id = "rune_context_lifecycle_" + uuid.uuid4().hex
    tags = [f"{run_id}_{i}" for i in range(len(selected))]
    trigger = "query-context-database-trigger"
    actions = [tap(.5, "conn-open-e2e-tcp-sql"), tap(5, "conn-toolbar-new-query")]
    statements = []
    for i, (target, tag) in enumerate(zip(selected, tags)):
        at = 7 + i * 5
        # Success is impossible on the wrong database, independently from
        # the UI label or target written into the history.
        stmt = (f"SELECT current_database() AS verified_database, "
                f"1 / CASE WHEN current_database()='{target}' THEN 1 ELSE 0 END AS route_proof "
                f"FROM (SELECT pg_sleep(0.9)) delay /* {tag} */;")
        statements.append(stmt)
        actions.extend([
            tap(at, trigger), tap(at+.65, f"option{keys.index(target)}"),
            tap(at+1.1, "sql-editor-scroll"), key(at+1.35, "a", cmd=True),
            {"t": at+1.6, "type": "paste", "text": stmt}, tap(at+2.25, "sql-run"),
        ])

    marker = f"query LIKE '%/* {run_id}_%'"
    self.addCleanup(primary.query,
                    f"SELECT pg_cancel_backend(pid) FROM pg_stat_activity "
                    f"WHERE pid <> pg_backend_pid() AND state='active' AND {marker};")
    seen = {}
    released = set()
    with ThreadPoolExecutor(max_workers=1) as pool:
        replay = pool.submit(self.replay, actions, until=41, timeout=125)
        deadline = monotonic() + 122
        while not replay.done() and monotonic() < deadline:
            known = sorted({pid for pid, _database in seen.values()})
            watched = f" OR pid IN ({','.join(map(str, known))})" if known else ""
            snapshot = primary.query(
                f"SELECT pid, datname, substring(query FROM '{run_id}_[0-5]'), state "
                f"FROM pg_stat_activity WHERE pid <> pg_backend_pid() AND ({marker}{watched});"
            )
            present = set()
            for record in snapshot.splitlines():
                pid_str, database, tag, state = record.split("|")
                pid = int(pid_str)
                present.add(pid)
                if state == "active" and tag in tags:
                    previous = seen.setdefault(tag, (pid, database))
                    self.assertEqual(previous, (pid, database))
            if not replay.done():
                released.update(pid for pid in known if pid not in present)
            if len(seen) == len(tags) and all(pid in released for pid, _database in seen.values()):
                break
            sleep(.10)
        final = replay.result()

    self.assertEqual(set(seen), set(tags), "did not see each real switch execute while the app was alive")
    self.assertEqual([seen[tag][1] for tag in tags], selected)
    self.assertEqual(len({seen[tag][0] for tag in tags}), len(tags), "expected independently opened backend resources")
    self.assertTrue(all(pid in released for pid, _database in seen.values()),
                    "an execution-owned backend remained open before editor teardown")
    self.expect_rendered(final, text=primary.database, within=trigger)
    self.expect_rendered(final, key="tab-1")
    self.expect_rendered(final, text=primary.database, within="result-cell-0-0")
    self.assertFalse(visible(final, key="query-error-message"))
    history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
    self.assertEqual([(entry["database"], entry["success"]) for entry in history],
                     [(target, True) for target in selected])
    self.assertEqual([entry["sql"].rstrip(";") for entry in history],
                     [stmt.rstrip(";") for stmt in statements])

def run_legacy_postgres_editor_database_isolation(self, primary, alternate):
    """Prove context belongs to the live editor, not to the shared connection."""
    primary.write_profile(self, default_database=False)
    keys = primary.query("SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname;").splitlines()
    self.assertEqual(keys[0], primary.database)
    trigger = "query-context-database-trigger"

    def submit(at, name, target):
        sql = (f"SELECT current_database() AS {name}, 1 / "
               f"(CASE WHEN current_database()='{target}' THEN 1 ELSE 0 END) AS route_proof;")
        return [tap(at, "sql-editor-scroll"), key(at+.25, "a", cmd=True),
                {"t": at+.55, "type": "paste", "text": sql}, tap(at+1.35, "sql-run")]

    actions = [
        tap(.5, "conn-open-e2e-tcp-sql"), tap(5, "conn-toolbar-new-query"),
        tap(6.8, trigger), tap(7.5, f"option{keys.index(alternate.database)}"),
        *submit(8.5, "first_editor_alt", alternate.database),
        tap(13, "conn-toolbar-new-query"),
        *submit(15, "second_editor_default", primary.database),
        tap(19.5, "tab-1"),
        *submit(21.3, "first_editor_still_alt", alternate.database),
        tap(26, "tab-close-1"), tap(27.5, "conn-toolbar-new-query"),
        *submit(29, "fresh_editor_default", primary.database),
    ]
    final = self.replay(actions, until=33.5, timeout=110)
    self.expect_rendered(final, text=primary.database, within=trigger)
    self.expect_rendered(final, key="tab-2")
    self.expect_rendered(final, key="tab-3")
    self.assertFalse(visible(final, key="tab-1"))
    self.expect_rendered(final, text="fresh_editor_default", within="result-data-grid-header")
    self.expect_rendered(final, text=primary.database, within="result-cell-0-0")
    history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
    expected = [(alternate.database, "first_editor_alt"),
                (primary.database, "second_editor_default"),
                (alternate.database, "first_editor_still_alt"),
                (primary.database, "fresh_editor_default")]
    self.assertEqual([(r["database"], r["success"]) for r in history],
                     [(target, True) for target, _ in expected])
    for entry, (_target, name) in zip(history, expected):
        self.assertIn(f" AS {name},", entry["sql"])


def run_legacy_postgres_explain_selected_database(self, primary, alternate):
    """Migrate SQ-EXPLAIN-001 with a real server plan and unchanged rows."""
    ordered = primary.query("SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname;").splitlines()
    primary.write_profile(self, default_database=False)
    name = "aaa_rune_explain_" + uuid.uuid4().hex[:12]
    relation = "public." + name
    self.assertEqual(primary.query(f"SELECT to_regclass('{relation}') IS NULL;"), "t")
    alternate.query(f"CREATE TABLE {relation} (id INTEGER PRIMARY KEY, label TEXT NOT NULL);")
    self.addCleanup(alternate.query, f"DROP TABLE {relation};")
    alternate.query(f"INSERT INTO {relation} VALUES (1,'original marker'),(2,'unchanged');")

    snapshot = f"SELECT id, label FROM {relation} ORDER BY id;"
    updated = f"UPDATE {relation} SET label='must not execute' WHERE id=1;"
    expected = ["1|original marker", "2|unchanged"]
    self.assertEqual(alternate.query(snapshot).splitlines(), expected)
    plan = json.loads(alternate.query("EXPLAIN (FORMAT JSON) " + updated))[0]["Plan"]
    self.assertEqual(plan["Operation"], "Update")
    self.assertEqual(plan["Relation Name"], name)
    primary_key_mode = plan["Plans"][0]["Node Type"]

    def paste(at, statement):
        return [tap(at, "sql-editor-scroll"), key(at+.25, "a", cmd=True),
                {"t": at+.55, "type": "paste", "text": statement}]
    trigger = "query-context-database-trigger"
    actions = [
        tap(.5, "conn-open-e2e-tcp-sql"), tap(5, "conn-toolbar-new-query"),
        tap(6.6, trigger), tap(7.6, f"option{ordered.index(alternate.database)}"),
        *paste(8.6, snapshot), tap(10, "sql-run"),
        *paste(13, updated), tap(14.3, "sql-explain"),
    ]
    explained = self.replay(actions, until=17.8)
    self.expect_rendered(explained, key="query-explain-panel")
    self.expect_rendered(explained, key="query-explain-plan")
    self.assertFalse(visible(explained, key="query-explain-error"))
    plan_lines = [n["text"] for n in explained["elements"]
                  if (n.get("key") or "").startswith("query-explain-plan-line-")]
    self.assertTrue(any(line.startswith("Update on ") and name in line for line in plan_lines))
    self.assertTrue(any(primary_key_mode in line for line in plan_lines))
    self.assertEqual(alternate.query(snapshot).splitlines(), expected)

    closed = self.replay([*actions, tap(18.2, "query-explain-close")], until=20.0)
    self.assertFalse(visible(closed, key="query-explain-panel"))
    self.expect_rendered(closed, text=alternate.database, within=trigger)
    self.expect_rendered(closed, text="original marker", within="result-cell-0-1")
    self.expect_rendered(closed, text="unchanged", within="result-cell-1-1")
    self.assertEqual(alternate.query(snapshot).splitlines(), expected)


def run_legacy_postgres_from_completion_selected_database(self, primary, alternate):
    """SE-INT-002: real FROM completion selects the exact read-only database relation."""
    db_order = primary.query(
        "SELECT datname FROM pg_database WHERE NOT datistemplate ORDER BY datname;"
    ).splitlines()
    self.assertEqual(db_order[0], primary.database)
    tag = "zz_seint_" + uuid.uuid4().hex[:8]

    def identifier(name):
        return '"' + name.replace('"', '""') + '"'

    decoy_schema = "completion Decoy " + tag[-8:]
    target_schema = 'Completion "Schema ' + tag[-8:]
    decoy_table = tag + "_decoy"
    target_table = tag + ' Mixed "Table'
    decoy = identifier(decoy_schema) + "." + identifier(decoy_table)
    target = identifier(target_schema) + "." + identifier(target_table)

    for server, schema, relation, value in (
        (primary, decoy_schema, decoy, "primary-decoy"),
        (alternate, target_schema, target, "alternate-chosen"),
    ):
        server.query(f"CREATE SCHEMA {identifier(schema)};")
        self.addCleanup(server.query, f"DROP SCHEMA {identifier(schema)};")
        server.query(f"CREATE TABLE {relation} (label TEXT NOT NULL);")
        self.addCleanup(server.query, f"DROP TABLE {relation};")
        server.query(f"INSERT INTO {relation} VALUES ('{value}');")

    start_sql = "SELECT current_database(), label FROM "
    primary.write_profile(self, default_database=False, read_only=True)
    trigger = "query-context-database-trigger"
    base = [
        tap(.5, "conn-open-e2e-tcp-sql"), tap(5, "conn-toolbar-new-query"),
        tap(6.2, "sql-editor-scroll"), key(6.5, "a", cmd=True),
        {"t": 6.9, "type": "paste", "text": start_sql},
    ]
    first = [*base, type_text(7.5, tag)]
    primary_popup = self.replay(first, until=8.6)
    self.expect_rendered(primary_popup, text=decoy, within="sql-completion-item-0")
    self.assertFalse(visible(primary_popup, text=target, within="sql-completion"))

    switched = [*first, tap(9.0, trigger),
                tap(9.8, f"option{db_order.index(alternate.database)}")]
    after_switch = self.replay(switched, until=10.7)
    self.expect_rendered(after_switch, text=alternate.database, within=trigger)
    self.assertFalse(visible(after_switch, key="sql-completion"))

    # Keep the original typed document. Backspace and retype only its last
    # character, exercising the new selected scope rather than a fresh editor.
    refocused = [*switched, tap(11.1, "sql-editor-scroll"),
                 key(11.4, "Backspace"), type_text(11.7, tag[-1])]
    selected = self.replay(refocused, until=12.7)
    self.expect_rendered(selected, text=target, within="sql-completion-item-0")
    self.assertFalse(visible(selected, text=decoy, within="sql-completion"))

    statement = start_sql + target
    self.assertEqual(alternate.query(statement), alternate.database + "|alternate-chosen")
    final = self.replay([*refocused, key(13.0, "Enter"), tap(14.0, "sql-run")], until=17.5)
    self.expect_rendered(final, text=alternate.database, within="result-cell-0-0")
    self.expect_rendered(final, text="alternate-chosen", within="result-cell-0-1")
    self.assertFalse(visible(final, key="sql-completion"))
    self.assertFalse(visible(final, key="query-error-message"))
    history = json.loads((self.root / "query-history.json").read_text(encoding="utf-8"))
    self.assertEqual([(row["database"], row["sql"].rstrip(";"), row["success"]) for row in history],
                     [(alternate.database, statement, True)])
    self.assertEqual(primary.query(f"SELECT label FROM {decoy};"), "primary-decoy")
    self.assertEqual(alternate.query(f"SELECT label FROM {target};"), "alternate-chosen")


def register_pg_editor_context_cases(suite, config, fixture_factory):
    """Register only when the explicitly allowlisted alternate is configured."""
    if "alternateDatabase" not in json.loads(config.read_text(encoding="utf-8")):
        return

    def case(journey):
        def run(self):
            primary = fixture_factory("postgresql", config)
            alternate = fixture_factory("postgresql", config, alternate=True)
            return journey(self, primary, alternate)
        return run

    for suffix, journey in (
        ("", run_legacy_postgres_editor_database_context),
        ("_lifecycle", run_legacy_postgres_database_context_lifecycle),
        ("_isolation", run_legacy_postgres_editor_database_isolation),
    ):
        name = "test_postgresql_legacy_editor_database_context" + suffix
        run = case(journey)
        run.__name__ = name
        run.__doc__ = journey.__doc__
        setattr(suite, name, run)

    run_from_completion = case(run_legacy_postgres_from_completion_selected_database)
    from_name = "test_postgresql_legacy_from_completion_applies_selected_catalog_relation"
    run_from_completion.__name__ = from_name
    run_from_completion.__doc__ = run_legacy_postgres_from_completion_selected_database.__doc__
    setattr(suite, from_name, run_from_completion)

    run_explain = case(run_legacy_postgres_explain_selected_database)
    name = "test_postgresql_legacy_editor_explain_selected_database"
    run_explain.__name__ = name
    run_explain.__doc__ = run_legacy_postgres_explain_selected_database.__doc__
    setattr(suite, name, run_explain)
