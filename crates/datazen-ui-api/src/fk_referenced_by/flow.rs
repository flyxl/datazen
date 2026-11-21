use super::catalog::{inbound_catalog, InboundCatalogCache};
use super::filters::{build_inbound_filters, referenced_values_of_row};
use super::types::{
    CatalogScope, CreatedPanel, FkOrigin, GridErrorClassifier, InboundCatalogFetcher, NavEntry,
    NavOrigin, OpenReferencingRowsInput, OutcomeError, OutcomeErrorCode, ReferencedByOutcome,
    ReferencingRowsHost, TableChangeContext, MAX_REFERENCING_TABS,
};
use crate::{fk_nav_key, FkNavTargetIdentity};

pub fn open_referencing_rows(
    input: &OpenReferencingRowsInput,
    host: &mut dyn ReferencingRowsHost,
    cache: &mut InboundCatalogCache,
    fetcher: &mut InboundCatalogFetcher,
    classifier: &GridErrorClassifier,
) -> ReferencedByOutcome {
    let scope = CatalogScope {
        db_session_id: input.db_session_id.clone(),
        database: input.database.clone(),
        schema: input.schema.clone(),
        table: input.source_table.clone(),
    };
    let catalog = match inbound_catalog(&scope, cache, fetcher) {
        Ok(catalog) => catalog,
        Err(raw) => {
            let code = classifier(&raw)
                .map(OutcomeErrorCode::Classified)
                .unwrap_or(OutcomeErrorCode::Unknown);
            return ReferencedByOutcome::refusal(OutcomeError { code, raw });
        }
    };

    if catalog.references.is_empty() {
        return ReferencedByOutcome::refusal(OutcomeError {
            code: OutcomeErrorCode::NoReferences,
            raw: format!(
                "grid.fk.noReferences: no table references '{}'",
                input.source_table
            ),
        });
    }

    let viable: Vec<_> = catalog
        .references
        .iter()
        .filter_map(|reference| {
            referenced_values_of_row(reference, &input.row).map(|values| (reference, values))
        })
        .collect();
    if viable.is_empty() {
        return ReferencedByOutcome::refusal(OutcomeError {
            code: OutcomeErrorCode::NoReferences,
            raw: "grid.fk.noReferences: the key of this row is NULL, so no row can reference it"
                .to_owned(),
        });
    }

    if let Some(viewport) = &input.source_viewport {
        host.record_viewport(&input.source_panel_id, viewport);
    }

    let source = host.source_panel(&input.source_panel_id);
    let connection_id = source
        .as_ref()
        .and_then(|panel| panel.connection_id.clone())
        .or_else(|| input.connection_id.clone())
        .unwrap_or_default();
    let db_session_id = source
        .as_ref()
        .and_then(|panel| panel.db_session_id.clone())
        .unwrap_or_else(|| input.db_session_id.clone());
    let connection_name = source
        .as_ref()
        .and_then(|panel| panel.connection_name.clone())
        .unwrap_or_default();
    let database_type = source
        .as_ref()
        .and_then(|panel| panel.database_type.clone())
        .or_else(|| input.driver_type.clone())
        .unwrap_or_default();

    let viable_count = viable.len();
    let capped: Vec<_> = viable.into_iter().take(MAX_REFERENCING_TABS).collect();
    let skipped = viable_count.saturating_sub(capped.len());
    let mut opened = Vec::with_capacity(capped.len());
    for (reference, values) in capped {
        let nav_values: Vec<_> = values.iter().cloned().map(Some).collect();
        let nav_key = fk_nav_key(
            &FkNavTargetIdentity {
                database: input.database.clone(),
                schema: input.schema.clone(),
                table: reference.table.clone(),
                columns: reference.columns.clone(),
            },
            &nav_values,
        );
        let panel_id = host.next_panel_id();
        let panel = CreatedPanel {
            id: panel_id.clone(),
            table_name: reference.table.clone(),
            database: input.database.clone(),
            table_schema: input.schema.clone(),
            sub_tab: "data".to_owned(),
            connection_id: connection_id.clone(),
            db_session_id: db_session_id.clone(),
            connection_name: connection_name.clone(),
            database_type: database_type.clone(),
        };
        host.add_panel(&panel);
        let filters = build_inbound_filters(reference, &values);
        let context = TableChangeContext {
            connection_id: connection_id.clone(),
            db_session_id: db_session_id.clone(),
            driver_type: database_type.clone(),
            database: input.database.clone(),
            schema: input.schema.clone(),
            table: reference.table.clone(),
        };
        host.seed_panel(&panel_id, &context, &filters);
        let origin = NavOrigin {
            source_panel_id: input.source_panel_id.clone(),
            source_table: input.source_table.clone(),
            local_columns: reference.columns.clone(),
            target_table: reference.table.clone(),
            target_columns: reference.referenced_columns.clone(),
        };
        let nav_entry_id = host.push_nav(&NavEntry {
            panel_id: panel_id.clone(),
            nav_key,
            origin: origin.clone(),
        });
        host.update_panel(
            &panel_id,
            &FkOrigin {
                source_panel_id: origin.source_panel_id,
                source_table: origin.source_table,
                local_columns: origin.local_columns,
                target_table: origin.target_table,
                target_columns: origin.target_columns,
                nav_entry_id,
            },
        );
        opened.push(panel_id);
    }

    ReferencedByOutcome {
        ok: true,
        panel_ids: opened,
        error: None,
        skipped,
    }
}
