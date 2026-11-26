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
        database.write_profile(self.store)
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


def with_metadata_journeys(suite, configs):
    for engine, config in configs.items():
        def journey(self, engine=engine, config=config):
            self.run_metadata_journey(engine, config)

        setattr(suite, f"test_{engine}_native_catalog_structure_matches_server", journey)
