//! T606: `tableEditing.ts` decisions as pure functions — the panel/cell
//! editability order, the editor navigation guard, the candidate loader's two
//! branches, and the "Set Value" gates/routing. No store, IPC or timer.

use std::collections::BTreeMap;

use datazen_ui::support::cell_types::CellTypeHints;
use datazen_ui::views::table_editing::{
    build_panel_column_editor_info, candidate_request_plan, derive_editing_context,
    enum_candidate_values, fk_candidate_values, guard_navigation, load_candidate_values,
    resolve_cell_editable, resolve_editor_before_navigation, resolve_panel_cell_editability,
    resolve_panel_cell_read_only_reason, resolve_panel_read_only_reason, resolve_set_value_actions,
    resolve_set_value_state, request_edit_plan, route_set_value, with_dialog_host_gate,
    CandidateLoaderArgsVm, CandidateRequestPlanVm, CandidateValueVm, CellEditabilityInputVm,
    CellValueErrorReason, EditingColumnVm, EditingContextVm, EditingCoordVm, EditingPanelFactsVm,
    EditingReadOnlyReason, EditingRowKind, EnumQueryRequestVm, FkCandidateErrorVm,
    FkCandidateSourceVm, FkResolutionVm, QueryResultVm, QueryStatementVm, RelationKind,
    SetValueActionId, SetValueRowFactsVm, SetValueStateVm, StagedIntentVm, NOT_UNIQUE_ERROR,
};

fn column(name: &str, data_type: &str) -> EditingColumnVm {
    EditingColumnVm {
        name: name.to_owned(),
        data_type: Some(data_type.to_owned()),
        nullable: name != "id",
        is_primary_key: name == "id",
        is_auto_increment: name == "token",
    }
}

fn facts() -> EditingPanelFactsVm {
    EditingPanelFactsVm {
        panel_id: "p1".to_owned(),
        connection_read_only: false,
        mount_reason: None,
        columns: [
            column("id", "integer"),
            column("created_at", "date"),
            column("seen_at", "timestamptz"),
            column("stamp", "timestamp"),
            column("avatar", "bytea"),
            column("kind", "enum"),
            column("note", "text"),
            column("token", "bytea"),
        ]
        .to_vec(),
    }
}

fn context() -> EditingContextVm {
    let meta = datazen_ui::views::table_editing::DriverEditingMetaVm {
        editable: Some(true),
        current_datetime_write: Some(true),
        hints: None,
    };
    derive_editing_context(&facts(), Some(&meta))
}

fn edit_input() -> CellEditabilityInputVm {
    CellEditabilityInputVm {
        mount_reason: None,
        connection_read_only: false,
        capability_editable: Some(true),
        relation_kind: RelationKind::Table,
        has_primary_key: true,
        row_identity_unique: true,
        column_is_auto_increment: false,
        cell_type: datazen_ui::support::cell_types::CellType::Text,
        in_memory_byte_size: 0,
        type_read_only: false,
        row_kind: EditingRowKind::Update,
    }
}

#[test]
fn context_folds_columns_and_driver_facts_once() {
    let ctx = context();
    assert!(ctx.has_columns && ctx.has_primary_key, "id is the primary key");
    assert_eq!(ctx.capability_editable, Some(true));
    assert_eq!(ctx.current_datetime_write, Some(true));
    let bare = derive_editing_context(&EditingPanelFactsVm::default(), None);
    assert!(!bare.has_columns && !bare.has_primary_key, "an unloaded panel has no columns");
    assert_eq!(bare.capability_editable, None, "absent capability means editable");
    let info = build_panel_column_editor_info(&ctx);
    assert!(info["id"].integer_only && info["kind"].nullable);
    assert_eq!(info["created_at"].cell_type, datazen_ui::support::cell_types::CellType::Date);
}

#[test]
fn panel_read_only_reason_follows_the_react_order() {
    let mut ctx = context();
    ctx.mount_reason = Some(EditingReadOnlyReason::View);
    ctx.connection_read_only = true;
    assert_eq!(
        resolve_panel_read_only_reason(&ctx),
        Some(EditingReadOnlyReason::View),
        "the mount point's reason comes first"
    );
    ctx.mount_reason = None;
    assert_eq!(resolve_panel_read_only_reason(&ctx), Some(EditingReadOnlyReason::ConnectionReadOnly));
    ctx.connection_read_only = false;
    ctx.capability_editable = Some(false);
    assert_eq!(resolve_panel_read_only_reason(&ctx), Some(EditingReadOnlyReason::DriverCapability));
    ctx.capability_editable = Some(true);
    ctx.relation_kind = RelationKind::View;
    assert_eq!(resolve_panel_read_only_reason(&ctx), Some(EditingReadOnlyReason::View));
    ctx.relation_kind = RelationKind::Table;

    let mut keyless = facts();
    for entry in &mut keyless.columns {
        entry.is_primary_key = false;
    }
    let keyless = derive_editing_context(&keyless, None);
    assert_eq!(resolve_panel_read_only_reason(&keyless), Some(EditingReadOnlyReason::NoPrimaryKey));
    let bare = derive_editing_context(&EditingPanelFactsVm::default(), None);
    assert_eq!(resolve_panel_read_only_reason(&bare), None, "not loaded is not a refusal");
}

#[test]
fn cell_read_only_reason_prefers_the_panel_then_the_column() {
    let mut ctx = context();
    assert_eq!(
        resolve_panel_cell_read_only_reason(&ctx, "avatar"),
        Some(EditingReadOnlyReason::BinaryColumn)
    );
    ctx.connection_read_only = true;
    assert_eq!(
        resolve_panel_cell_read_only_reason(&ctx, "avatar"),
        Some(EditingReadOnlyReason::ConnectionReadOnly),
        "a panel reason hides the column reason"
    );
    ctx.connection_read_only = false;
    ctx.hints = Some(CellTypeHints {
        read_only_types: vec!["text".to_owned()],
        ..Default::default()
    });
    assert_eq!(
        resolve_panel_cell_read_only_reason(&ctx, "note"),
        Some(EditingReadOnlyReason::TypeNotEditable),
        "a driver-declared type is a column refusal"
    );
}

#[test]
fn cell_editability_pins_the_whole_gate_order_and_messages() {
    use EditingReadOnlyReason::*;
    let mut input = edit_input();
    assert!(resolve_cell_editable(&input).editable);
    let ok = resolve_cell_editable(&input);
    assert_eq!((ok.reason, ok.code, ok.message_key), (None, None, ""));

    input.mount_reason = Some(Generated);
    assert_eq!(resolve_cell_editable(&input).reason, Some(Generated));
    input.mount_reason = None;
    input.connection_read_only = true;
    assert_eq!(resolve_cell_editable(&input).reason, Some(ConnectionReadOnly));
    input.connection_read_only = false;
    input.capability_editable = Some(false);
    assert_eq!(resolve_cell_editable(&input).reason, Some(DriverCapability));
    input.capability_editable = Some(true);
    input.relation_kind = RelationKind::View;
    assert_eq!(resolve_cell_editable(&input).reason, Some(View));
    input.relation_kind = RelationKind::Table;
    input.has_primary_key = false;
    assert_eq!(resolve_cell_editable(&input).reason, Some(NoPrimaryKey));
    input.has_primary_key = true;
    input.row_identity_unique = false;
    assert_eq!(resolve_cell_editable(&input).reason, Some(AmbiguousRowIdentity));
    input.row_identity_unique = true;
    input.column_is_auto_increment = true;
    assert_eq!(resolve_cell_editable(&input).reason, Some(AutoIncrement));
    input.row_kind = EditingRowKind::Insert;
    assert!(resolve_cell_editable(&input).editable, "a draft row may fill an identity column");
    input.row_kind = EditingRowKind::Update;
    input.column_is_auto_increment = false;
    input.type_read_only = true;
    assert_eq!(resolve_cell_editable(&input).reason, Some(TypeNotEditable));
    input.type_read_only = false;
    input.cell_type = datazen_ui::support::cell_types::CellType::Bytes;
    assert_eq!(resolve_cell_editable(&input).reason, Some(BinaryColumn));
    input.cell_type = datazen_ui::support::cell_types::CellType::Text;
    input.in_memory_byte_size = 64 * 1024 + 1;
    let refused = resolve_cell_editable(&input);
    assert_eq!(refused.reason, Some(LargeValue));
    assert_eq!(refused.code, Some("grid.cell.readOnly"));
    assert_eq!(refused.message_key, "cellEditor.readOnly.largeValue");
}

#[test]
fn request_edit_opens_only_after_the_editability_check() {
    let ctx = context();
    let coord = EditingCoordVm {
        row_index: 3,
        column_name: "note".to_owned(),
    };
    let (ok, effect) = request_edit_plan(Some(&ctx), &coord);
    assert!(ok.editable);
    assert_eq!(effect.unwrap().column, "note");
    let (refused, effect) = request_edit_plan(None, &coord);
    assert_eq!(refused.message_key, "cellEditor.readOnly.connection");
    assert!(effect.is_none(), "a refusal never opens an editor");

    let bytes = EditingCoordVm { row_index: 0, column_name: "avatar".to_owned() };
    let (refused, effect) = request_edit_plan(Some(&ctx), &bytes);
    assert!(!refused.editable && effect.is_none());
    assert!(
        !resolve_panel_cell_editability(Some(&ctx), "token").editable,
        "an auto-increment column refuses an inline edit"
    );
}

#[test]
fn navigation_guard_only_commits_this_panels_editor() {
    use datazen_ui::views::table_editing::OpenEditorVm;
    assert!(resolve_editor_before_navigation("p1", None).ok);
    let other = OpenEditorVm {
        panel_id: "p2".to_owned(),
        commit: Some(CellValueErrorReason::InvalidDate),
    };
    let plan = guard_navigation("p1", Some(&other));
    assert!(!plan.commit_editor && plan.navigate, "another panel's editor cannot hold our draft");

    let bad = OpenEditorVm {
        panel_id: "p1".to_owned(),
        commit: Some(CellValueErrorReason::EmptyNotNull),
    };
    let guard = resolve_editor_before_navigation("p1", Some(&bad));
    assert!(!guard.ok);
    assert_eq!(guard.reason, Some(CellValueErrorReason::EmptyNotNull));
    let plan = guard_navigation("p1", Some(&bad));
    assert!(plan.commit_editor && !plan.navigate, "a refused draft stops the navigation");
    assert_eq!(guard.reason.expect("reason").message_key(), "cellEditor.notNull");

    let good = OpenEditorVm { panel_id: "p1".to_owned(), commit: None };
    let plan = guard_navigation("p1", Some(&good));
    assert!(plan.commit_editor && plan.navigate && plan.reason.is_none());
    for (reason, key) in [
        (CellValueErrorReason::InvalidNumber, "cellEditor.invalidNumber"),
        (CellValueErrorReason::InvalidInteger, "cellEditor.invalidInteger"),
        (CellValueErrorReason::InvalidBoolean, "cellEditor.invalidBoolean"),
        (CellValueErrorReason::InvalidDate, "cellEditor.invalidDate"),
        (CellValueErrorReason::InvalidTime, "cellEditor.invalidTime"),
        (CellValueErrorReason::InvalidDateTime, "cellEditor.invalidDateTime"),
        (CellValueErrorReason::InvalidJson, "cellEditor.invalidJson"),
        (CellValueErrorReason::InvalidUuid, "cellEditor.invalidUuid"),
        (CellValueErrorReason::TooLarge, "cellEditor.valueTooLarge"),
        (CellValueErrorReason::NotEditable, "cellEditor.binaryNotEditable"),
    ] {
        assert_eq!(reason.message_key(), key);
    }
}

fn loader_args() -> CandidateLoaderArgsVm {
    CandidateLoaderArgsVm {
        db_session_id: "session-1".to_owned(),
        table: "orders".to_owned(),
        schema: Some("public".to_owned()),
        database: Some("app".to_owned()),
        distinct_sql: Some("SELECT DISTINCT \"status\" FROM \"orders\" LIMIT 200".to_owned()),
        fk_sources: BTreeMap::new(),
    }
}

#[test]
fn candidate_request_prefers_the_fk_browse_and_pins_its_wire_shape() {
    let mut args = loader_args();
    args.fk_sources.insert(
        "customer_id".to_owned(),
        FkCandidateSourceVm {
            local_column: "customer_id".to_owned(),
            constraint_name: Some("orders_customer_fk".to_owned()),
        },
    );
    let plan = candidate_request_plan("customer_id", &args);
    let CandidateRequestPlanVm::FkBrowse(request) = &plan else {
        panic!("a single-column FK must take the browse branch");
    };
    assert_eq!(request.columns, vec!["customer_id".to_owned()]);
    assert_eq!(request.constraint_name.as_deref(), Some("orders_customer_fk"));
    assert!(request.values.is_empty() && request.browse);
    assert_eq!(request.limit, 50, "the selector's hard cap");
    assert_eq!(request.db_session_id, "session-1");
    assert_eq!(request.table, "orders");

    let enum_plan = candidate_request_plan("status", &args);
    assert_eq!(
        enum_plan,
        CandidateRequestPlanVm::EnumQuery(EnumQueryRequestVm {
            db_session_id: "session-1".to_owned(),
            sql: args.distinct_sql.unwrap(),
            database: Some("app".to_owned()),
            schema: Some("public".to_owned()),
        })
    );
}

#[test]
fn candidate_request_has_no_sql_branch_and_no_implicit_query() {
    let mut args = loader_args();
    args.distinct_sql = Some(String::new());
    assert_eq!(
        candidate_request_plan("status", &args),
        CandidateRequestPlanVm::NoSql,
        "an empty statement means no candidates"
    );
    assert!(load_candidate_values(&CandidateRequestPlanVm::NoSql, None, None).unwrap().is_empty());
}

#[test]
fn enum_candidates_read_the_first_statement_and_skip_nulls() {
    let result = QueryResultVm {
        statements: vec![QueryStatementVm {
            rows: vec![
                vec![CandidateValueVm::Text("a".to_owned())],
                vec![CandidateValueVm::Null],
                vec![CandidateValueVm::Number(2.to_string())],
                vec![CandidateValueVm::Bool(true), CandidateValueVm::Text("ignored".to_owned())],
            ],
        }],
    };
    assert_eq!(
        enum_candidate_values(Some(&result)),
        vec!["a".to_owned(), "2".to_owned(), "true".to_owned()]
    );
    assert!(enum_candidate_values(None).is_empty());
    assert!(enum_candidate_values(Some(&QueryResultVm::default())).is_empty());
}

#[test]
fn fk_candidates_refuse_non_unique_targets_and_keep_statement_order() {
    let resolution = FkResolutionVm {
        unique: false,
        candidates: vec![vec![CandidateValueVm::Number(1.to_string())]],
    };
    assert_eq!(
        fk_candidate_values(&resolution),
        Err(FkCandidateErrorVm { message_key: "fkSelector.notUnique" }),
        "a non-unique target would make the pick ambiguous"
    );
    assert_eq!(NOT_UNIQUE_ERROR.message_key, "fkSelector.notUnique");
    let unique = FkResolutionVm {
        unique: true,
        candidates: vec![
            vec![CandidateValueVm::Number(1.to_string())],
            vec![CandidateValueVm::Number(1.to_string())],
            vec![CandidateValueVm::Null],
            vec![CandidateValueVm::Text("z".to_owned())],
        ],
    };
    assert_eq!(
        fk_candidate_values(&unique).unwrap(),
        vec!["1".to_owned(), "1".to_owned(), "z".to_owned()],
        "duplicates are kept, nulls are skipped and order is the statement's"
    );
    let plan = candidate_request_plan("customer_id", &loader_args());
    assert!(load_candidate_values(&plan, None, None).unwrap().is_empty());
}

#[test]
fn set_value_state_gates_the_date_picker_by_kind_zone_and_writability() {
    let ctx = context();
    let facts = SetValueRowFactsVm::default();
    let state = |name: &str| resolve_set_value_state(&ctx, name, &facts).unwrap();
    assert!(state("created_at").can_pick_date, "a plain date gets the picker");
    assert!(state("stamp").can_pick_date, "a plain datetime gets the picker");
    assert!(state("stamp").can_set_current_datetime, "the driver declares the expression");
    assert!(!state("seen_at").can_pick_date, "a zoned column keeps its text editor");
    assert!(!state("note").can_pick_date && !state("note").can_load_binary);

    let mut ro = context();
    ro.connection_read_only = true;
    let ro_state = resolve_set_value_state(&ro, "created_at", &facts).unwrap();
    assert!(!ro_state.can_pick_date, "a read-only panel hides the picker");
    let keyless = derive_editing_context(&facts_for_keyless(), None);
    let keyless_state = resolve_set_value_state(&keyless, "created_at", &facts).unwrap();
    assert!(!keyless_state.can_pick_date, "a keyless table hides the picker");
}

fn facts_for_keyless() -> EditingPanelFactsVm {
    let mut panel = facts();
    for entry in &mut panel.columns {
        entry.is_primary_key = false;
    }
    panel
}

#[test]
fn set_value_state_gates_the_binary_loader_and_candidate_entry() {
    let ctx = context();
    let facts = SetValueRowFactsVm { has_pending: true, candidates_loaded: false };
    let binary = resolve_set_value_state(&ctx, "avatar", &facts).unwrap();
    assert!(binary.can_load_binary, "binaryColumn is the reason the loader exists");
    assert!(!binary.can_load_candidates);
    let auto = resolve_set_value_state(&ctx, "token", &facts).unwrap();
    assert!(!auto.can_load_binary, "an auto-increment column is refused outright");
    let candidates = resolve_set_value_state(&ctx, "kind", &facts).unwrap();
    assert!(candidates.can_load_candidates, "enum candidates not loaded yet");
    let loaded = resolve_set_value_state(
        &ctx,
        "kind",
        &SetValueRowFactsVm { has_pending: false, candidates_loaded: true },
    )
    .unwrap();
    assert!(!loaded.can_load_candidates && !loaded.has_pending);
    let plain = resolve_set_value_state(&ctx, "id", &facts).unwrap();
    assert!(plain.can_null == false && plain.can_empty, "an empty string is a value");
    assert!(resolve_set_value_state(&ctx, "absent", &facts).is_none());
}

#[test]
fn set_value_routing_never_stages_a_dialog_entry() {
    use datazen_ui::views::table_editing::{SetValueDialogVm, SetValueRouteVm};
    assert_eq!(
        route_set_value(SetValueActionId::PickDate, true),
        SetValueRouteVm::OpenDialog(SetValueDialogVm::PickDate)
    );
    assert_eq!(
        route_set_value(SetValueActionId::LoadBinaryFromFile, true),
        SetValueRouteVm::OpenDialog(SetValueDialogVm::LoadBinaryFromFile)
    );
    assert_eq!(route_set_value(SetValueActionId::PickDate, false), SetValueRouteVm::NoOp);
    assert_eq!(route_set_value(SetValueActionId::LoadBinaryFromFile, false), SetValueRouteVm::NoOp);
    assert_eq!(SetValueDialogVm::PickDate.as_str(), "pickDate");
    assert_eq!(SetValueDialogVm::LoadBinaryFromFile.as_str(), "loadBinaryFromFile");

    assert_eq!(
        route_set_value(SetValueActionId::SetNull, false),
        SetValueRouteVm::Stage(StagedIntentVm::Null)
    );
    assert_eq!(
        route_set_value(SetValueActionId::SetEmpty, false),
        SetValueRouteVm::Stage(StagedIntentVm::Empty)
    );
    assert_eq!(
        route_set_value(SetValueActionId::SetDefault, false),
        SetValueRouteVm::Stage(StagedIntentVm::Absent)
    );
    assert_eq!(
        route_set_value(SetValueActionId::RevertCell, false),
        SetValueRouteVm::Stage(StagedIntentVm::Absent),
        "setDefault and revertCell are the same write"
    );
    assert_eq!(
        route_set_value(SetValueActionId::SetCurrentDatetime, false),
        SetValueRouteVm::Stage(StagedIntentVm::CurrentDatetimeExpression)
    );

    let ctx = context();
    let state = resolve_set_value_state(&ctx, "created_at", &SetValueRowFactsVm::default()).unwrap();
    assert!(with_dialog_host_gate(state, true).can_pick_date);
    let no_host = with_dialog_host_gate(state, false);
    assert!(!no_host.can_pick_date && !no_host.can_load_binary);
}

#[test]
fn set_value_actions_keep_the_exact_order_and_keys() {
    let state = SetValueStateVm {
        can_null: true,
        can_empty: true,
        row_is_insert: false,
        has_pending: true,
        can_pick_date: true,
        can_load_binary: true,
        can_set_current_datetime: true,
        ..Default::default()
    };
    let actions = resolve_set_value_actions(&state);
    assert_eq!(
        actions.iter().map(|action| action.id).collect::<Vec<_>>(),
        vec![
            SetValueActionId::SetNull,
            SetValueActionId::SetEmpty,
            SetValueActionId::RevertCell,
            SetValueActionId::PickDate,
            SetValueActionId::LoadBinaryFromFile,
            SetValueActionId::SetCurrentDatetime,
        ]
    );
    assert_eq!(
        actions.iter().map(|action| action.label_key).collect::<Vec<_>>(),
        vec![
            "dataTable.setNull",
            "cellEditor.setEmpty",
            "cellEditor.revertCell",
            "dataTable.pickDate",
            "dataTable.loadBinaryFromFile",
            "dataTable.setCurrentDatetime",
        ]
    );
    let insert = SetValueStateVm { row_is_insert: true, has_pending: false, ..state };
    let ids = resolve_set_value_actions(&insert).into_iter().map(|a| a.id).collect::<Vec<_>>();
    assert!(ids.contains(&SetValueActionId::SetDefault));
    assert!(!ids.contains(&SetValueActionId::RevertCell));
    for action in [
        SetValueActionId::SetNull,
        SetValueActionId::SetEmpty,
        SetValueActionId::SetDefault,
        SetValueActionId::RevertCell,
        SetValueActionId::PickDate,
        SetValueActionId::LoadBinaryFromFile,
        SetValueActionId::SetCurrentDatetime,
    ] {
        assert!(!action.as_str().is_empty());
    }
    let plain = resolve_set_value_actions(&SetValueStateVm::default());
    assert!(plain.is_empty(), "a state with no flags offers nothing");
}
