use crate::SingleTableStructureExportResult;
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportTableStructureResult {
    Saved,
    Cancelled,
    Unsupported,
}

impl ExportTableStructureResult {
    pub fn as_str(self) -> &'static str {
        match self {
            ExportTableStructureResult::Saved => "saved",
            ExportTableStructureResult::Cancelled => "cancelled",
            ExportTableStructureResult::Unsupported => "unsupported",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DdlQuery {
    pub sql: String,
    pub extract_column_index: usize,
}

pub struct DdlDialect {
    pub get_table_ddl_query: Box<dyn Fn(&str) -> DdlQuery>,
}

pub struct DdlRowExtractor {
    extract: Box<dyn Fn(&[Vec<Value>]) -> String>,
}

impl DdlRowExtractor {
    pub fn extract(&self, rows: &[Vec<Value>]) -> String {
        (self.extract)(rows)
    }
}

pub type SqlDialectProvider = dyn Fn(&str) -> Option<DdlDialect>;
pub type DdlFetcher =
    dyn FnMut(&str, &str, &str, &DdlRowExtractor, &str) -> Result<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StructureExportInput {
    pub db_session_id: String,
    pub database_type: String,
    pub database: String,
    pub schema: Option<String>,
    pub table_name: String,
    pub ddl: String,
}

pub type StructureExporter =
    dyn FnMut(&StructureExportInput) -> Result<SingleTableStructureExportResult, String>;

fn js_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Number(number) => number.to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Null => "null".to_owned(),
        Value::Array(items) => items
            .iter()
            .map(|item| if item.is_null() { String::new() } else { js_string(item) })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_owned(),
    }
}

pub fn export_table_structure_to_file(
    db_session_id: &str,
    table_name: &str,
    database: &str,
    database_type: &str,
    schema: Option<&str>,
    dialect_provider: &SqlDialectProvider,
    get_ddl: &mut DdlFetcher,
    export_structure: &mut StructureExporter,
) -> Result<ExportTableStructureResult, String> {
    let Some(dialect) = dialect_provider(database_type) else {
        return Ok(ExportTableStructureResult::Unsupported);
    };
    let query = (dialect.get_table_ddl_query)(table_name);
    let extract_column_index = query.extract_column_index;
    let extract = DdlRowExtractor {
        extract: Box::new(move |rows: &[Vec<Value>]| match rows.first().and_then(|row| row.get(extract_column_index)) {
            Some(Value::String(text)) => text.clone(),
            Some(Value::Null) | None => String::new(),
            Some(value) => js_string(value),
        }),
    };
    let ddl = get_ddl(db_session_id, table_name, &query.sql, &extract, database)?;
    let input = StructureExportInput {
        db_session_id: db_session_id.to_owned(),
        database_type: database_type.to_owned(),
        database: database.to_owned(),
        schema: schema.map(str::to_owned),
        table_name: table_name.to_owned(),
        ddl,
    };
    let result = export_structure(&input)?;
    Ok(match result {
        SingleTableStructureExportResult::Saved => ExportTableStructureResult::Saved,
        SingleTableStructureExportResult::Cancelled => ExportTableStructureResult::Cancelled,
    })
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;

    fn dialect(sql: &str, index: usize) -> DdlDialect {
        let sql = sql.to_owned();
        DdlDialect {
            get_table_ddl_query: Box::new(move |table| DdlQuery {
                sql: format!("{sql}:{table}"),
                extract_column_index: index,
            }),
        }
    }

    #[test]
    fn missing_dialect_returns_unsupported_without_io() {
        let mut get_ddl = |_: &str, _: &str, _: &str, _: &DdlRowExtractor, _: &str| -> Result<String, String> {
            panic!("get_ddl must not run")
        };
        let mut exporter = |_: &StructureExportInput| -> Result<SingleTableStructureExportResult, String> {
            panic!("export must not run")
        };
        let result = export_table_structure_to_file(
            "s1", "t", "db", "none", None, &|_| None, &mut get_ddl, &mut exporter,
        ).expect("ok");
        assert_eq!(result, ExportTableStructureResult::Unsupported);
        assert_eq!(result.as_str(), "unsupported");
    }

    #[test]
    fn ddl_is_fetched_and_forwarded_with_the_exact_query_and_schema() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut get_ddl = move |session: &str, table: &str, sql: &str, _: &DdlRowExtractor, database: &str| -> Result<String, String> {
            sink.borrow_mut().push(format!("{session}|{table}|{sql}|{database}"));
            Ok("CREATE TABLE t (...)".to_owned())
        };
        let exports = Rc::new(RefCell::new(Vec::new()));
        let export_sink = exports.clone();
        let mut exporter = move |input: &StructureExportInput| -> Result<SingleTableStructureExportResult, String> {
            export_sink.borrow_mut().push(input.clone());
            Ok(SingleTableStructureExportResult::Saved)
        };
        let provider = |db_type: &str| {
            assert_eq!(db_type, "postgres");
            Some(dialect("SHOW DDL", 1))
        };
        let result = export_table_structure_to_file(
            "s1", "users", "main", "postgres", Some("public"), &provider, &mut get_ddl, &mut exporter,
        ).expect("ok");
        assert_eq!(result, ExportTableStructureResult::Saved);
        assert_eq!(result.as_str(), "saved");
        assert_eq!(seen.borrow().as_slice(), &["s1|users|SHOW DDL:users|main".to_owned()]);
        let inputs = exports.borrow();
        assert_eq!(inputs.len(), 1);
        assert_eq!(inputs[0].db_session_id, "s1");
        assert_eq!(inputs[0].database_type, "postgres");
        assert_eq!(inputs[0].database, "main");
        assert_eq!(inputs[0].schema.as_deref(), Some("public"));
        assert_eq!(inputs[0].table_name, "users");
        assert_eq!(inputs[0].ddl, "CREATE TABLE t (...)");
    }

    #[test]
    fn absent_schema_stays_absent() {
        let mut get_ddl = |_: &str, _: &str, _: &str, _: &DdlRowExtractor, _: &str| -> Result<String, String> {
            Ok(String::new())
        };
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut exporter = move |input: &StructureExportInput| -> Result<SingleTableStructureExportResult, String> {
            sink.borrow_mut().push(input.schema.clone());
            Ok(SingleTableStructureExportResult::Cancelled)
        };
        let result = export_table_structure_to_file(
            "s1", "t", "db", "pg", None, &|_| Some(dialect("q", 0)), &mut get_ddl, &mut exporter,
        ).expect("ok");
        assert_eq!(result, ExportTableStructureResult::Cancelled);
        assert_eq!(seen.borrow().as_slice(), &[None]);
    }

    #[test]
    fn extractor_handles_string_number_null_and_empty_rows() {
        let seen = Rc::new(RefCell::new(Vec::new()));
        let sink = seen.clone();
        let mut get_ddl = move |_: &str, _: &str, _: &str, extract: &DdlRowExtractor, _: &str| -> Result<String, String> {
            sink.borrow_mut().push(extract.extract(&[]));
            sink.borrow_mut().push(extract.extract(&[vec![Value::Null, Value::String("DDL".into())]]));
            sink.borrow_mut().push(extract.extract(&[vec![Value::String("x".into()), Value::from(42)]]));
            sink.borrow_mut().push(extract.extract(&[vec![Value::String("x".into()), Value::Null]]));
            sink.borrow_mut().push(extract.extract(&[vec![Value::String("x".into()), json_bool()]]));
            Ok("d".to_owned())
        };
        let mut exporter = |_: &StructureExportInput| -> Result<SingleTableStructureExportResult, String> {
            Ok(SingleTableStructureExportResult::Saved)
        };
        export_table_structure_to_file(
            "s", "t", "db", "pg", None, &|_| Some(dialect("q", 1)), &mut get_ddl, &mut exporter,
        ).expect("ok");
        assert_eq!(
            seen.borrow().as_slice(),
            &["".to_owned(), "DDL".to_owned(), "42".to_owned(), "".to_owned(), "true".to_owned()]
        );
    }

    fn json_bool() -> Value {
        Value::Bool(true)
    }

    #[test]
    fn ddl_and_exporter_errors_propagate() {
        let mut get_ddl = |_: &str, _: &str, _: &str, _: &DdlRowExtractor, _: &str| -> Result<String, String> {
            Err("ddl failed".to_owned())
        };
        let mut exporter = |_: &StructureExportInput| -> Result<SingleTableStructureExportResult, String> {
            panic!("export must not run")
        };
        assert_eq!(
            export_table_structure_to_file(
                "s", "t", "db", "pg", None, &|_| Some(dialect("q", 0)), &mut get_ddl, &mut exporter,
            ).unwrap_err(),
            "ddl failed"
        );

        let mut get_ddl = |_: &str, _: &str, _: &str, _: &DdlRowExtractor, _: &str| -> Result<String, String> {
            Ok("d".to_owned())
        };
        let mut exporter = |_: &StructureExportInput| -> Result<SingleTableStructureExportResult, String> {
            Err("export failed".to_owned())
        };
        assert_eq!(
            export_table_structure_to_file(
                "s", "t", "db", "pg", None, &|_| Some(dialect("q", 0)), &mut get_ddl, &mut exporter,
            ).unwrap_err(),
            "export failed"
        );
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::export_table_structure_to_file;
        let _: Option<crate::ExportTableStructureResult> = None;
        let _: Option<crate::DdlQuery> = None;
        let _: Option<crate::DdlDialect> = None;
        let _: Option<crate::DdlRowExtractor> = None;
        let _: Option<crate::StructureExportInput> = None;
        let _: Option<&crate::SqlDialectProvider> = None;
    }
}
