use super::*;
use crate::{DatabaseTypeMeta, TableContextInput};

fn registry() -> DatabaseRegistry {
    let mut registry = DatabaseRegistry::default();
    registry.register_metas(vec![(
        "mysql".to_owned(),
        DatabaseTypeMeta {
            quote_char: "`".to_owned(),
            ..Default::default()
        },
    )]);
    registry
}

fn input() -> TableContextInput {
    TableContextInput {
        connection_id: "connection-1".to_owned(),
        db_session_id: "session-1".to_owned(),
        database_type: "mysql".to_owned(),
        database: "app".to_owned(),
        schema: None,
        table_name: Some("order`items".to_owned()),
        table_schema: Some("sales".to_owned()),
        view_name: None,
        view_schema: None,
        object_type: None,
        name: None,
    }
}

#[test]
fn a_panel_shaped_input_produces_the_one_normalized_context() {
    let context = match build_table_context(&input()) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    assert_eq!(context.connection_id, "connection-1");
    assert_eq!(context.db_session_id, "session-1");
    assert_eq!(context.database_type, "mysql");
    assert_eq!(context.database, "app");
    assert_eq!(context.schema.as_deref(), Some("sales"));
    assert_eq!(context.table_name, "order`items");
}

#[test]
fn naming_fallbacks_and_trimming_are_pinned() {
    let mut view = input();
    view.table_name = None;
    view.view_name = Some("  v_orders  ".to_owned());
    view.table_schema = None;
    view.schema = Some(" raw ".to_owned());
    view.view_schema = Some("ignored".to_owned());
    let context = match build_table_context(&view) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    assert_eq!(context.table_name, "v_orders");
    assert_eq!(context.schema.as_deref(), Some("raw"));

    let mut named = input();
    named.table_name = None;
    named.name = Some("by_name".to_owned());
    let named_context = match build_table_context(&named) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    assert_eq!(named_context.table_name, "by_name");
}

#[test]
fn identifiers_are_quoted_through_driver_metadata() {
    let context = match build_table_context(&input()) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    let registry = registry();
    assert_eq!(quote_table_identifier(&context, &registry), "`sales`.`order``items`");
    assert_eq!(
        build_safe_table_sql_template(&context, TableSqlActionKind::Select, &registry),
        Some("SELECT * FROM `sales`.`order``items`;".to_owned())
    );
    assert_eq!(
        build_safe_table_sql_template(&context, TableSqlActionKind::OpenData, &registry),
        None
    );
}

#[test]
fn all_safe_templates_are_draft_text_with_no_guessed_columns() {
    let context = match build_table_context(&input()) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    let registry = registry();
    assert_eq!(
        build_safe_table_sql_template(&context, TableSqlActionKind::Insert, &registry),
        Some("INSERT INTO `sales`.`order``items` /* driver-generated columns and values */;".to_owned())
    );
    assert_eq!(
        build_safe_table_sql_template(&context, TableSqlActionKind::Update, &registry),
        Some("UPDATE `sales`.`order``items` SET /* driver-generated assignments */ WHERE /* driver-generated predicate */;".to_owned())
    );
    assert_eq!(
        build_safe_table_sql_template(&context, TableSqlActionKind::Ddl, &registry),
        Some("/* Driver-generated DDL for `sales`.`order``items`; use the existing DDL command. */".to_owned())
    );
}

#[test]
fn the_five_actions_keep_the_product_order_and_draft_only_contract() {
    let registry = registry();
    let actions = match build_table_sql_actions(&input(), None, &registry) {
        Ok(value) => value,
        Err(error) => panic!("actions should build: {error}"),
    };
    assert_eq!(
        actions.iter().map(|action| action.kind).collect::<Vec<_>>(),
        [
            TableSqlActionKind::OpenData,
            TableSqlActionKind::Select,
            TableSqlActionKind::Insert,
            TableSqlActionKind::Update,
            TableSqlActionKind::Ddl
        ]
    );
    assert!(actions
        .iter()
        .all(|action| action.execution == SqlActionExecution::DraftOnly));
    assert!(actions
        .iter()
        .all(|action| action.context.table_name == "order`items"));
    assert_eq!(actions[0].context.focus, QueryOpenFocus::Result);
    assert_eq!(actions[1].context.focus, QueryOpenFocus::Editor);
    assert_eq!(actions[0].label, "Open Data");
    assert_eq!(actions[2].label, "INSERT");
    assert!(actions[0].sql_template.is_none());
    assert!(actions[1].sql_template.as_deref().map(|sql| sql.contains("SELECT * FROM")).unwrap_or(false));
    assert!(!actions[0].requires_driver_generation);
    assert!(actions[2].requires_driver_generation);
    assert!(actions[3].requires_driver_generation);
    assert!(actions[4].requires_driver_generation);
    assert_eq!(actions[2].context, actions[2].query_open_context);
}

#[test]
fn explicit_specs_override_source_initial_sql_and_focus() {
    let registry = registry();
    let spec = TableSqlActionSpec {
        kind: TableSqlActionKind::Select,
        source: Some(QueryOpenSource::ObjectSearch),
        initial_sql: Some("SELECT 1".to_owned()),
        focus: Some(QueryOpenFocus::Result),
    };
    let context = match build_query_open_context(&input(), TableActionInput::Spec(&spec), &registry) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    assert_eq!(context.source, QueryOpenSource::ObjectSearch);
    assert_eq!(context.initial_sql.as_deref(), Some("SELECT 1"));
    assert_eq!(context.focus, QueryOpenFocus::Result);
    assert_eq!(context.action, TableSqlActionKind::Select);
    assert_eq!(context.database, "app");
    assert_eq!(context.schema.as_deref(), Some("sales"));
}

#[test]
fn an_empty_explicit_initial_sql_is_kept_instead_of_the_generic_select() {
    let registry = registry();
    let spec = TableSqlActionSpec {
        kind: TableSqlActionKind::Select,
        source: None,
        initial_sql: Some(String::new()),
        focus: None,
    };
    let context = match build_query_open_context(&input(), TableActionInput::Spec(&spec), &registry) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    assert_eq!(context.initial_sql.as_deref(), Some(""));
}

#[test]
fn incomplete_or_non_table_inputs_are_refused_with_exact_messages() {
    let mut non_table = input();
    non_table.object_type = Some("function".to_owned());
    non_table.name = Some("refresh_users".to_owned());
    assert_eq!(
        build_table_context(&non_table).err().as_deref(),
        Some("Object type function cannot produce a table context")
    );

    let mut blank = input();
    blank.table_name = Some("   ".to_owned());
    assert_eq!(
        build_table_context(&blank).err().as_deref(),
        Some("Missing tableName for table action")
    );

    let mut missing_session = input();
    missing_session.db_session_id = String::new();
    assert_eq!(
        build_table_context(&missing_session).err().as_deref(),
        Some("Missing dbSessionId for table action")
    );

    let mut allowed = input();
    allowed.object_type = Some(" MaterializedView ".to_owned());
    assert!(build_table_context(&allowed).is_ok());
}

#[test]
fn the_no_primary_key_escape_hatch_is_a_read_first_select_draft() {
    let registry = registry();
    let template = build_no_primary_key_edit_template(
        &match build_table_context(&input()) {
            Ok(value) => value,
            Err(error) => panic!("context should build: {error}"),
        },
        &registry,
    );
    let lines = template.lines().collect::<Vec<_>>();
    assert_eq!(lines.len(), 3);
    assert!(lines[0].starts_with("-- This table has no primary key"));
    assert!(lines[1].starts_with("-- Select the rows first, then write the UPDATE/DELETE"));
    assert_eq!(
        lines[2],
        "SELECT * FROM `sales`.`order``items` WHERE /* e.g. id = 123 */;"
    );
    assert!(template.ends_with(';'));

    let spec = match no_primary_key_edit_spec(&input(), &registry) {
        Ok(value) => value,
        Err(error) => panic!("spec should build: {error}"),
    };
    assert_eq!(spec.kind, TableSqlActionKind::Select);
    assert_eq!(spec.source, Some(QueryOpenSource::TableAction));
    assert_eq!(spec.focus, Some(QueryOpenFocus::Editor));
    assert_eq!(spec.initial_sql.as_deref(), Some(template.as_str()));

    let context = match build_query_open_context(&input(), TableActionInput::Spec(&spec), &registry) {
        Ok(value) => value,
        Err(error) => panic!("context should build: {error}"),
    };
    assert_eq!(context.action, TableSqlActionKind::Select);
    assert!(context.initial_sql.as_deref().map(|sql| sql.contains("WHERE")).unwrap_or(false));
    assert_ne!(
        context.initial_sql,
        build_safe_table_sql_template(
            &match build_table_context(&input()) {
                Ok(value) => value,
                Err(error) => panic!("context should build: {error}"),
            },
            TableSqlActionKind::Select,
            &registry
        )
    );
}

#[test]
fn a_custom_action_list_is_honored_in_order() {
    let registry = registry();
    let kinds = [TableSqlActionKind::Ddl, TableSqlActionKind::OpenData];
    let actions = match build_table_sql_actions(&input(), Some(&kinds), &registry) {
        Ok(value) => value,
        Err(error) => panic!("actions should build: {error}"),
    };
    assert_eq!(
        actions.iter().map(|action| action.kind).collect::<Vec<_>>(),
        kinds
    );
}

#[test]
fn wire_literals_are_pinned() {
    assert_eq!(QueryOpenSource::TableAction.as_str(), "table-action");
    assert_eq!(QueryOpenSource::ObjectSearch.as_str(), "object-search");
    assert_eq!(QueryOpenSource::AiAction.as_str(), "ai-action");
    assert_eq!(QueryOpenFocus::Editor.as_str(), "editor");
    assert_eq!(QueryOpenFocus::Result.as_str(), "result");
    assert_eq!(SqlActionExecution::DraftOnly.as_str(), "draft-only");
}

#[test]
fn crate_root_reexports_the_table_sql_action_surface() {
    fn resolves<T: 'static + ?Sized>() {}
    resolves::<crate::TableContext>();
    resolves::<crate::QueryOpenContext>();
    resolves::<crate::TableSqlAction>();
    resolves::<crate::TableSqlActionSpec>();
    resolves::<crate::QueryOpenSource>();
    resolves::<crate::QueryOpenFocus>();
    resolves::<crate::SqlActionExecution>();
    assert_eq!(crate::TABLE_SQL_ACTION_KINDS.len(), 5);
    assert_eq!(
        crate::build_table_sql_actions(&input(), None, &registry())
            .map(|actions| actions.len())
            .unwrap_or(0),
        5
    );
}
