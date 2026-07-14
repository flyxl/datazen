//! Roundtrip tests: verify that PG → IR → MySQL and MySQL → IR → PG
//! produce the same results as the old direct mapping functions in sync.rs.

#[cfg(test)]
mod tests {
    use crate::db::ColumnSchema;
    use crate::sync::adapter::{SyncSourceAdapter, SyncTargetAdapter};
    use crate::sync::adapters::mysql::MysqlSyncAdapter;
    use crate::sync::adapters::postgresql::PgSyncAdapter;
    use crate::sync::adapters::sqlite::SqliteSyncAdapter;
    use crate::sync::adapters::trino::TrinoSyncAdapter;

    fn col(name: &str, data_type: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: data_type.into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    // ── PG → IR → MySQL ────────────────────────────────────────────

    #[test]
    fn pg_to_mysql_integer_types() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let cases = vec![
            ("integer", "INT"),
            ("int4", "INT"),
            ("bigint", "BIGINT"),
            ("int8", "BIGINT"),
            ("smallint", "SMALLINT"),
            ("int2", "SMALLINT"),
        ];

        for (pg_type, expected_mysql) in cases {
            let ir = pg.column_to_ir(&col("x", pg_type), None);
            let result = mysql.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_mysql, "PG `{pg_type}` → MySQL");
        }
    }

    #[test]
    fn pg_to_mysql_varchar() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let ir = pg.column_to_ir(&col("x", "character varying"), Some("character varying(255)"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "VARCHAR(255)");

        let ir = pg.column_to_ir(&col("x", "character varying"), Some("character varying(100)"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "VARCHAR(100)");

        let ir = pg.column_to_ir(&col("x", "character varying"), Some("character varying"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "VARCHAR(255)");
    }

    #[test]
    fn pg_to_mysql_char() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let ir = pg.column_to_ir(&col("x", "character"), Some("character(10)"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "CHAR(10)");

        let ir = pg.column_to_ir(&col("x", "character"), Some("character"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "CHAR(1)");
    }

    #[test]
    fn pg_to_mysql_numeric() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let ir = pg.column_to_ir(&col("x", "numeric"), Some("numeric(10,2)"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "DECIMAL(10,2)");

        let ir = pg.column_to_ir(&col("x", "numeric"), Some("numeric"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "DECIMAL(65,30)");
    }

    #[test]
    fn pg_to_mysql_text_bool_float() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let cases = vec![
            ("text", "TEXT"),
            ("boolean", "TINYINT(1)"),
            ("bool", "TINYINT(1)"),
            ("real", "FLOAT"),
            ("float4", "FLOAT"),
            ("double precision", "DOUBLE"),
            ("float8", "DOUBLE"),
        ];

        for (pg_type, expected_mysql) in cases {
            let ir = pg.column_to_ir(&col("x", pg_type), None);
            let result = mysql.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_mysql, "PG `{pg_type}` → MySQL");
        }
    }

    #[test]
    fn pg_to_mysql_binary_json_uuid() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let cases = vec![
            ("bytea", "LONGBLOB"),
            ("json", "JSON"),
            ("jsonb", "JSON"),
            ("uuid", "CHAR(36)"),
        ];

        for (pg_type, expected_mysql) in cases {
            let ir = pg.column_to_ir(&col("x", pg_type), None);
            let result = mysql.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_mysql, "PG `{pg_type}` → MySQL");
        }
    }

    #[test]
    fn pg_to_mysql_date_time() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let cases = vec![
            ("date", "DATE"),
            ("time without time zone", "TIME"),
            ("time", "TIME"),
            ("time with time zone", "TIME"),
            ("timetz", "TIME"),
            ("timestamp without time zone", "DATETIME"),
            ("timestamp", "DATETIME"),
            ("timestamp with time zone", "DATETIME"),
            ("timestamptz", "DATETIME"),
        ];

        for (pg_type, expected_mysql) in cases {
            let ir = pg.column_to_ir(&col("x", pg_type), None);
            let result = mysql.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_mysql, "PG `{pg_type}` → MySQL");
        }
    }

    #[test]
    fn pg_to_mysql_network_money_misc() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let cases = vec![
            ("inet", "VARCHAR(45)"),
            ("cidr", "VARCHAR(43)"),
            ("macaddr", "VARCHAR(17)"),
            ("macaddr8", "VARCHAR(17)"),
            ("interval", "VARCHAR(255)"),
            ("money", "DECIMAL(19,2)"),
            ("xml", "TEXT"),
        ];

        for (pg_type, expected_mysql) in cases {
            let ir = pg.column_to_ir(&col("x", pg_type), None);
            let result = mysql.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_mysql, "PG `{pg_type}` → MySQL");
        }
    }

    #[test]
    fn pg_to_mysql_bit() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let ir = pg.column_to_ir(&col("x", "bit"), None);
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "BIT(1)");

        let ir = pg.column_to_ir(&col("x", "bit(8)"), Some("bit(8)"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "BIT(8)");
    }

    #[test]
    fn pg_to_mysql_array_to_json() {
        let pg = PgSyncAdapter;
        let mysql = MysqlSyncAdapter { is_mariadb: false };

        let ir = pg.column_to_ir(&col("x", "text[]"), Some("text[]"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "JSON");

        let ir = pg.column_to_ir(&col("x", "integer[]"), Some("integer[]"));
        assert_eq!(mysql.ir_type_to_native(&ir.ir_type), "JSON");
    }

    // ── MySQL → IR → PG ────────────────────────────────────────────

    #[test]
    fn mysql_to_pg_tinyint1_is_bool() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "tinyint(1)"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "boolean");
    }

    #[test]
    fn mysql_to_pg_integer_types() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let cases = vec![
            ("tinyint", "smallint"),
            ("smallint", "smallint"),
            ("mediumint", "integer"),
            ("int", "integer"),
            ("int(11)", "integer"),
            ("bigint", "bigint"),
        ];

        for (mysql_type, expected_pg) in cases {
            let ir = mysql.column_to_ir(&col("x", mysql_type), None);
            let result = pg.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_pg, "MySQL `{mysql_type}` → PG");
        }
    }

    #[test]
    fn mysql_to_pg_varchar_char() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "varchar(255)"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "character varying(255)");

        let ir = mysql.column_to_ir(&col("x", "char(10)"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "character(10)");
    }

    #[test]
    fn mysql_to_pg_decimal() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "decimal(10,2)"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "numeric(10,2)");
    }

    #[test]
    fn mysql_to_pg_float_double() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "float"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "real");

        let ir = mysql.column_to_ir(&col("x", "double"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "double precision");
    }

    #[test]
    fn mysql_to_pg_datetime_date_time() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "datetime"), None);
        assert_eq!(
            pg.ir_type_to_native(&ir.ir_type),
            "timestamp without time zone"
        );

        let ir = mysql.column_to_ir(&col("x", "date"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "date");

        let ir = mysql.column_to_ir(&col("x", "time"), None);
        assert_eq!(
            pg.ir_type_to_native(&ir.ir_type),
            "time without time zone"
        );
    }

    #[test]
    fn mysql_to_pg_text_blob() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        for t in ["text", "longtext", "mediumtext", "tinytext"] {
            let ir = mysql.column_to_ir(&col("x", t), None);
            assert_eq!(pg.ir_type_to_native(&ir.ir_type), "text", "MySQL `{t}` → PG");
        }

        for t in ["blob", "longblob", "mediumblob", "tinyblob"] {
            let ir = mysql.column_to_ir(&col("x", t), None);
            assert_eq!(pg.ir_type_to_native(&ir.ir_type), "bytea", "MySQL `{t}` → PG");
        }
    }

    #[test]
    fn mysql_to_pg_json() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "json"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "jsonb");
    }

    #[test]
    fn mysql_to_pg_enum_set() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "enum('a','b')"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "text");

        let ir = mysql.column_to_ir(&col("x", "set('x','y')"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "text");
    }

    #[test]
    fn mysql_to_pg_binary() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        for t in ["binary", "varbinary"] {
            let ir = mysql.column_to_ir(&col("x", t), None);
            assert_eq!(pg.ir_type_to_native(&ir.ir_type), "bytea", "MySQL `{t}` → PG");
        }
    }

    #[test]
    fn mysql_to_pg_bit() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "bit(1)"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "boolean");

        let ir = mysql.column_to_ir(&col("x", "bit"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "boolean");
    }

    #[test]
    fn mysql_to_pg_year() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let pg = PgSyncAdapter;

        let ir = mysql.column_to_ir(&col("x", "year"), None);
        assert_eq!(pg.ir_type_to_native(&ir.ir_type), "smallint");
    }

    // ── Cross-database roundtrip: PG → IR → Trino ──────────────────

    #[test]
    fn pg_to_trino_common_types() {
        let pg = PgSyncAdapter;
        let trino = TrinoSyncAdapter;

        let cases = vec![
            ("integer", "integer"),
            ("bigint", "bigint"),
            ("boolean", "boolean"),
            ("text", "varchar"),
            ("double precision", "double"),
            ("real", "real"),
            ("uuid", "uuid"),
            ("json", "json"),
            ("jsonb", "json"),
            ("bytea", "varbinary"),
            ("date", "date"),
        ];

        for (pg_type, expected_trino) in cases {
            let ir = pg.column_to_ir(&col("x", pg_type), None);
            let result = trino.ir_type_to_native(&ir.ir_type);
            assert_eq!(result, expected_trino, "PG `{pg_type}` → Trino");
        }
    }

    // ── Cross-database roundtrip: MySQL → IR → SQLite ───────────────

    #[test]
    fn mysql_to_sqlite_common_types() {
        let mysql = MysqlSyncAdapter { is_mariadb: false };
        let sqlite = SqliteSyncAdapter;

        let cases = vec![
            ("int", "INTEGER"),
            ("bigint", "INTEGER"),
            ("varchar(100)", "TEXT"),
            ("text", "TEXT"),
            ("float", "REAL"),
            ("double", "REAL"),
            ("json", "TEXT"),
            ("blob", "BLOB"),
            ("datetime", "TEXT"),
            ("tinyint(1)", "INTEGER"),
        ];

        for (mysql_type, expected_sqlite) in cases {
            let ir = mysql.column_to_ir(&col("x", mysql_type), None);
            let result = sqlite.ir_type_to_native(&ir.ir_type);
            assert_eq!(
                result, expected_sqlite,
                "MySQL `{mysql_type}` → SQLite"
            );
        }
    }
}
