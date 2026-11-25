//! Independent audit for T606: pure `tableEditing` decisions.
//!
//! Pin the candidate loader branches, dialog-host routing and byte-size /
//! relation-kind boundaries the coder does not drive.

use std::collections::BTreeMap;

use datazen_ui::support::cell_types::CellType;
use datazen_ui::views::table_editing::{
    candidate_request_plan, derive_editing_context, load_candidate_values, request_edit_plan,
    resolve_cell_editable, route_set_value, with_dialog_host_gate, CandidateLoaderArgsVm,
    CandidateRequestPlanVm, CandidateValueVm, CellEditabilityInputVm, EditingColumnVm,
    EditingContextVm, EditingCoordVm, EditingPanelFactsVm, EditingRowKind, EnumQueryRequestVm,
    FkCandidateSourceVm, FkResolutionVm, FkBrowseRequestVm, QueryResultVm, QueryStatementVm,
    RelationKind, SetValueActionId, SetValueDialogVm, SetValueRouteVm, StagedIntentVm,
};

fn input() -> CellEditabilityInputVm {
    CellEditabilityInputVm {
        mount_reason: None,
        connection_read_only: false,
        capability_editable: None,
        relation_kind: RelationKind::Table,
        has_primary_key: true,
        row_identity_unique: true,
        column_is_auto_increment: false,
        cell_type: CellType::Text,
        in_memory_byte_size: 0,
        type_read_only: false,
        row_kind: EditingRowKind::Update,
    }
}

fn context() -> EditingContextVm {
    derive_editing_context(
        &EditingPanelFactsVm {
            panel_id: "p".to_owned(),
            columns: vec![EditingColumnVm {
                name: "note".to_owned(),
                data_type: Some("text".to_owned()),
                nullable: true,
                is_primary_key: true,
                ..Default::default()
            }],
            ..Default::default()
        },
        None,
    )
}

#[test]
fn candidate_display_and_optional_fk_constraint() {
    assert_eq!(CandidateValueVm::Other("x".to_owned()).display().as_deref(), Some("x"));
    assert_eq!(CandidateValueVm::Null.display(), None);
    let mut args = CandidateLoaderArgsVm {
        distinct_sql: Some("SELECT 1".to_owned()),
        ..Default::default()
    };
    args.fk_sources.insert(
        "cid".to_owned(),
        FkCandidateSourceVm { local_column: "c".to_owned(), constraint_name: None },
    );
    let plan = candidate_request_plan("cid", &args);
    match &plan {
        CandidateRequestPlanVm::FkBrowse(FkBrowseRequestVm { constraint_name, .. }) => {
            assert_eq!(*constraint_name, None, "audit: a source with no constraint name keeps None");
        }
        _ => panic!("audit: FK source must take the browse branch"),
    }
}

#[test]
fn load_candidate_values_covers_each_branch() {
    let enum_plan = CandidateRequestPlanVm::EnumQuery(EnumQueryRequestVm {
        db_session_id: "s".to_owned(),
        sql: "SELECT 1".to_owned(),
        database: None,
        schema: None,
    });
    assert!(load_candidate_values(&enum_plan, None, None).unwrap().is_empty(), "audit: no result means no enum values");
    let result = QueryResultVm {
        statements: vec![QueryStatementVm {
            rows: vec![vec![CandidateValueVm::Text("a".to_owned())]],
        }],
    };
    assert_eq!(load_candidate_values(&enum_plan, Some(&result), None).unwrap(), vec!["a"]);

    let fk_plan = CandidateRequestPlanVm::FkBrowse(FkBrowseRequestVm {
        db_session_id: "s".to_owned(),
        database: None,
        schema: None,
        table: "t".to_owned(),
        columns: vec!["c".to_owned()],
        constraint_name: None,
        values: Vec::new(),
        limit: 50,
        browse: true,
    });
    assert!(load_candidate_values(&fk_plan, None, None).unwrap().is_empty(), "audit: no resolution offers nothing");
    let bad = FkResolutionVm { unique: false, candidates: Vec::new() };
    assert!(load_candidate_values(&fk_plan, None, Some(&bad)).is_err(), "audit: a non-unique FK resolution is refused");
    let good = FkResolutionVm {
        unique: true,
        candidates: vec![vec![CandidateValueVm::Text("z".to_owned())]],
    };
    assert_eq!(load_candidate_values(&fk_plan, None, Some(&good)).unwrap(), vec!["z"]);
}

#[test]
fn dialog_host_routes_value_actions_to_staging() {
    for (action, intent) in [
        (SetValueActionId::SetNull, StagedIntentVm::Null),
        (SetValueActionId::SetEmpty, StagedIntentVm::Empty),
        (SetValueActionId::SetCurrentDatetime, StagedIntentVm::CurrentDatetimeExpression),
    ] {
        assert_eq!(
            route_set_value(action, true),
            SetValueRouteVm::Stage(intent),
            "audit: a dialog host does not change value-intent actions"
        );
    }
    assert_eq!(
        route_set_value(SetValueActionId::PickDate, true),
        SetValueRouteVm::OpenDialog(SetValueDialogVm::PickDate)
    );
}

#[test]
fn dialog_gate_hides_binary_loader_without_a_host() {
    let mut s = datazen_ui::views::table_editing::SetValueStateVm {
        can_pick_date: true,
        can_load_binary: true,
        ..Default::default()
    };
    s = with_dialog_host_gate(s, false);
    assert!(!s.can_pick_date && !s.can_load_binary, "audit: no host hides both dialog entries");
    assert!(s.can_null == false && s.has_pending == false, "audit: only dialog flags change");
}

#[test]
fn byte_size_boundary_and_view_relation() {
    let mut i = input();
    i.in_memory_byte_size = 64 * 1024;
    assert!(resolve_cell_editable(&i).editable, "audit: exactly 64 KiB is still editable");
    i.in_memory_byte_size = 64 * 1024 + 1;
    assert!(!resolve_cell_editable(&i).editable, "audit: one byte over 64 KiB is refused");

    let mut ctx = context();
    ctx.relation_kind = RelationKind::View;
    let coord = EditingCoordVm { row_index: 0, column_name: "note".to_owned() };
    let (refused, effect) = request_edit_plan(Some(&ctx), &coord);
    assert!(!refused.editable && effect.is_none(), "audit: a view refuses the edit entry");
}
