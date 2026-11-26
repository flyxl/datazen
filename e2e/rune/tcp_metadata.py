"""Real PostgreSQL/MySQL catalog and table-structure journeys."""
import re
import uuid

from scenarios import tap, visible
from tcp_sql import DisposableSql


def records(database, sql):
    """A separate psql/mysql connection, with a delimiter safe for fixture names."""
    return [line.split("|") for line in database.query(sql).splitlines() if line]


def metadata_fixture(test, database):
    tag = uuid.uuid4().hex[:10]
    table = f"aaa_rune_meta_{tag}"
    owner = f"aaa_rune_owner_{tag}"
    schema = database.database
    if database.engine == "postgresql":
        schema = f"aaa_rune_md_{tag}"
        database.query(f"CREATE SCHEMA {schema};")
        test.addCleanup(database.query, f"DROP SCHEMA {schema};")
    relation, reference = f"{schema}.{table}", f"{schema}.{owner}"
    fk, unique, secondary = f"fk_rune_{tag}", f"uq_rune_{tag}", f"ix_rune_{tag}"
    database.query(f"""
        CREATE TABLE {reference} (namespace VARCHAR(16) NOT NULL, entity INTEGER NOT NULL,
                                  PRIMARY KEY(namespace, entity));
    """)
    test.addCleanup(database.query, f"DROP TABLE {reference};")
    database.query(f"""
        CREATE TABLE {relation} (
            id INTEGER PRIMARY KEY, region VARCHAR(16) NOT NULL, owner_id INTEGER NOT NULL,
            label VARCHAR(84) NULL,
            CONSTRAINT {fk} FOREIGN KEY(region, owner_id) REFERENCES {reference}(namespace, entity)
        );
    """)
    test.addCleanup(database.query, f"DROP TABLE {relation};")
    database.query(f"CREATE UNIQUE INDEX {unique} ON {relation} (label, owner_id);")
    database.query(f"CREATE INDEX {secondary} ON {relation} (owner_id, region);")
    return schema, table, owner, fk, unique, secondary


def catalog_oracle(database, schema, table, fk):
    condition = f"table_schema='{schema}' AND table_name='{table}'"
    columns = records(database, f"""
        SELECT CONCAT(column_name, '|', data_type, '|', is_nullable)
        FROM information_schema.columns WHERE {condition} ORDER BY ordinal_position;
    """)
    if database.engine == "postgresql":
        indexes = records(database, f"""
            SELECT CONCAT(ci.relname, '|', string_agg(
                pg_get_indexdef(ci.oid, slot, true), ', ' ORDER BY slot),
                '|', CASE WHEN idx.indisunique THEN 'yes' ELSE 'no' END)
            FROM pg_index idx JOIN pg_class ci ON ci.oid=idx.indexrelid
            CROSS JOIN LATERAL generate_series(1, idx.indnkeyatts) AS slot
            WHERE idx.indrelid='{schema}.{table}'::regclass
            GROUP BY ci.oid, ci.relname, idx.indisunique ORDER BY ci.relname;
        """)
        definition = database.query(f"""
            SELECT pg_get_constraintdef(oid) FROM pg_constraint
            WHERE conrelid='{schema}.{table}'::regclass AND conname='{fk}';
        """)
        match = re.match(r"FOREIGN KEY \(([^)]+)\) REFERENCES ([^( ]+)\(([^)]+)\)", definition)
        if not match:
            raise AssertionError("PostgreSQL catalog did not return the composite FK")
        local, target, remote = match.groups()
        reference = f"{target} ({remote})"
    else:
        indexes = records(database, f"""
            SELECT CONCAT(index_name, '|',
                GROUP_CONCAT(column_name ORDER BY seq_in_index SEPARATOR ', '),
                '|', CASE WHEN non_unique=0 THEN 'yes' ELSE 'no' END)
            FROM information_schema.statistics WHERE {condition}
            GROUP BY index_name, non_unique ORDER BY index_name;
        """)
        links = records(database, f"""
            SELECT CONCAT(column_name, '|', referenced_table_name, '|', referenced_column_name)
            FROM information_schema.key_column_usage WHERE {condition} AND constraint_name='{fk}'
            ORDER BY ordinal_position;
        """)
        if len(links) != 2 or links[0][1] != links[1][1]:
            raise AssertionError("MySQL catalog did not return the composite FK")
        local = ", ".join(row[0] for row in links)
        reference = f"{links[0][1]} ({', '.join(row[2] for row in links)})"
    return columns, indexes, [fk, local, reference]


def row_cells(tree, prefix):
    keys = [node["key"] for node in tree["elements"]
            if re.fullmatch(re.escape(prefix) + r"\d+", node.get("key", ""))
            and visible(tree, key=node["key"])]
    rows = []
    for key in keys:
        root = next(node["path"] for node in tree["elements"] if node.get("key") == key)
        rows.append([node["text"] for node in tree["elements"]
                     if node.get("text") and node["path"].startswith(root + "/")])
    return rows


class TcpMetadataJourneys:
    def run_metadata_journey(self, engine, config):
        database = DisposableSql(engine, config)
        schema, table, owner, fk, unique, secondary = metadata_fixture(self, database)
        columns, indexes, link = catalog_oracle(database, schema, table, fk)
        self.assertEqual([row[0] for row in columns], ["id", "region", "owner_id", "label"])
        self.assertIn([unique, "label, owner_id", "yes"], indexes)
        self.assertIn([secondary, "owner_id, region", "no"], indexes)
        qualified_owner = f"{schema}.{owner}" if engine == "postgresql" else owner
        self.assertEqual(link, [fk, "region, owner_id", f"{qualified_owner} (namespace, entity)"])
        database.write_profile(self)
        browse = [tap(.5, "conn-open-e2e-tcp-sql"), tap(4, f"tree-table-{table}")]
        structure = self.replay([*browse, tap(5.5, "view-columns")], until=7.5)
        self.expect_rendered(structure, key=f"tree-table-{table}")
        if engine == "postgresql":
            self.expect_rendered(structure, key=f"tree-schema-{database.database}-{schema}")
        self.assertFalse(visible(structure, key="table-read-error"))
        self.assertFalse(visible(structure, key="columns-structure-notice"))
        # The MySQL native driver reports the full declaration (e.g. varchar(84));
        # the SQL oracle reads its base type independently. Match them on that level.
        actual_columns = [[name, sql_type.lower().split("(")[0], nullable]
                          for name, sql_type, nullable in row_cells(structure, "column-row-")]
        self.assertEqual(actual_columns, [[name, kind.lower(), nullable.lower()]
                                         for name, kind, nullable in columns])

        index_tree = self.replay([*browse, tap(5.5, "view-indexes")], until=7.5)
        self.assertFalse(visible(index_tree, key="indexes-structure-notice"))
        self.assertEqual(sorted(row_cells(index_tree, "index-row-")), sorted(indexes))
        fk_tree = self.replay([*browse, tap(5.5, "view-fks")], until=7.5)
        self.assertFalse(visible(fk_tree, key="fks-structure-notice"))
        self.assertEqual(row_cells(fk_tree, "fk-row-"), [link])

        if engine == "mysql":
            raw = database.query(f"SHOW CREATE TABLE {schema}.{table};")
            name, definition = raw.split("\t", 1)
            self.assertEqual(name, table)
            expected_lines = definition.replace(r"\n", "\n").splitlines()
            self.assertGreater(len(expected_lines), 5)
            ddl = self.replay([*browse, tap(5.5, "view-ddl")], until=7.5)
            self.expect_rendered(ddl, key="table-ddl")
            for line in expected_lines:
                self.expect_rendered(ddl, text=line, within="table-ddl")


    def run_postgres_table_ddl(self, config):
        database = DisposableSql("postgresql", config)
        tag = uuid.uuid4().hex[:10]
        schema = f"AAA Rune's {tag}"
        table = f'AAA Report "draft" {tag}'
        owner = f'Owner {tag}'
        secondary = f'By Label {tag}'

        def ident(name):
            return '"' + name.replace('"', '""') + '"'

        def literal(value):
            return "'" + value.replace("'", "''") + "'"

        relation = f"{ident(schema)}.{ident(table)}"
        reference = f"{ident(schema)}.{ident(owner)}"
        database.query(f"CREATE SCHEMA {ident(schema)};")
        self.addCleanup(database.query, f"DROP SCHEMA {ident(schema)};")
        database.query(f"CREATE TABLE {reference} (id integer PRIMARY KEY);")
        self.addCleanup(database.query, f"DROP TABLE {reference};")
        database.query(f"""
            INSERT INTO {reference} VALUES (11);
            CREATE TABLE {relation} (
                id integer,
                "Owner Key" integer NOT NULL,
                "Label" varchar(31) DEFAULT 'owner''s label',
                score integer DEFAULT 7 NOT NULL,
                CONSTRAINT "Report PK" PRIMARY KEY (id),
                CONSTRAINT "Owner FK" FOREIGN KEY ("Owner Key")
                    REFERENCES {reference} (id) ON DELETE RESTRICT,
                CONSTRAINT "Positive Score" CHECK (score >= 0)
            );
            CREATE UNIQUE INDEX {ident(secondary)} ON {relation} ("Label", "Owner Key")
                WHERE score > 0;
            INSERT INTO {relation} (id, "Owner Key") VALUES (1, 11);
        """)
        self.addCleanup(database.query, f"DROP TABLE {relation};")
        oid = f"{literal(relation)}::pg_catalog.regclass"

        def oracle():
            columns = records(database, f"""
                SELECT quote_ident(a.attname) || '|' || format_type(a.atttypid, a.atttypmod)
                    || '|' || a.attnotnull::text || '|' || COALESCE(pg_get_expr(d.adbin, d.adrelid), '')
                FROM pg_catalog.pg_attribute a
                LEFT JOIN pg_catalog.pg_attrdef d ON d.adrelid=a.attrelid AND d.adnum=a.attnum
                WHERE a.attrelid={oid} AND a.attnum > 0 AND NOT a.attisdropped
                ORDER BY a.attnum;
            """)
            constraints = records(database, f"""
                SELECT quote_ident(conname) || '|' || pg_get_constraintdef(oid)
                FROM pg_catalog.pg_constraint WHERE conrelid={oid} ORDER BY conname;
            """)
            indexes = database.query(f"""
                SELECT pg_get_indexdef(i.indexrelid)
                FROM pg_catalog.pg_index i
                JOIN pg_catalog.pg_class c ON c.oid=i.indexrelid
                WHERE i.indrelid={oid} AND c.relname={literal(secondary)};
            """)
            rows = records(database, f"""
                SELECT id::text || '|' || "Owner Key"::text || '|' || "Label" || '|' || score::text
                FROM {relation} ORDER BY id;
            """)
            return columns, constraints, indexes, rows

        original = oracle()
        columns, constraints, index, rows = original
        self.assertEqual([column[0] for column in columns], ["id", '"Owner Key"', '"Label"', "score"])
        self.assertEqual(rows, [["1", "11", "owner's label", "7"]])
        self.assertEqual({name for name, _ in constraints}, {'"Report PK"', '"Owner FK"', '"Positive Score"'})
        self.assertTrue(index.startswith(f"CREATE UNIQUE INDEX {ident(secondary)} ON {relation}"))
        self.assertIn("WHERE (score > 0)", index)
        database.write_profile(self, read_only=True)
        actions = [tap(.5, "conn-open-e2e-tcp-sql"), tap(4, f"tree-table-{table}"),
                   tap(5.6, "view-ddl")]
        ddl = self.replay(actions, until=8.1)
        self.expect_rendered(ddl, key=f"tree-schema-{database.database}-{schema}")
        self.expect_rendered(ddl, text=f"CREATE TABLE {relation} (", within="table-ddl")
        container = next(n["path"] for n in ddl["elements"] if n.get("key") == "table-ddl")
        lines = [n["text"] for n in ddl["elements"] if n.get("text")
                 and n["path"].startswith(container + "/")]
        normalized = [line.strip().removesuffix(",") for line in lines]
        for name, kind, not_null, default in columns:
            expected = f"{name} {kind}"
            if default:
                expected += f" DEFAULT {default}"
            if not_null == "true":
                expected += " NOT NULL"
            self.assertIn(expected, normalized)
        for name, definition in constraints:
            self.assertIn(f"CONSTRAINT {name} {definition}", normalized)
        self.assertEqual([line for line in normalized if line.startswith("CREATE UNIQUE INDEX")],
                         [index + ";"], "the primary-key index must not be emitted twice")
        self.assertEqual(oracle(), original)

        # The current navigator addresses rows by bare name. When that key
        # becomes ambiguous, it must not display the public table's DDL.
        decoy = f"public.{ident(table)}"
        database.query(f"CREATE TABLE {decoy} (decoy_only integer); INSERT INTO {decoy} VALUES (42);")
        self.addCleanup(database.query, f"DROP TABLE {decoy};")
        refused = self.replay(actions[:2], until=6.5)
        self.expect_rendered(refused, text="This table name is ambiguous across namespaces.",
                             within="table-read-error")
        # Refusal happens before a readable table workspace or DDL tab exists.
        self.assertFalse(visible(refused, key="view-ddl"))
        self.assertFalse(visible(refused, key="table-ddl"))
        self.assertFalse(visible(refused, text="decoy_only"))
        self.assertEqual(oracle(), original)
        self.assertEqual(database.query(f"SELECT decoy_only FROM {decoy};"), "42")



    def run_postgres_unsupported_ddl(self, config):
        database = DisposableSql("postgresql", config)
        tag = uuid.uuid4().hex[:12]
        schema = f"aaa_rune_unsupported_{tag}"
        table = f"aaa_rune_identity_{tag}"
        relation = f"{schema}.{table}"
        database.query(f"CREATE SCHEMA {schema};")
        self.addCleanup(database.query, f"DROP SCHEMA {schema};")
        database.query(f"""
            CREATE TABLE {relation}(id integer GENERATED ALWAYS AS IDENTITY, marker text);
        """)
        self.addCleanup(database.query, f"DROP TABLE {relation};")
        database.query(f"INSERT INTO {relation} (marker) VALUES ('identity-row');")

        def oracle():
            kind = database.query(f"""
                SELECT attidentity FROM pg_catalog.pg_attribute
                WHERE attrelid='{relation}'::regclass AND attname='id';
            """)
            rows = database.query(f"SELECT id::text || '|' || marker FROM {relation};")
            return kind, rows

        original = oracle()
        self.assertEqual(original, ("a", "1|identity-row"))
        database.write_profile(self, read_only=True)
        tree = self.replay([
            tap(.5, "conn-open-e2e-tcp-sql"), tap(4, f"tree-table-{table}"),
            tap(5.6, "view-ddl"),
        ], until=8.1)
        self.expect_rendered(tree, key="table-ddl-empty")
        self.assertFalse(visible(tree, key="table-ddl"))
        self.assertEqual(oracle(), original)

def with_metadata_journeys(suite, configs):
    for engine, config in configs.items():
        def journey(self, engine=engine, config=config):
            self.run_metadata_journey(engine, config)

        setattr(suite, f"test_{engine}_native_catalog_structure_matches_server", journey)
        if engine == "postgresql":
            def table_ddl(self, config=config):
                return self.run_postgres_table_ddl(config)

            setattr(suite, "test_postgresql_native_table_ddl_uses_exact_namespace", table_ddl)

            def unsupported_ddl(self, config=config):
                return self.run_postgres_unsupported_ddl(config)

            setattr(suite, "test_postgresql_native_table_ddl_does_not_fake_identity_definition", unsupported_ddl)
