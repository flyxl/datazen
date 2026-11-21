use crate::{
    FkFilterTarget, FkNavTargetIdentity, ForeignKeyTargetRef, ForeignKeyTargetResolution,
    NavValue, ResolveForeignKeyTargetsRequest, Value, build_fk_filters, fk_nav_key,
};
use crate::fk_navigation_actions::types::{
    FkActionDeps, FkActionEffects, FkNavOrigin, FkNavState, FkJumpError, FkJumpOutcome,
    FkOrigin, OpenForeignKeyTargetInput, Panel, PanelKind, PanelState, TableChangeContext,
    TableIdentity, classify_grid_error, host_error_message,
};

fn nav_values(values: &[Value]) -> Vec<NavValue> {
    values.iter().cloned().map(Some).collect()
}

fn find_reusable_panel(
    target: &TableIdentity,
    nav_key: &str,
    source_panel_id: &str,
    panels: &PanelState,
    nav: &FkNavState,
) -> Option<(String, String)> {
    if let Some(duplicate_id) = nav.find_entry_id_by_nav_key(nav_key) {
        if let Some(entry) = nav.back.iter().find(|entry| entry.id == duplicate_id) {
            if panels.panels.iter().any(|panel| panel.id == entry.panel_id) {
                return Some((entry.panel_id.clone(), entry.id.clone()));
            }
        }
    }
    for entry in nav.back.iter().rev() {
        let Some(origin) = panels.panels.iter().find(|panel| panel.id == entry.origin.source_panel_id) else {
            continue;
        };
        if origin.id == source_panel_id {
            continue;
        }
        let Some(identity) = origin.identity() else {
            continue;
        };
        if identity.same(target) {
            return Some((origin.id.clone(), entry.id.clone()));
        }
    }
    None
}

fn record_source_viewport(input: &OpenForeignKeyTargetInput, effects: &mut FkActionEffects) {
    if let Some(viewport) = input.source_viewport.as_ref() {
        effects.recorded_viewports.push((input.source_panel_id.clone(), viewport.clone()));
        effects.timeline.push("viewport".to_owned());
    }
}

fn resolve_request(input: &OpenForeignKeyTargetInput) -> ResolveForeignKeyTargetsRequest {
    ResolveForeignKeyTargetsRequest {
        db_session_id: input.db_session_id.clone(),
        database: Some(input.database.clone()),
        schema: input.schema.clone(),
        table: input.source_table.clone(),
        qualified_table: None,
        columns: input.foreign_key.columns.clone(),
        constraint_name: Some(input.foreign_key.name.clone()),
        values: input.values.clone(),
        limit: None,
    }
}

fn refusal(code: &str, raw: String) -> FkJumpOutcome {
    FkJumpOutcome::refused(FkJumpError { code: code.to_owned(), raw })
}

pub fn open_foreign_key_target(
    input: &OpenForeignKeyTargetInput,
    nav: &mut FkNavState,
    panels: &mut PanelState,
    effects: &mut FkActionEffects,
    deps: &FkActionDeps,
) -> FkJumpOutcome {
    let columns = &input.foreign_key.columns;
    if columns.is_empty() || columns.len() != input.values.len() {
        return refusal(
            "grid.fk.shapeMismatch",
            format!(
                "grid.fk.shapeMismatch: the foreign key has {} local column(s) but {} value(s)",
                columns.len(),
                input.values.len()
            ),
        );
    }
    if input.values.iter().any(|value| matches!(value, Value::Null)) {
        return refusal(
            "grid.fk.valueNull",
            "grid.fk.valueNull: a member of the foreign key has no value, so the target row cannot be addressed".to_owned(),
        );
    }

    let request = resolve_request(input);
    let resolution: ForeignKeyTargetResolution = match (deps.resolve_target)(&request) {
        Ok(resolution) => resolution,
        Err(error) => {
            let raw = host_error_message(&error);
            let code = classify_grid_error(&raw);
            return refusal(&code, raw);
        }
    };

    record_source_viewport(input, effects);

    let target: &ForeignKeyTargetRef = &resolution.target;
    let nav_target = FkNavTargetIdentity {
        database: target.database.clone(),
        schema: target.schema.clone(),
        table: target.table.clone(),
        columns: target.columns.clone(),
    };
    let values = nav_values(&input.values);
    let nav_key = fk_nav_key(&nav_target, &values);
    let identity = TableIdentity::of_target(target);

    if let Some((panel_id, entry_id)) = find_reusable_panel(
        &identity,
        &nav_key,
        &input.source_panel_id,
        panels,
        nav,
    ) {
        nav.rewind_to(&entry_id);
        effects.timeline.push("rewind".to_owned());
        panels.set_active_panel(&panel_id);
        effects.timeline.push("activate".to_owned());
        return FkJumpOutcome::landed(panel_id, true, !resolution.exists);
    }

    let filter_target = FkFilterTarget { columns: target.columns.clone() };
    let Some(conditions) = build_fk_filters(&filter_target, &values) else {
        return refusal(
            "grid.fk.shapeMismatch",
            format!(
                "grid.fk.shapeMismatch: the resolved target has {} referenced column(s) but {} value(s)",
                target.columns.len(),
                input.values.len()
            ),
        );
    };

    let source_panel = panels.panels.iter().find(|panel| panel.id == input.source_panel_id);
    let panel_id = (deps.next_panel_id)();
    let panel = Panel {
        id: panel_id.clone(),
        kind: PanelKind::Table,
        table_name: target.table.clone(),
        database: target.database.clone(),
        table_schema: target.schema.clone(),
        connection_id: source_panel
            .map(|panel| panel.connection_id.clone())
            .or_else(|| input.connection_id.clone())
            .unwrap_or_default(),
        db_session_id: source_panel
            .map(|panel| panel.db_session_id.clone())
            .unwrap_or_else(|| input.db_session_id.clone()),
        connection_name: source_panel
            .map(|panel| panel.connection_name.clone())
            .unwrap_or_default(),
        database_type: source_panel
            .and_then(|panel| panel.database_type.clone())
            .or_else(|| input.driver_type.clone()),
    };
    panels.add_panel(panel.clone());
    effects.timeline.push("add_panel".to_owned());

    let context = TableChangeContext {
        connection_id: panel.connection_id.clone(),
        db_session_id: panel.db_session_id.clone(),
        driver_type: panel.database_type.clone(),
        database: target.database.clone(),
        schema: target.schema.clone(),
        table: target.table.clone(),
    };
    effects.seeded_panels.push((panel_id.clone(), context, conditions));
    effects.timeline.push("seed".to_owned());

    let origin = FkNavOrigin {
        source_panel_id: input.source_panel_id.clone(),
        source_table: input.source_table.clone(),
        local_columns: columns.clone(),
        target_table: target.table.clone(),
        target_columns: target.columns.clone(),
    };
    let entry = nav.push(&panel_id, &nav_key, origin.clone());
    effects.timeline.push("push".to_owned());

    panels.update_panel(
        &panel_id,
        FkOrigin {
            source_panel_id: origin.source_panel_id,
            source_table: origin.source_table,
            local_columns: origin.local_columns,
            target_table: origin.target_table,
            target_columns: origin.target_columns,
            nav_entry_id: entry.id,
        },
    );
    effects.timeline.push("update_panel".to_owned());

    FkJumpOutcome::landed(panel_id, false, !resolution.exists)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fk_navigation_actions::types::{FkJumpOutcome, PanelKind};
    use crate::{FilterOperator, ForeignKeyInfo, GridViewport};

    fn fk() -> ForeignKeyInfo {
        ForeignKeyInfo {
            name: "fk_orders_customer".to_owned(),
            columns: vec!["customer_id".to_owned()],
            referenced_table: "customers".to_owned(),
            referenced_columns: vec!["id".to_owned()],
            on_update: None,
            on_delete: None,
            deferrability: None,
        }
    }

    fn input() -> OpenForeignKeyTargetInput {
        OpenForeignKeyTargetInput {
            source_panel_id: "p1".to_owned(),
            source_table: "orders".to_owned(),
            db_session_id: "session-1".to_owned(),
            database: "shop".to_owned(),
            schema: None,
            connection_id: Some("conn-1".to_owned()),
            driver_type: Some("postgresql".to_owned()),
            foreign_key: fk(),
            values: vec![Value::from(7)],
            source_viewport: None,
        }
    }

    fn target() -> ForeignKeyTargetRef {
        ForeignKeyTargetRef {
            database: "shop".to_owned(),
            schema: Some("public".to_owned()),
            table: "customers".to_owned(),
            columns: vec!["id".to_owned()],
            data_types: vec!["INT".to_owned()],
            primary_keys: vec!["id".to_owned()],
            unique: true,
            candidate_limit: 10,
        }
    }

    fn resolution(exists: bool) -> ForeignKeyTargetResolution {
        ForeignKeyTargetResolution {
            target: target(),
            exists,
            candidates: Vec::new(),
            truncated: false,
        }
    }

    fn deps(result: Result<ForeignKeyTargetResolution, String>) -> FkActionDeps {
        FkActionDeps {
            resolve_target: Box::new(move |_| result.clone()),
            next_panel_id: Box::new(|| "tbl-2".to_owned()),
        }
    }

    fn source_panel() -> Panel {
        Panel {
            id: "p1".to_owned(),
            kind: PanelKind::Table,
            table_name: "orders".to_owned(),
            database: "shop".to_owned(),
            table_schema: None,
            connection_id: "conn-source".to_owned(),
            db_session_id: "session-source".to_owned(),
            connection_name: "Source".to_owned(),
            database_type: Some("postgresql".to_owned()),
        }
    }

    #[test]
    fn an_empty_or_mismatched_shape_is_refused_before_ipc() {
        let seen = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let seen_by_closure = seen.clone();
        let mut current = input();
        current.foreign_key.columns.clear();
        let deps = FkActionDeps {
            resolve_target: Box::new(move |_| {
                seen_by_closure.borrow_mut().push("called");
                Ok(resolution(true))
            }),
            next_panel_id: Box::new(|| "x".to_owned()),
        };
        let outcome = open_foreign_key_target(
            &current,
            &mut FkNavState::default(),
            &mut PanelState::default(),
            &mut FkActionEffects::default(),
            &deps,
        );
        assert!(seen.borrow().is_empty());
        assert_eq!(outcome.error.map(|error| error.code), Some("grid.fk.shapeMismatch".to_owned()));

        let mut current = input();
        current.foreign_key.columns.push("second".to_owned());
        let outcome = open_foreign_key_target(
            &current,
            &mut FkNavState::default(),
            &mut PanelState::default(),
            &mut FkActionEffects::default(),
            &deps,
        );
        assert!(outcome.error.as_ref().is_some_and(|error| error.raw.contains("2 local column(s) but 1 value(s)")));
    }

    #[test]
    fn a_null_value_is_refused_before_ipc() {
        let mut current = input();
        current.values = vec![Value::Null];
        let outcome = open_foreign_key_target(
            &current,
            &mut FkNavState::default(),
            &mut PanelState::default(),
            &mut FkActionEffects::default(),
            &deps(Ok(resolution(true))),
        );
        assert_eq!(outcome.error.as_ref().map(|error| error.code.as_str()), Some("grid.fk.valueNull"));
        assert!(outcome.error.as_ref().is_some_and(|error| error.raw.contains("has no value")));
    }

    #[test]
    fn a_failed_resolution_opens_nothing_and_classifies_the_error() {
        let outcome = open_foreign_key_target(
            &input(),
            &mut FkNavState::default(),
            &mut PanelState::default(),
            &mut FkActionEffects::default(),
            &deps(Err("  grid.fk.probeFailed: boom  ".to_owned())),
        );
        assert_eq!(outcome.error.as_ref().map(|error| error.code.as_str()), Some("grid.fk.probeFailed"));
        assert_eq!(outcome.error.map(|error| error.raw), Some("grid.fk.probeFailed: boom".to_owned()));
        assert!(outcome.panel_id.is_none());
    }

    #[test]
    fn an_unclassified_failure_is_unknown() {
        let outcome = open_foreign_key_target(
            &input(),
            &mut FkNavState::default(),
            &mut PanelState::default(),
            &mut FkActionEffects::default(),
            &deps(Err("plain boom".to_owned())),
        );
        assert_eq!(outcome.error.as_ref().map(|error| error.code.as_str()), Some("unknown"));
    }

    #[test]
    fn a_new_jump_creates_a_seeded_tab_and_records_the_navigation() {
        let mut nav = FkNavState::default();
        let mut panels = PanelState::default();
        panels.panels.push(source_panel());
        let mut effects = FkActionEffects::default();
        let outcome = open_foreign_key_target(
            &input(),
            &mut nav,
            &mut panels,
            &mut effects,
            &deps(Ok(resolution(true))),
        );
        assert_eq!(outcome, FkJumpOutcome::landed("tbl-2".to_owned(), false, false));
        assert_eq!(panels.added.len(), 1);
        assert_eq!(panels.added[0].table_name, "customers");
        assert_eq!(panels.added[0].connection_id, "conn-source");
        assert_eq!(panels.added[0].db_session_id, "session-source");
        assert_eq!(panels.added[0].connection_name, "Source");
        assert_eq!(effects.seeded_panels.len(), 1);
        let (panel_id, context, conditions) = &effects.seeded_panels[0];
        assert_eq!(panel_id, "tbl-2");
        assert_eq!(context.table, "customers");
        assert_eq!(context.schema.as_deref(), Some("public"));
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].column, "id");
        assert_eq!(conditions[0].operator, FilterOperator::Eq);
        assert_eq!(nav.back.len(), 1);
        assert_eq!(nav.back[0].origin.target_columns, vec!["id".to_owned()]);
        assert_eq!(panels.updated[0].1.nav_entry_id, "fk-1");
    }

    #[test]
    fn the_new_tab_uses_input_fallbacks_when_the_source_panel_is_unknown() {
        let mut panels = PanelState::default();
        let outcome = open_foreign_key_target(
            &input(),
            &mut FkNavState::default(),
            &mut panels,
            &mut FkActionEffects::default(),
            &deps(Ok(resolution(false))),
        );
        assert!(outcome.target_missing);
        assert_eq!(panels.added[0].connection_id, "conn-1");
        assert_eq!(panels.added[0].db_session_id, "session-1");
        assert_eq!(panels.added[0].connection_name, "");
        assert_eq!(panels.added[0].database_type.as_deref(), Some("postgresql"));
    }

    #[test]
    fn the_viewport_is_snapshot_before_the_target_tab_is_added() {
        let mut current = input();
        current.source_viewport = Some(GridViewport {
            scroll_top: 42.0,
            anchor_row: Some(3.0),
            anchor_column: Some("name".to_owned()),
        });
        let mut effects = FkActionEffects::default();
        open_foreign_key_target(
            &current,
            &mut FkNavState::default(),
            &mut PanelState::default(),
            &mut effects,
            &deps(Ok(resolution(true))),
        );
        assert_eq!(effects.timeline[0], "viewport");
        assert_eq!(effects.timeline, vec!["viewport", "add_panel", "seed", "push", "update_panel"]);
        assert_eq!(effects.recorded_viewports[0].0, "p1");
    }

    #[test]
    fn repeating_the_same_jump_reuses_the_existing_tab() {
        let mut nav = FkNavState::default();
        let mut panels = PanelState::default();
        let mut effects = FkActionEffects::default();
        open_foreign_key_target(
            &input(),
            &mut nav,
            &mut panels,
            &mut effects,
            &deps(Ok(resolution(true))),
        );
        let mut effects = FkActionEffects::default();
        let outcome = open_foreign_key_target(
            &input(),
            &mut nav,
            &mut panels,
            &mut effects,
            &deps(Ok(resolution(true))),
        );
        assert_eq!(outcome, FkJumpOutcome::landed("tbl-2".to_owned(), true, false));
        assert_eq!(nav.rewinds, vec!["fk-1"]);
        assert_eq!(panels.active_panel_id.as_deref(), Some("tbl-2"));
        assert_eq!(panels.added.len(), 1);
    }

    #[test]
    fn the_way_back_reuses_the_origin_tab_instead_of_opening_a_third() {
        let mut nav = FkNavState::default();
        let mut panels = PanelState::default();
        panels.panels.push(source_panel());
        let deps = deps(Ok(resolution(true)));
        open_foreign_key_target(
            &input(),
            &mut nav,
            &mut panels,
            &mut FkActionEffects::default(),
            &deps,
        );
        let mut back = input();
        back.source_panel_id = "tbl-2".to_owned();
        back.source_table = "customers".to_owned();
        back.foreign_key = ForeignKeyInfo {
            name: "fk_back".to_owned(),
            columns: vec!["id".to_owned()],
            referenced_table: "orders".to_owned(),
            referenced_columns: vec!["customer_id".to_owned()],
            ..fk()
        };
        back.values = vec![Value::from(99)];
        let back_target = ForeignKeyTargetRef {
            schema: None,
            table: "orders".to_owned(),
            columns: vec!["customer_id".to_owned()],
            ..target()
        };
        let back_deps = FkActionDeps {
            resolve_target: Box::new(move |_| Ok(ForeignKeyTargetResolution {
                target: back_target.clone(),
                exists: true,
                candidates: Vec::new(),
                truncated: false,
            })),
            next_panel_id: Box::new(|| "tbl-3".to_owned()),
        };
        let outcome = open_foreign_key_target(
            &back,
            &mut nav,
            &mut panels,
            &mut FkActionEffects::default(),
            &back_deps,
        );
        assert_eq!(outcome, FkJumpOutcome::landed("p1".to_owned(), true, false));
        assert_eq!(panels.active_panel_id.as_deref(), Some("p1"));
    }

    #[test]
    fn a_tab_opened_by_hand_is_never_reused() {
        let mut panels = PanelState::default();
        let hand_opened = Panel {
            id: "hand".to_owned(),
            table_name: "customers".to_owned(),
            database: "shop".to_owned(),
            table_schema: Some("public".to_owned()),
            ..source_panel()
        };
        panels.panels.push(hand_opened);
        let mut nav = FkNavState::default();
        let origin = FkNavOrigin {
            source_panel_id: "p1".to_owned(),
            source_table: "orders".to_owned(),
            local_columns: vec!["customer_id".to_owned()],
            target_table: "customers".to_owned(),
            target_columns: vec!["id".to_owned()],
        };
        nav.push("hand", "other-key", origin);
        let outcome = open_foreign_key_target(
            &input(),
            &mut nav,
            &mut panels,
            &mut FkActionEffects::default(),
            &deps(Ok(resolution(true))),
        );
        assert_eq!(outcome.panel_id.as_deref(), Some("tbl-2"));
        assert!(!outcome.reused);
    }
}
