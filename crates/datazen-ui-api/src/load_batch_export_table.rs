use serde_json::{Map, Value};

use crate::types::ColumnSchema;
use crate::{
    BatchExportColumn, BatchExportTableInput, DdlQuery, SqlDialectProvider, TableDataResult,
    TableSchema,
};

pub const DEFAULT_MAX_ROWS: usize = 100_000;
pub const DEFAULT_PAGE_SIZE: usize = 500;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableDataRequest {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
    pub driver_type: String,
    pub table: String,
    pub page: i64,
    pub page_size: i64,
    pub skip_count: bool,
}

pub struct BatchDdlRowExtractor {
    index: usize,
}

impl BatchDdlRowExtractor {
    pub fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn extract(&self, rows: &[Vec<Value>]) -> String {
        let Some(row) = rows.first() else {
            return String::new();
        };
        match row.get(self.index) {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Null) | None => String::new(),
            Some(value) => value.to_string(),
        }
    }
}

pub type SchemaFetcher = dyn Fn(&str, &str, &str) -> Result<TableSchema, String>;
pub type BatchDdlFetcher =
    dyn FnMut(&str, &str, &str, &BatchDdlRowExtractor, &str) -> Result<String, String>;
pub type TableDataFetcher = dyn FnMut(&TableDataRequest) -> Result<TableDataResult, String>;
pub type RelationSchemaProvider = dyn Fn(&str, &str) -> Option<String>;

pub struct LoadBatchExportDeps {
    pub get_schema: Box<SchemaFetcher>,
    pub get_ddl: Box<BatchDdlFetcher>,
    pub get_table_data: Box<TableDataFetcher>,
    pub get_relation_schema: Box<RelationSchemaProvider>,
}

pub struct LoadBatchExportTableParams {
    pub db_session_id: String,
    pub table_name: String,
    pub database: String,
    pub database_type: Option<String>,
    pub max_rows: usize,
    pub page_size: usize,
    pub include_rows: bool,
}

pub fn rows_to_records(
    columns: &[ColumnSchema],
    rows: &[Vec<Option<Value>>],
) -> Vec<Map<String, Value>> {
    rows.iter()
        .map(|row| {
            let mut record = Map::new();
            for (index, column) in columns.iter().enumerate() {
                record.insert(
                    column.name.clone(),
                    row.get(index).cloned().flatten().unwrap_or(Value::Null),
                );
            }
            record
        })
        .collect()
}

fn load_ddl(
    db_session_id: &str,
    table_name: &str,
    database: &str,
    database_type: Option<&str>,
    get_ddl: &mut BatchDdlFetcher,
    get_dialect: &SqlDialectProvider,
) -> Option<String> {
    let database_type = database_type?;
    let dialect = get_dialect(database_type)?;
    let query: DdlQuery = (dialect.get_table_ddl_query)(table_name);
    let extractor = BatchDdlRowExtractor::new(query.extract_column_index);
    let ddl = get_ddl(db_session_id, table_name, &query.sql, &extractor, database).ok()?;
    if ddl.trim().is_empty() {
        None
    } else {
        Some(ddl)
    }
}

fn load_all_rows(
    db_session_id: &str,
    table_name: &str,
    page_size: usize,
    max_rows: usize,
    database: &str,
    schema: Option<&str>,
    driver_type: &str,
    get_table_data: &mut TableDataFetcher,
) -> Result<Vec<Map<String, Value>>, String> {
    let mut all_rows = Vec::new();
    let mut page = 0i64;
    let mut total_rows: Option<i64> = None;
    while all_rows.len() < max_rows {
        let request = TableDataRequest {
            db_session_id: db_session_id.to_owned(),
            database: database.to_owned(),
            schema: schema.map(str::to_owned),
            driver_type: driver_type.to_owned(),
            table: table_name.to_owned(),
            page,
            page_size: page_size as i64,
            skip_count: page > 0,
        };
        let result = get_table_data(&request)?;
        if let Some(total) = result.total_rows {
            total_rows = Some(total);
        }
        let page_records = rows_to_records(&result.columns, &result.rows);
        let remaining = max_rows - all_rows.len();
        if page_records.len() > remaining {
            all_rows.extend(page_records.into_iter().take(remaining));
            break;
        }
        all_rows.extend(page_records.iter().cloned());
        if page_records.len() < page_size {
            break;
        }
        if total_rows.is_some_and(|total| all_rows.len() as i64 >= total) {
            break;
        }
        page += 1;
    }
    Ok(all_rows)
}

pub fn load_batch_export_table_data(
    params: &LoadBatchExportTableParams,
    deps: &mut LoadBatchExportDeps,
    get_dialect: &SqlDialectProvider,
) -> Result<BatchExportTableInput, String> {
    let schema = (deps.get_schema)(&params.db_session_id, &params.table_name, &params.database)?;
    let ddl = load_ddl(
        &params.db_session_id,
        &params.table_name,
        &params.database,
        params.database_type.as_deref(),
        deps.get_ddl.as_mut(),
        get_dialect,
    );
    let rows = if params.include_rows {
        let relation_schema = (deps.get_relation_schema)(&params.db_session_id, &params.table_name);
        load_all_rows(
            &params.db_session_id,
            &params.table_name,
            params.page_size,
            params.max_rows,
            &params.database,
            relation_schema.as_deref(),
            params.database_type.as_deref().unwrap_or(""),
            deps.get_table_data.as_mut(),
        )?
    } else {
        Vec::new()
    };
    let result_schema = (deps.get_relation_schema)(&params.db_session_id, &params.table_name);
    Ok(BatchExportTableInput {
        table_name: params.table_name.clone(),
        schema: result_schema,
        ddl,
        columns: schema
            .columns
            .iter()
            .map(|column| BatchExportColumn {
                name: column.name.clone(),
            })
            .collect(),
        rows: rows.into_iter().map(Value::Object).collect(),
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn column(name: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.to_owned(),
            data_type: "text".to_owned(),
            nullable: true,
            default_value: None,
            is_primary_key: None,
            is_auto_increment: None,
            comment: None,
        }
    }

    fn table_schema() -> TableSchema {
        TableSchema {
            table_name: "t".to_owned(),
            columns: vec![column("id"), column("name")],
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
        }
    }

    fn params() -> LoadBatchExportTableParams {
        LoadBatchExportTableParams {
            db_session_id: "s1".to_owned(),
            table_name: "t".to_owned(),
            database: "db".to_owned(),
            database_type: Some("postgres".to_owned()),
            max_rows: DEFAULT_MAX_ROWS,
            page_size: DEFAULT_PAGE_SIZE,
            include_rows: true,
        }
    }

    fn dialect_provider(database_type: &str) -> Option<crate::DdlDialect> {
        if database_type == "postgres" {
            Some(crate::DdlDialect {
                get_table_ddl_query: Box::new(|table| DdlQuery {
                    sql: format!("DDL({table})"),
                    extract_column_index: 1,
                }),
            })
        } else {
            None
        }
    }

    fn deps(
        requests: Rc<RefCell<Vec<TableDataRequest>>>,
        pages: Vec<TableDataResult>,
    ) -> LoadBatchExportDeps {
        let mut pages = pages.into_iter();
        LoadBatchExportDeps {
            get_schema: Box::new(|_, _, _| Ok(table_schema())),
            get_ddl: Box::new(|_, _, sql, extractor, _| {
                Ok(format!(
                    "DDL_SQL={sql};{}",
                    extractor.extract(&[vec![
                        Value::Null,
                        Value::String("CREATE TABLE t".to_owned())
                    ]])
                ))
            }),
            get_table_data: Box::new(move |request| {
                requests.borrow_mut().push(request.clone());
                pages.next().ok_or_else(|| "no more pages".to_owned())
            }),
            get_relation_schema: Box::new(|_, _| Some("public".to_owned())),
        }
    }

    fn page(rows: Vec<Vec<Option<Value>>>, total: Option<i64>) -> TableDataResult {
        TableDataResult {
            columns: vec![column("id"), column("name")],
            rows,
            total_rows: total,
            total_rows_kind: None,
            page: 0,
            page_size: DEFAULT_PAGE_SIZE as u32,
            paging: None,
        }
    }

    #[test]
    fn constants_are_pinned() {
        assert_eq!(DEFAULT_MAX_ROWS, 100_000);
        assert_eq!(DEFAULT_PAGE_SIZE, 500);
    }

    #[test]
    fn row_records_use_column_names_and_pad_null() {
        let rows = vec![
            vec![
                Some(Value::Number(1.into())),
                Some(Value::String("a".to_owned())),
            ],
            vec![Some(Value::Number(2.into()))],
        ];
        let records = rows_to_records(&[column("id"), column("name")], &rows);
        assert_eq!(records[0]["id"], serde_json::json!(1));
        assert_eq!(records[0]["name"], serde_json::json!("a"));
        assert_eq!(records[1]["name"], Value::Null);
    }

    #[test]
    fn ddl_extractor_handles_strings_numbers_and_missing_values() {
        let extractor = BatchDdlRowExtractor::new(1);
        assert_eq!(
            extractor.extract(&[vec![Value::Null, Value::String("DDL".to_owned())]]),
            "DDL"
        );
        assert_eq!(
            extractor.extract(&[vec![Value::Null, Value::Number(3.into())]]),
            "3"
        );
        assert_eq!(extractor.extract(&[vec![Value::Null, Value::Null]]), "");
        assert_eq!(extractor.extract(&[]), "");
        assert_eq!(extractor.extract(&[vec![Value::Null]]), "");
    }

    #[test]
    fn full_load_returns_schema_ddl_columns_and_paged_rows() {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut deps = deps(
            requests.clone(),
            vec![
                page(vec![vec![Some(Value::Number(1.into())), None]], Some(3)),
                page(
                    vec![
                        vec![Some(Value::Number(2.into())), None],
                        vec![Some(Value::Number(3.into())), None],
                    ],
                    Some(3),
                ),
            ],
        );
        let mut paged = params();
        paged.page_size = 1;
        let result = load_batch_export_table_data(&paged, &mut deps, &dialect_provider).unwrap();
        assert_eq!(result.table_name, "t");
        assert_eq!(result.schema.as_deref(), Some("public"));
        assert_eq!(result.ddl.as_deref(), Some("DDL_SQL=DDL(t);CREATE TABLE t"));
        assert_eq!(result.columns.len(), 2);
        assert_eq!(result.columns[0].name, "id");
        assert_eq!(result.rows.len(), 3);
        let captured = requests.borrow();
        assert_eq!(captured.len(), 2);
        assert!(!captured[0].skip_count);
        assert!(captured[1].skip_count);
        assert_eq!(captured[1].page, 1);
    }

    #[test]
    fn paging_stops_on_short_page_and_on_total_rows() {
        let short_requests = Rc::new(RefCell::new(Vec::new()));
        let mut short_deps = deps(short_requests.clone(), vec![page(vec![], None)]);
        let mut short_params = params();
        short_params.page_size = 2;
        assert_eq!(
            load_batch_export_table_data(&short_params, &mut short_deps, &dialect_provider)
                .unwrap()
                .rows
                .len(),
            0
        );
        assert_eq!(short_requests.borrow().len(), 1);

        let total_requests = Rc::new(RefCell::new(Vec::new()));
        let mut total_deps = deps(
            total_requests.clone(),
            vec![page(
                vec![vec![Some(Value::Number(1.into())), None]],
                Some(1),
            )],
        );
        let mut total_params = params();
        total_params.page_size = 1;
        assert_eq!(
            load_batch_export_table_data(&total_params, &mut total_deps, &dialect_provider)
                .unwrap()
                .rows
                .len(),
            1
        );
        assert_eq!(total_requests.borrow().len(), 1);
    }

    #[test]
    fn max_rows_truncates_the_last_page() {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut limit_deps = deps(
            requests,
            vec![page(
                vec![
                    vec![Some(Value::Number(1.into())), None],
                    vec![Some(Value::Number(2.into())), None],
                    vec![Some(Value::Number(3.into())), None],
                ],
                None,
            )],
        );
        let mut limit_params = params();
        limit_params.max_rows = 2;
        limit_params.page_size = 3;
        let result =
            load_batch_export_table_data(&limit_params, &mut limit_deps, &dialect_provider)
                .unwrap();
        assert_eq!(result.rows.len(), 2);
    }

    #[test]
    fn include_rows_false_skips_table_data_but_keeps_metadata() {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut skip_deps = deps(requests.clone(), Vec::new());
        let mut skip_params = params();
        skip_params.include_rows = false;
        let result =
            load_batch_export_table_data(&skip_params, &mut skip_deps, &dialect_provider).unwrap();
        assert!(result.rows.is_empty());
        assert_eq!(result.ddl.as_deref(), Some("DDL_SQL=DDL(t);CREATE TABLE t"));
        assert!(requests.borrow().is_empty());
    }

    #[test]
    fn missing_dialect_or_blank_ddl_yields_none() {
        let requests = Rc::new(RefCell::new(Vec::new()));
        let mut no_dialect = deps(requests, Vec::new());
        let mut no_dialect_params = params();
        no_dialect_params.database_type = Some("redis".to_owned());
        no_dialect_params.include_rows = false;
        let result =
            load_batch_export_table_data(&no_dialect_params, &mut no_dialect, &dialect_provider)
                .unwrap();
        assert_eq!(result.ddl, None);

        let mut blank = deps(Rc::new(RefCell::new(Vec::new())), Vec::new());
        blank.get_ddl = Box::new(|_, _, _, _, _| Ok("   ".to_owned()));
        let mut blank_params = params();
        blank_params.include_rows = false;
        let result =
            load_batch_export_table_data(&blank_params, &mut blank, &dialect_provider).unwrap();
        assert_eq!(result.ddl, None);
    }

    #[test]
    fn schema_fetch_failure_propagates_and_ddl_failure_is_ignored() {
        let mut failing = deps(Rc::new(RefCell::new(Vec::new())), Vec::new());
        failing.get_schema = Box::new(|_, _, _| Err("schema down".to_owned()));
        assert_eq!(
            load_batch_export_table_data(&params(), &mut failing, &dialect_provider).unwrap_err(),
            "schema down"
        );

        let mut ddl_fail = deps(Rc::new(RefCell::new(Vec::new())), Vec::new());
        ddl_fail.get_ddl = Box::new(|_, _, _, _, _| Err("ddl down".to_owned()));
        ddl_fail.get_table_data = Box::new(|_| Err("no rows".to_owned()));
        let mut no_rows = params();
        no_rows.include_rows = false;
        assert_eq!(
            load_batch_export_table_data(&no_rows, &mut ddl_fail, &dialect_provider)
                .unwrap()
                .ddl,
            None
        );
    }
}
