//! Data Transfer-only synchronization for explicit values imported into
//! PostgreSQL identity and serial columns.

use crate::postgres::PostgresDriver;
use datazen_driver_api::{ConnectionHandle, DatabaseDriver, DriverError};
use sqlx::Row;

struct SequenceOptions {
    schema: String,
    name: String,
    start: i64,
    increment: i64,
    min: i64,
    max: i64,
    cache: i64,
    cycle: bool,
    can_read: bool,
}

impl PostgresDriver {
    pub(crate) async fn advance_transfer_identity_sequences_impl(
        &self,
        handle: &ConnectionHandle,
        schema: Option<&str>,
        table: &str,
        columns: &[String],
    ) -> Result<(), DriverError> {
        if columns.is_empty() {
            return Ok(());
        }

        let pinned = self
            .transactions
            .lock()
            .await
            .get(&handle.id)
            .cloned()
            .ok_or_else(|| {
                DriverError::TransactionError(
                    "identity sequence synchronization requires the active table transaction"
                        .into(),
                )
            })?;
        let mut conn = pinned.lock().await;

        let version: i32 =
            sqlx::query_scalar("SELECT current_setting('server_version_num')::integer")
                .fetch_one(&mut **conn)
                .await
                .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        if version < 100_000 {
            return Err(DriverError::Unsupported(
                "safe transactional identity sequence synchronization requires PostgreSQL 10 or newer; upgrade the target or import into a target whose sequence is already ahead".into(),
            ));
        }

        let relation = qualified_ident(self, schema, table);
        let table_lock = format!("LOCK TABLE {relation} IN SHARE ROW EXCLUSIVE MODE");
        sqlx::query(&table_lock)
            .execute(&mut **conn)
            .await
            .map_err(|error| {
                DriverError::QueryFailed(format!(
                    "cannot lock target table before sequence synchronization: {error}"
                ))
            })?;

        let mut seen = std::collections::HashSet::new();
        for column in columns {
            if !seen.insert(column) {
                continue;
            }
            self.advance_one_transfer_identity_sequence(
                &mut *conn, &relation, schema, table, column,
            )
            .await?;
        }
        Ok(())
    }

    async fn advance_one_transfer_identity_sequence(
        &self,
        conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
        table_relation: &str,
        schema: Option<&str>,
        table: &str,
        column: &str,
    ) -> Result<(), DriverError> {
        let sequence_for_column = sqlx::query_scalar::<_, Option<String>>(
            "SELECT pg_catalog.pg_get_serial_sequence($1, $2)",
        )
        .bind(table_relation)
        .bind(column)
        .fetch_one(&mut **conn)
        .await
        .map_err(|error| DriverError::QueryFailed(error.to_string()))?
        .ok_or_else(|| {
            DriverError::Unsupported(format!(
                "target column {}.{}.{} has no owned serial/identity sequence; transfer cannot safely advance generated IDs",
                schema.unwrap_or("<search_path>"), table, column
            ))
        })?;

        let options = sequence_options(conn, &sequence_for_column).await?;
        if !options.can_read {
            return Err(DriverError::Unsupported(format!(
                "cannot read the owned sequence for target column {}.{}.{}; grant sequence SELECT and ownership to the transfer role",
                schema.unwrap_or("<search_path>"),
                table,
                column
            )));
        }
        if options.cycle || options.increment <= 0 {
            return Err(DriverError::Unsupported(format!(
                "target identity sequence {}.{} uses CYCLE or a non-positive increment; change it to a non-cycling ascending sequence before importing explicit IDs",
                options.schema, options.name
            )));
        }
        if options.cache != 1 {
            return Err(DriverError::Unsupported(format!(
                "target identity sequence {}.{} has CACHE {}; safe transfer reseeding currently requires CACHE 1 so another session cannot reuse a preallocated ID range",
                options.schema, options.name, options.cache
            )));
        }

        let sequence_relation = qualified_ident(self, Some(&options.schema), &options.name);
        // With CACHE 1 this no-op option change obtains PostgreSQL's sequence
        // DDL lock before reading state. It blocks concurrent nextval calls
        // until the table transaction commits or rolls back.
        let lock_sequence = format!("ALTER SEQUENCE {sequence_relation} CACHE 1");
        sqlx::query(&lock_sequence)
            .execute(&mut **conn)
            .await
            .map_err(|error| {
                DriverError::QueryFailed(format!(
                    "cannot safely lock owned sequence {}.{} (check sequence ownership): {error}",
                    options.schema, options.name
                ))
            })?;

        // Re-read options after acquiring the sequence lock. If a concurrent
        // sequence alteration changed its contract, this table transaction is
        // rolled back by the Host before anything is committed.
        let locked_options = sequence_options(conn, &sequence_for_column).await?;
        if locked_options.schema != options.schema
            || locked_options.name != options.name
            || locked_options.start != options.start
            || locked_options.increment != options.increment
            || locked_options.min != options.min
            || locked_options.max != options.max
            || locked_options.cache != 1
            || locked_options.cycle
        {
            return Err(DriverError::Unsupported(format!(
                "target sequence {}.{} changed while transfer was preparing its restart; retry after sequence DDL stops",
                options.schema, options.name
            )));
        }

        let state_sql = format!("SELECT last_value::bigint, is_called FROM {sequence_relation}");
        let state_row = sqlx::query(&state_sql)
            .fetch_one(&mut **conn)
            .await
            .map_err(|error| {
                DriverError::QueryFailed(format!(
                    "cannot inspect owned sequence {}.{}: {error}",
                    options.schema, options.name
                ))
            })?;
        let last_value: i64 = state_row
            .try_get(0)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;
        let is_called: bool = state_row
            .try_get(1)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?;

        let target_column = self.quote_ident(column);
        let max_sql = format!("SELECT MAX({target_column})::bigint FROM {table_relation}");
        let imported_max: Option<i64> =
            sqlx::query_scalar(&max_sql)
                .fetch_one(&mut **conn)
                .await
                .map_err(|error| DriverError::QueryFailed(error.to_string()))?;

        let restart = restart_value(
            last_value,
            is_called,
            locked_options.start,
            locked_options.increment,
            imported_max,
        )?;
        if restart < i128::from(locked_options.min) || restart > i128::from(locked_options.max) {
            return Err(DriverError::Unsupported(format!(
                "target identity sequence {}.{} cannot generate a value beyond the imported high-water mark within its configured bounds",
                options.schema, options.name
            )));
        }
        let restart = i64::try_from(restart).map_err(|_| {
            DriverError::Unsupported(format!(
                "target identity sequence {}.{} restart value exceeds PostgreSQL's supported range",
                options.schema, options.name
            ))
        })?;
        let restart_sql = format!("ALTER SEQUENCE {sequence_relation} RESTART WITH {restart}");
        sqlx::query(&restart_sql)
            .execute(&mut **conn)
            .await
            .map_err(|error| {
                DriverError::QueryFailed(format!(
                    "cannot restart owned sequence {}.{} at {restart}: {error}",
                    options.schema, options.name
                ))
            })?;
        Ok(())
    }
}

async fn sequence_options(
    conn: &mut sqlx::pool::PoolConnection<sqlx::Postgres>,
    sequence_name: &str,
) -> Result<SequenceOptions, DriverError> {
    let row = sqlx::query(
        "SELECT n.nspname, c.relname, s.seqstart::bigint, s.seqincrement::bigint, \
                s.seqmin::bigint, s.seqmax::bigint, s.seqcache::bigint, s.seqcycle, \
                pg_catalog.has_sequence_privilege(c.oid, 'SELECT') \
         FROM pg_catalog.pg_class c \
         JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace \
         JOIN pg_catalog.pg_sequence s ON s.seqrelid = c.oid \
         WHERE c.oid = $1::regclass",
    )
    .bind(sequence_name)
    .fetch_optional(&mut **conn)
    .await
    .map_err(|error| DriverError::QueryFailed(error.to_string()))?
    .ok_or_else(|| {
        DriverError::Unsupported(format!(
            "PostgreSQL could not resolve owned identity sequence {sequence_name}"
        ))
    })?;
    Ok(SequenceOptions {
        schema: row
            .try_get(0)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        name: row
            .try_get(1)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        start: row
            .try_get(2)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        increment: row
            .try_get(3)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        min: row
            .try_get(4)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        max: row
            .try_get(5)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        cache: row
            .try_get(6)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        cycle: row
            .try_get(7)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
        can_read: row
            .try_get(8)
            .map_err(|error| DriverError::QueryFailed(error.to_string()))?,
    })
}

fn next_sequence_value_after(
    maximum: i64,
    start: i64,
    increment: i64,
) -> Result<i128, DriverError> {
    if increment <= 0 {
        return Err(DriverError::Unsupported(
            "identity sequence increment must be positive".into(),
        ));
    }
    let maximum = i128::from(maximum);
    let start = i128::from(start);
    let increment = i128::from(increment);
    if maximum < start {
        return Ok(start);
    }
    Ok(start + ((maximum - start) / increment + 1) * increment)
}

fn restart_value(
    last_value: i64,
    is_called: bool,
    start: i64,
    increment: i64,
    imported_max: Option<i64>,
) -> Result<i128, DriverError> {
    if increment <= 0 {
        return Err(DriverError::Unsupported(
            "identity sequence increment must be positive".into(),
        ));
    }
    let current_next = if is_called {
        i128::from(last_value) + i128::from(increment)
    } else {
        i128::from(last_value)
    };
    let imported_next = imported_max
        .map(|maximum| next_sequence_value_after(maximum, start, increment))
        .transpose()?;
    Ok(imported_next.map_or(current_next, |value| value.max(current_next)))
}

fn qualified_ident(driver: &PostgresDriver, schema: Option<&str>, name: &str) -> String {
    match schema {
        Some(schema) => format!(
            "{}.{}",
            driver.quote_ident(schema),
            driver.quote_ident(name)
        ),
        None => driver.quote_ident(name),
    }
}

pub(crate) fn render_sync_sql(
    driver: &PostgresDriver,
    schema: Option<&str>,
    table: &str,
    columns: &[String],
) -> Vec<String> {
    columns
        .iter()
        .map(|column| render_one_sync_sql(driver, schema, table, column))
        .collect()
}

fn render_one_sync_sql(
    driver: &PostgresDriver,
    schema: Option<&str>,
    table: &str,
    column: &str,
) -> String {
    let relation = qualified_ident(driver, schema, table);
    let relation_literal = sql_literal(&relation);
    let column_literal = sql_literal(column);
    let tag = loop {
        let candidate = format!(
            "datazen_transfer_sequence_{}",
            uuid::Uuid::new_v4().simple()
        );
        if !relation.contains(&candidate) && !column.contains(&candidate) {
            break candidate;
        }
    };

    format!(
        r#"DO ${tag}$
DECLARE
    v_relation text := {relation_literal};
    v_column text := {column_literal};
    v_table_oid regclass;
    v_sequence_name text;
    v_sequence_oid regclass;
    v_sequence_schema text;
    v_sequence_relname text;
    v_start bigint;
    v_increment bigint;
    v_min bigint;
    v_max bigint;
    v_cache bigint;
    v_cycle boolean;
    v_can_read boolean;
    v_start_before bigint;
    v_increment_before bigint;
    v_min_before bigint;
    v_max_before bigint;
    v_cycle_before boolean;
    v_last bigint;
    v_is_called boolean;
    v_imported_max numeric;
    v_current_next numeric;
    v_imported_next numeric;
    v_restart numeric;
    v_default text;
BEGIN
    v_table_oid := pg_catalog.to_regclass(v_relation);
    IF v_table_oid IS NULL THEN
        RAISE EXCEPTION 'Data Transfer target table % does not exist', v_relation;
    END IF;
    SELECT pg_catalog.pg_get_expr(d.adbin, d.adrelid)
      INTO v_default
      FROM pg_catalog.pg_attribute a
      LEFT JOIN pg_catalog.pg_attrdef d
        ON d.adrelid = a.attrelid AND d.adnum = a.attnum
     WHERE a.attrelid = v_table_oid AND a.attname = v_column
       AND a.attnum > 0 AND NOT a.attisdropped;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'Data Transfer target column %.% does not exist', v_relation, v_column;
    END IF;
    v_sequence_name := pg_catalog.pg_get_serial_sequence(v_relation, v_column);
    IF v_sequence_name IS NULL THEN
        IF v_default LIKE '%nextval(%' THEN
            RAISE EXCEPTION 'target column %.% uses an unowned sequence that cannot be safely advanced', v_relation, v_column;
        END IF;
        RETURN;
    END IF;
    IF current_setting('server_version_num')::integer < 100000 THEN
        RAISE EXCEPTION 'safe transactional identity sequence synchronization requires PostgreSQL 10 or newer for owned serial/identity sequences';
    END IF;
    v_sequence_oid := v_sequence_name::regclass;
    SELECT n.nspname, c.relname, s.seqstart::bigint, s.seqincrement::bigint,
           s.seqmin::bigint, s.seqmax::bigint, s.seqcache::bigint, s.seqcycle,
           pg_catalog.has_sequence_privilege(c.oid, 'SELECT')
      INTO v_sequence_schema, v_sequence_relname, v_start, v_increment,
           v_min, v_max, v_cache, v_cycle, v_can_read
      FROM pg_catalog.pg_class c
      JOIN pg_catalog.pg_namespace n ON n.oid = c.relnamespace
      JOIN pg_catalog.pg_sequence s ON s.seqrelid = c.oid
     WHERE c.oid = v_sequence_oid;
    IF NOT FOUND OR NOT v_can_read THEN
        RAISE EXCEPTION 'cannot inspect the owned sequence for target column %.%', v_relation, v_column;
    END IF;
    IF v_cycle OR v_increment <= 0 THEN
        RAISE EXCEPTION 'target sequence %.% must be ascending and NO CYCLE before importing explicit IDs', v_sequence_schema, v_sequence_relname;
    END IF;
    IF v_cache <> 1 THEN
        RAISE EXCEPTION 'target sequence %.% has CACHE %, safe transfer reseeding requires CACHE 1', v_sequence_schema, v_sequence_relname, v_cache;
    END IF;
    v_start_before := v_start;
    v_increment_before := v_increment;
    v_min_before := v_min;
    v_max_before := v_max;
    v_cycle_before := v_cycle;

    EXECUTE pg_catalog.format('LOCK TABLE %s IN SHARE ROW EXCLUSIVE MODE', v_relation);
    EXECUTE pg_catalog.format('ALTER SEQUENCE %s CACHE 1', v_sequence_oid);
    SELECT s.seqstart::bigint, s.seqincrement::bigint, s.seqmin::bigint,
           s.seqmax::bigint, s.seqcache::bigint, s.seqcycle
      INTO v_start, v_increment, v_min, v_max, v_cache, v_cycle
      FROM pg_catalog.pg_sequence s
     WHERE s.seqrelid = v_sequence_oid;
    IF v_start <> v_start_before OR v_increment <> v_increment_before
       OR v_min <> v_min_before OR v_max <> v_max_before
       OR v_cache <> 1 OR v_cycle OR v_cycle <> v_cycle_before THEN
        RAISE EXCEPTION 'target sequence %.% changed while preparing Data Transfer', v_sequence_schema, v_sequence_relname;
    END IF;
    EXECUTE pg_catalog.format('SELECT last_value::bigint, is_called FROM %s', v_sequence_oid)
       INTO v_last, v_is_called;
    EXECUTE pg_catalog.format('SELECT MAX(%I)::numeric FROM %s', v_column, v_relation)
       INTO v_imported_max;

    v_current_next := CASE WHEN v_is_called
        THEN v_last::numeric + v_increment::numeric
        ELSE v_last::numeric END;
    v_restart := v_current_next;
    IF v_imported_max IS NOT NULL THEN
        IF v_imported_max < v_start THEN
            v_imported_next := v_start::numeric;
        ELSE
            v_imported_next := v_start::numeric
                + (trunc((v_imported_max - v_start::numeric) / v_increment::numeric) + 1)
                  * v_increment::numeric;
        END IF;
        v_restart := greatest(v_restart, v_imported_next);
    END IF;
    IF v_restart < v_min OR v_restart > v_max THEN
        RAISE EXCEPTION 'target sequence %.% cannot generate a value beyond the imported high-water mark', v_sequence_schema, v_sequence_relname;
    END IF;
    EXECUTE pg_catalog.format(
        'ALTER SEQUENCE %s RESTART WITH %s', v_sequence_oid, v_restart::bigint
    );
END
${tag}$"#
    )
}

fn sql_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(crate) fn render_sql_file_insert(
    insert_template: &str,
    identity_override_marker: &str,
) -> Result<String, DriverError> {
    if identity_override_marker.is_empty()
        || insert_template.matches(identity_override_marker).count() != 1
    {
        return Err(DriverError::Unsupported(
            "PostgreSQL SQL-file INSERT has an invalid identity override marker".into(),
        ));
    }
    let statement_tag = unique_dollar_tag("transfer_insert", insert_template);
    let statement_literal = format!("${statement_tag}${insert_template}${statement_tag}$");
    let marker_literal = sql_literal(identity_override_marker);
    let body = format!(
        "DECLARE\n    v_insert_sql text := {statement_literal};\nBEGIN\n    IF current_setting('server_version_num')::integer >= 100000 THEN\n        EXECUTE pg_catalog.replace(v_insert_sql, {marker_literal}, 'OVERRIDING SYSTEM VALUE');\n    ELSE\n        EXECUTE pg_catalog.replace(v_insert_sql, {marker_literal}, '');\n    END IF;\nEND"
    );
    let block_tag = unique_dollar_tag("transfer_block", &body);
    Ok(format!("DO ${block_tag}${body}${block_tag}$"))
}

fn unique_dollar_tag(prefix: &str, payload: &str) -> String {
    loop {
        let candidate = format!("datazen_{prefix}_{}", uuid::Uuid::new_v4().simple());
        let delimiter = format!("${candidate}$");
        if !payload.contains(&delimiter) {
            return candidate;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        next_sequence_value_after, render_sql_file_insert, render_sync_sql, restart_value,
    };
    use crate::postgres::PostgresDriver;
    use datazen_driver_api::{ConnectionHandle, DriverError};

    #[test]
    fn imported_highwater_advances_to_the_next_sequence_value() {
        assert_eq!(next_sequence_value_after(41, 1, 1).unwrap(), 42);
        assert_eq!(next_sequence_value_after(41, 1, 10).unwrap(), 51);
        assert_eq!(next_sequence_value_after(0, 1, 1).unwrap(), 1);
    }

    #[test]
    fn non_positive_increment_is_refused() {
        assert!(next_sequence_value_after(41, 1, 0).is_err());
        assert!(next_sequence_value_after(41, 1, -1).is_err());
        assert!(restart_value(41, true, 1, 0, Some(41)).is_err());
        assert!(restart_value(41, true, 1, -1, Some(41)).is_err());
    }

    #[test]
    fn restart_preserves_existing_highwater_and_un_called_start() {
        assert_eq!(restart_value(80, true, 1, 1, Some(41)).unwrap(), 81);
        assert_eq!(restart_value(1, false, 1, 1, Some(41)).unwrap(), 42);
        assert_eq!(restart_value(7, false, 7, 1, None).unwrap(), 7);
        assert_eq!(restart_value(80, true, 1, 10, Some(41)).unwrap(), 90);
        assert_eq!(restart_value(5, false, 10, 3, Some(6)).unwrap(), 10);
        assert_eq!(restart_value(10, true, 1, 10, Some(12)).unwrap(), 21);
        assert_eq!(
            restart_value(i64::MAX, true, 1, i64::MAX, Some(i64::MAX)).unwrap(),
            i128::from(i64::MAX) * 2
        );
    }

    #[tokio::test]
    async fn empty_identity_column_list_is_a_noop_without_a_table_transaction() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "missing-transaction".into(),
            pool_id: "unused-pool".into(),
        };

        assert!(driver
            .advance_transfer_identity_sequences_impl(&handle, None, "items", &[])
            .await
            .is_ok());
    }

    #[tokio::test]
    async fn identity_sync_refuses_to_run_outside_the_active_table_transaction() {
        let driver = PostgresDriver::new();
        let handle = ConnectionHandle {
            id: "missing-transaction".into(),
            pool_id: "unused-pool".into(),
        };
        let error = driver
            .advance_transfer_identity_sequences_impl(
                &handle,
                Some("public"),
                "items",
                &["id".into()],
            )
            .await
            .unwrap_err();

        match error {
            DriverError::TransactionError(message) => {
                assert!(message.contains("active table transaction"));
            }
            other => panic!("expected transaction error, got {other:?}"),
        }
    }

    #[test]
    fn sql_file_sync_uses_quoted_schema_table_and_column_names() {
        let driver = PostgresDriver::new();
        let sql = render_sync_sql(
            &driver,
            Some("schema.with\"quote"),
            "table.with\"quote",
            &["id.with\"quote".into()],
        );
        assert_eq!(sql.len(), 1);
        assert!(sql[0].contains("\"schema.with\"\"quote\".\"table.with\"\"quote\""));
        assert!(sql[0].contains("'id.with\"quote'"));
        assert!(sql[0].contains("pg_get_serial_sequence"));
        assert!(sql[0].contains("ALTER SEQUENCE %s RESTART WITH %s"));
        assert!(
            sql[0].find("IF v_sequence_name IS NULL").unwrap()
                < sql[0]
                    .find("IF current_setting('server_version_num')::integer < 100000")
                    .unwrap()
        );
    }

    #[test]
    fn sql_file_sync_escapes_literal_quotes_and_empty_column_lists_emit_no_sql() {
        let driver = PostgresDriver::new();
        assert!(render_sync_sql(&driver, Some("ignored"), "items", &[]).is_empty());

        let sql = render_sync_sql(
            &driver,
            Some("schema'with'quotes"),
            "table'with'quotes",
            &["id'with'quotes".into()],
        );
        assert_eq!(sql.len(), 1);
        assert!(sql[0]
            .contains("v_relation text := '\"schema''with''quotes\".\"table''with''quotes\"'"));
        assert!(sql[0].contains("v_column text := 'id''with''quotes'"));
    }

    #[test]
    fn sql_file_insert_renders_a_single_version_safe_identity_statement() {
        let marker = "/*DATAZEN_TRANSFER_OVERRIDE_TEST*/";
        let insert = format!(
            "INSERT INTO \"people VALUES table\" (\"id\") {marker} VALUES ('text VALUES value')"
        );
        let sql = render_sql_file_insert(&insert, marker).unwrap();
        assert!(sql.contains("server_version_num')::integer >= 100000"));
        assert!(sql.contains("pg_catalog.replace(v_insert_sql, '/*DATAZEN_TRANSFER_OVERRIDE_TEST*/', 'OVERRIDING SYSTEM VALUE')"));
        assert!(sql.contains(
            "pg_catalog.replace(v_insert_sql, '/*DATAZEN_TRANSFER_OVERRIDE_TEST*/', '')"
        ));
        assert!(sql.contains(&insert));
        assert_eq!(sql.matches("INSERT INTO").count(), 1);
        assert!(!sql.contains("OVERRIDING SYSTEM VALUE VALUES ('text VALUES value')"));
        assert!(render_sql_file_insert("INSERT INTO people (id) VALUES (1)", "").is_err());
        assert!(render_sql_file_insert("INSERT INTO people (id) VALUES (1)", "missing").is_err());
        assert!(
            render_sql_file_insert("{0} INSERT INTO people (id) VALUES (1) {0}", "{0}").is_err()
        );
    }
}
