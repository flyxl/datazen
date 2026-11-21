use serde_json::Value;

use crate::DdlQuery;

pub struct RelationDdlDialect {
    pub get_table_ddl_query: Box<dyn Fn(&str, Option<&str>) -> DdlQuery>,
    pub get_view_ddl_query: Option<Box<dyn Fn(&str, Option<&str>) -> DdlQuery>>,
}

pub type RelationDialectProvider = dyn Fn(&str) -> Option<RelationDdlDialect>;

pub struct DdlRowValueExtractor {
    index: usize,
}

impl DdlRowValueExtractor {
    pub fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn extract(&self, rows: &[Vec<Value>]) -> String {
        let Some(value) = rows.first().and_then(|row| row.get(self.index)) else {
            return String::new();
        };
        match value {
            Value::Null => String::new(),
            Value::String(text) => text.clone(),
            Value::Number(number) => number.to_string(),
            Value::Bool(flag) => flag.to_string(),
            Value::Array(items) => items
                .iter()
                .map(|item| match item {
                    Value::Null => String::new(),
                    Value::String(text) => text.clone(),
                    Value::Number(number) => number.to_string(),
                    Value::Bool(flag) => flag.to_string(),
                    Value::Array(_) | Value::Object(_) => "[object Object]".to_owned(),
                })
                .collect::<Vec<_>>()
                .join(","),
            Value::Object(_) => "[object Object]".to_owned(),
        }
    }
}

pub type CachedDdlFetcher =
    dyn FnMut(&str, &str, &str, &DdlRowValueExtractor, &str) -> Result<String, String>;

pub type ObjectDdlFetcher =
    dyn FnMut(&str, &str, &str, Option<&str>, &str) -> Result<String, String>;

pub fn fetch_relation_ddl(
    db_session_id: &str,
    table_name: &str,
    database: &str,
    database_type: Option<&str>,
    is_view: bool,
    schema: Option<&str>,
    dialect_provider: &RelationDialectProvider,
    get_cached_ddl: &mut CachedDdlFetcher,
    get_object_ddl: &mut ObjectDdlFetcher,
) -> String {
    if db_session_id.is_empty() || table_name.is_empty() || database.is_empty() {
        return String::new();
    }

    if let Some(dialect) = database_type.and_then(dialect_provider) {
        let query = if is_view {
            dialect
                .get_view_ddl_query
                .as_ref()
                .map(|build| build(table_name, schema))
                .unwrap_or_else(|| (dialect.get_table_ddl_query)(table_name, schema))
        } else {
            (dialect.get_table_ddl_query)(table_name, schema)
        };
        let extractor = DdlRowValueExtractor::new(query.extract_column_index);
        if let Ok(ddl) = get_cached_ddl(db_session_id, table_name, &query.sql, &extractor, database)
        {
            if !ddl.trim().is_empty() {
                return ddl;
            }
        }
    }

    let kind = if is_view { "view" } else { "table" };
    let effective_schema = schema.or(Some(database));
    if let Ok(ddl) = get_object_ddl(db_session_id, kind, table_name, effective_schema, database) {
        if !ddl.trim().is_empty() {
            return ddl;
        }
    }

    String::new()
}

pub struct ClipboardSteps<'a> {
    pub api: Option<&'a mut dyn FnMut(&str) -> Result<(), String>>,
    pub host: Option<&'a mut dyn FnMut(&str) -> Result<(), String>>,
    pub dom: Option<&'a mut dyn FnMut(&str) -> bool>,
}

pub fn copy_to_clipboard(text: &str, steps: &mut ClipboardSteps<'_>) -> bool {
    if text.is_empty() {
        return false;
    }
    if let Some(api) = steps.api.as_deref_mut() {
        if api(text).is_ok() {
            return true;
        }
    }
    if let Some(host) = steps.host.as_deref_mut() {
        if host(text).is_ok() {
            return true;
        }
    }
    if let Some(dom) = steps.dom.as_deref_mut() {
        return dom(text);
    }
    false
}

pub struct RichClipboardSteps<'a> {
    pub rich_api: Option<&'a mut dyn FnMut(&str, &str) -> Result<(), String>>,
    pub host_rich: Option<&'a mut dyn FnMut(&str, &str) -> Result<(), String>>,
    pub plain: ClipboardSteps<'a>,
}

pub fn copy_rich_text_to_clipboard(
    html: &str,
    text: &str,
    steps: &mut RichClipboardSteps<'_>,
) -> bool {
    if html.is_empty() {
        return copy_to_clipboard(text, &mut steps.plain);
    }
    if let Some(api) = steps.rich_api.as_deref_mut() {
        if api(html, text).is_ok() {
            return true;
        }
    }
    if let Some(host) = steps.host_rich.as_deref_mut() {
        if host(html, text).is_ok() {
            return true;
        }
    }
    copy_to_clipboard(text, &mut steps.plain)
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use serde_json::json;

    use super::*;

    fn query(sql: &str, index: usize) -> DdlQuery {
        DdlQuery {
            sql: sql.to_owned(),
            extract_column_index: index,
        }
    }

    fn dialect(table_sql: &str, view_sql: Option<&str>) -> RelationDdlDialect {
        let table_sql = table_sql.to_owned();
        RelationDdlDialect {
            get_table_ddl_query: Box::new(move |table: &str, schema: Option<&str>| {
                let suffix = schema.unwrap_or("");
                query(&format!("{table_sql}:{table}:{suffix}"), 0)
            }),
            get_view_ddl_query: view_sql.map(|sql| {
                let sql = sql.to_owned();
                Box::new(move |table: &str, schema: Option<&str>| {
                    let suffix = schema.unwrap_or("");
                    query(&format!("{sql}:{table}:{suffix}"), 0)
                }) as Box<dyn Fn(&str, Option<&str>) -> DdlQuery>
            }),
        }
    }

    fn no_dialect(_: &str) -> Option<RelationDdlDialect> {
        None
    }

    fn table_dialect(_: &str) -> Option<RelationDdlDialect> {
        Some(dialect("TABLE_SQL", None))
    }

    fn view_dialect(_: &str) -> Option<RelationDdlDialect> {
        Some(dialect("TABLE_SQL", Some("VIEW_SQL")))
    }

    fn make_cached(
        value: &str,
        calls: Rc<RefCell<Vec<String>>>,
    ) -> impl FnMut(&str, &str, &str, &DdlRowValueExtractor, &str) -> Result<String, String> {
        let value = value.to_owned();
        move |_, _, sql, _, _| {
            calls.borrow_mut().push(sql.to_owned());
            Ok(value.clone())
        }
    }

    fn make_object(
        value: &str,
        calls: Rc<RefCell<Vec<(String, String, Option<String>)>>>,
    ) -> impl FnMut(&str, &str, &str, Option<&str>, &str) -> Result<String, String> {
        let value = value.to_owned();
        move |_, kind, table, schema, _| {
            calls
                .borrow_mut()
                .push((kind.to_owned(), table.to_owned(), schema.map(str::to_owned)));
            Ok(value.clone())
        }
    }

    #[test]
    fn empty_inputs_short_circuit_before_any_seam_runs() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut cached = make_cached("CREATE TABLE x", calls.clone());
        let object_calls = Rc::new(RefCell::new(Vec::new()));
        let mut object = make_object("FALLBACK", object_calls.clone());
        assert_eq!(
            fetch_relation_ddl(
                "",
                "t",
                "db",
                Some("pg"),
                false,
                None,
                &table_dialect,
                &mut cached,
                &mut object
            ),
            ""
        );
        assert_eq!(
            fetch_relation_ddl(
                "s",
                "",
                "db",
                Some("pg"),
                false,
                None,
                &table_dialect,
                &mut cached,
                &mut object
            ),
            ""
        );
        assert_eq!(
            fetch_relation_ddl(
                "s",
                "t",
                "",
                Some("pg"),
                false,
                None,
                &table_dialect,
                &mut cached,
                &mut object
            ),
            ""
        );
        assert!(calls.borrow().is_empty());
        assert!(object_calls.borrow().is_empty());
    }

    #[test]
    fn cached_ddl_wins_and_uses_the_table_query_with_schema() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut cached = make_cached("  CREATE TABLE ok  ", calls.clone());
        let object_calls = Rc::new(RefCell::new(Vec::new()));
        let mut object = make_object("FALLBACK", object_calls.clone());
        let ddl = fetch_relation_ddl(
            "s",
            "users",
            "main",
            Some("pg"),
            false,
            Some("public"),
            &table_dialect,
            &mut cached,
            &mut object,
        );
        assert_eq!(ddl, "  CREATE TABLE ok  ");
        assert_eq!(
            calls.borrow().as_slice(),
            &["TABLE_SQL:users:public".to_owned()]
        );
        assert!(object_calls.borrow().is_empty());
    }

    #[test]
    fn view_query_is_chosen_only_when_the_dialect_offers_it() {
        for (provider, expected) in [
            (&view_dialect as &RelationDialectProvider, "VIEW_SQL:t:"),
            (&table_dialect as &RelationDialectProvider, "TABLE_SQL:t:"),
        ] {
            let calls = Rc::new(RefCell::new(Vec::new()));
            let mut cached = make_cached("DDL", calls.clone());
            let object_calls = Rc::new(RefCell::new(Vec::new()));
            let mut object = make_object("FALLBACK", object_calls.clone());
            fetch_relation_ddl(
                "s",
                "t",
                "main",
                Some("pg"),
                true,
                None,
                provider,
                &mut cached,
                &mut object,
            );
            assert_eq!(calls.borrow().as_slice(), &[expected.to_owned()]);
        }
    }

    #[test]
    fn cached_errors_and_blank_results_fall_through_to_object_ddl() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let failing_calls = calls.clone();
        let mut failing = move |_: &str,
                                _: &str,
                                _: &str,
                                _: &DdlRowValueExtractor,
                                _: &str|
              -> Result<String, String> {
            failing_calls.borrow_mut().push("called".to_owned());
            Err("boom".to_owned())
        };
        let object_calls = Rc::new(RefCell::new(Vec::new()));
        let mut object = make_object("  FALLBACK DDL  ", object_calls.clone());
        let ddl = fetch_relation_ddl(
            "s",
            "t",
            "db",
            Some("pg"),
            false,
            Some("sch"),
            &table_dialect,
            &mut failing,
            &mut object,
        );
        assert_eq!(ddl, "  FALLBACK DDL  ");
        assert_eq!(
            object_calls.borrow().as_slice(),
            &[("table".to_owned(), "t".to_owned(), Some("sch".to_owned()))]
        );

        let calls2 = Rc::new(RefCell::new(Vec::new()));
        let mut blank = make_cached("   ", calls2.clone());
        let object_calls2 = Rc::new(RefCell::new(Vec::new()));
        let mut object2 = make_object("SECOND", object_calls2.clone());
        let ddl = fetch_relation_ddl(
            "s",
            "t",
            "db",
            Some("pg"),
            false,
            None,
            &table_dialect,
            &mut blank,
            &mut object2,
        );
        assert_eq!(ddl, "SECOND");
        assert_eq!(object_calls2.borrow()[0].2, Some("db".to_owned()));
    }

    #[test]
    fn missing_dialect_skips_the_cached_path_and_empty_schema_is_kept() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut cached = make_cached("NEVER", calls.clone());
        let object_calls = Rc::new(RefCell::new(Vec::new()));
        let mut object = make_object("OBJ", object_calls.clone());
        let ddl = fetch_relation_ddl(
            "s",
            "t",
            "db",
            Some("pg"),
            true,
            Some(""),
            &no_dialect,
            &mut cached,
            &mut object,
        );
        assert_eq!(ddl, "OBJ");
        assert!(calls.borrow().is_empty());
        assert_eq!(object_calls.borrow()[0].0, "view");
        assert_eq!(object_calls.borrow()[0].2, Some(String::new()));
    }

    #[test]
    fn both_seams_failing_or_blank_returns_empty() {
        let calls = Rc::new(RefCell::new(Vec::new()));
        let mut cached = make_cached("", calls.clone());
        let object_calls: Rc<RefCell<Vec<(String, String, Option<String>)>>> =
            Rc::new(RefCell::new(Vec::new()));
        let mut failing_object =
            move |_: &str, _: &str, _: &str, _: Option<&str>, _: &str| -> Result<String, String> {
                Err("no ddl".to_owned())
            };
        assert_eq!(
            fetch_relation_ddl(
                "s",
                "t",
                "db",
                None,
                false,
                None,
                &table_dialect,
                &mut cached,
                &mut failing_object
            ),
            ""
        );
        assert!(calls.borrow().is_empty());
        assert!(object_calls.borrow().is_empty());
    }

    #[test]
    fn extractor_matches_js_string_conversion() {
        let extract = DdlRowValueExtractor::new(1);
        let rows = vec![vec![Value::Null, Value::String("text".to_owned())]];
        assert_eq!(extract.extract(&rows), "text");
        assert_eq!(DdlRowValueExtractor::new(0).extract(&rows), "");
        assert_eq!(DdlRowValueExtractor::new(5).extract(&rows), "");
        assert_eq!(DdlRowValueExtractor::new(9).extract(&[]), "");
        let rows = vec![vec![Value::Null, json!(7)]];
        assert_eq!(extract.extract(&rows), "7");
        let rows = vec![vec![Value::Null, json!([1, null, "x"])]];
        assert_eq!(extract.extract(&rows), "1,,x");
        let rows = vec![vec![Value::Null, json!({"a": 1})]];
        assert_eq!(extract.extract(&rows), "[object Object]");
        let rows = vec![vec![Value::Null, json!(true)]];
        assert_eq!(extract.extract(&rows), "true");
    }

    #[test]
    fn clipboard_tries_api_then_host_then_dom() {
        let mut api = |_: &str| Err("denied".to_owned());
        let mut host = |_: &str| Ok(());
        let mut dom = |_: &str| false;
        let mut steps = ClipboardSteps {
            api: Some(&mut api),
            host: Some(&mut host),
            dom: Some(&mut dom),
        };
        assert!(copy_to_clipboard("payload", &mut steps));
    }

    #[test]
    fn clipboard_empty_text_and_dom_result() {
        let mut api = |_: &str| Ok(());
        let mut steps = ClipboardSteps {
            api: Some(&mut api),
            host: None,
            dom: None,
        };
        assert!(!copy_to_clipboard("", &mut steps));
        let mut dom = |_: &str| false;
        let mut steps = ClipboardSteps {
            api: None,
            host: None,
            dom: Some(&mut dom),
        };
        assert!(!copy_to_clipboard("x", &mut steps));
        let mut steps = ClipboardSteps {
            api: None,
            host: None,
            dom: None,
        };
        assert!(!copy_to_clipboard("x", &mut steps));
    }

    #[test]
    fn rich_clipboard_empty_html_uses_the_plain_chain() {
        let mut api = |_: &str| Ok(());
        let mut rich_api = |_: &str, _: &str| -> Result<(), String> { Ok(()) };
        let mut steps = RichClipboardSteps {
            rich_api: Some(&mut rich_api),
            host_rich: None,
            plain: ClipboardSteps {
                api: Some(&mut api),
                host: None,
                dom: None,
            },
        };
        assert!(copy_rich_text_to_clipboard("", "text", &mut steps));
    }

    #[test]
    fn rich_clipboard_tries_rich_api_then_host_then_plain() {
        let mut rich_api = |_: &str, _: &str| Err("unsupported".to_owned());
        let mut host_rich = |html: &str, _: &str| {
            assert_eq!(html, "<b>x</b>");
            Ok(())
        };
        let mut plain_api = |_: &str| Ok(());
        let mut steps = RichClipboardSteps {
            rich_api: Some(&mut rich_api),
            host_rich: Some(&mut host_rich),
            plain: ClipboardSteps {
                api: Some(&mut plain_api),
                host: None,
                dom: None,
            },
        };
        assert!(copy_rich_text_to_clipboard("<b>x</b>", "x", &mut steps));
    }

    #[test]
    fn rich_clipboard_falls_back_to_plain_text_when_rich_steps_fail() {
        let mut rich_api = |_: &str, _: &str| Err("no".to_owned());
        let mut host_rich = |_: &str, _: &str| Err("no".to_owned());
        let mut plain_api = |text: &str| {
            assert_eq!(text, "plain");
            Ok(())
        };
        let mut steps = RichClipboardSteps {
            rich_api: Some(&mut rich_api),
            host_rich: Some(&mut host_rich),
            plain: ClipboardSteps {
                api: Some(&mut plain_api),
                host: None,
                dom: None,
            },
        };
        assert!(copy_rich_text_to_clipboard("<b>x</b>", "plain", &mut steps));
    }
}
