//! Port of `packages/frontend/src/windows/schema-diff/
//! SchemaDiffSelectionSteps.tsx` (T250) — the wizard's two selection
//! steps, one component.
//!
//! `step === 'endpoints'` mounts the migration endpoints bar (T010's
//! port, configured for the schema-diff wizard: grid layout, the
//! `schema-diff` testid prefix, the `schemaDiff` i18n prefix, no swap
//! and no compare button, the cross-dialect note as the footer);
//! `step === 'objects'` mounts the objects step (T149's port: the
//! table picks with select-all/none) over the unified objects picker
//! (T290's — a prebuilt child seam here, exactly the `settings`
//! subtree's pattern for not-yet-landed siblings).
//!
//! ## What maps where
//!
//! - The `endpoints` prop (`useSchemaDiffEndpoints`'s return, T384)
//!   becomes [`SchemaDiffEndpointsVm`]: the two connection ids, the
//!   database/schema choices and their option lists ride the
//!   T010 bar's `EndpointVm`s; the setters route the bar's six
//!   `on_*_change` callbacks.
//! - `setTablePicks`'s two map arms are the component's own callbacks:
//!   [`Self::on_select_all`]/[`Self::on_select_none`] toggle every
//!   row's `enabled` flag (the host applies them to its pick list).
//! - `unifiedObjects` (`useSchemaDiffUnifiedObjects`'s return, T420)
//!   is [`UnifiedObjectsVm`]: the picker seam's inputs (source/target
//!   objects, the selected-key sets, the errors, the cross-dialect
//!   flag, the catalog load state) — the picker component lands with
//!   T290 and consumes them; until then the host passes a prebuilt
//!   picker element ([`Self::unified_picker`]).
//! - `data-catalog-load-state` has no engine attribute: the load state
//!   rides the container's key (`schema-diff-objects-step` while
//!   settled, `-loading` / `-invalidated` suffixes while not), the
//!   same key-encodes-testid convention every port uses.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim: the endpoints step is the
//! T010 bar's `schema-diff` prefix; the objects step is
//! `schema-diff-objects-step`.

use std::rc::Rc;

use rune::{col, El};
use rune_kit::Kit;

use crate::components::migration::migration_endpoints_bar::{
    EndpointsBarVm, EndpointVm, MigrationEndpointsBar, MigrationI18nPrefix,
};
use crate::ui::kit::kit;
use crate::views::schema_diff::schema_diff_objects_step::SchemaDiffObjectsStep;
use crate::views::schema_diff::schema_diff_table_names::SchemaDiffTablePickVm;

/// The wizard step (`'endpoints' | 'objects'`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectionStep {
    Endpoints,
    Objects,
}

/// One connection option the endpoints step lists (the bar's currency:
/// value + label + an optional disabled flag).
#[derive(Clone)]
pub(crate) struct SelectionConnectionOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
}

impl SelectionConnectionOption {
    fn to_endpoint_option(&self) -> EndpointOption {
        EndpointOption { value: self.value.clone(), label: self.label.clone(), disabled: self.disabled }
    }
}

use crate::components::migration::migration_endpoints_bar::EndpointOption;

/// The `endpoints` prop: `useSchemaDiffEndpoints`'s return (T384's
/// hook), projected for the bar.
#[derive(Clone)]
pub(crate) struct SchemaDiffEndpointsVm {
    pub source_id: String,
    pub target_id: String,
    /// The source's chosen database / schema (None = the placeholder).
    pub source_database: Option<String>,
    pub target_database: Option<String>,
    pub source_schema: Option<String>,
    pub target_schema: Option<String>,
    /// The connection list (both selects share it in the schema-diff
    /// wizard; React passes it as `connOptions`).
    pub conn_options: Vec<SelectionConnectionOption>,
    /// The target select's own list (`targetOptions`) — when the
    /// wizard distinguishes them.
    pub target_options: Option<Vec<SelectionConnectionOption>>,
    pub source_databases: Vec<String>,
    pub target_databases: Vec<String>,
    pub source_schemas: Vec<String>,
    pub target_schemas: Vec<String>,
    /// Whether the two endpoints speak different dialects (the picker's
    /// cross-dialect mode).
    pub is_cross_dialect: bool,
}

impl SchemaDiffEndpointsVm {
    fn conn_options(&self) -> Vec<EndpointOption> {
        self.conn_options.iter().map(SelectionConnectionOption::to_endpoint_option).collect()
    }

    fn target_options(&self) -> Vec<EndpointOption> {
        self.target_options
            .as_ref()
            .map(|options| options.iter().map(SelectionConnectionOption::to_endpoint_option).collect())
            .unwrap_or_else(|| self.conn_options())
    }
}

/// The `unifiedObjects` prop: `useSchemaDiffUnifiedObjects`'s return
/// (T420's hook), projected for the picker (T290's component).
pub(crate) struct UnifiedObjectsVm<'a> {
    pub loading: bool,
    pub source_objects: &'a [String],
    pub target_objects: &'a [String],
    pub selected_source_keys: &'a [String],
    pub selected_target_keys: &'a [String],
    pub errors: &'a [String],
    /// `catalogLoadState` (`'never-started' | 'loading' | 'completed' |
    /// 'invalidated'`).
    pub catalog_load_state: CatalogLoadState,
}

/// `useSchemaDiffUnifiedObjects`'s catalog load state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CatalogLoadState {
    NeverStarted,
    Loading,
    Completed,
    Invalidated,
}

impl CatalogLoadState {
    /// The container key's suffix (the `data-catalog-load-state`
    /// encoding).
    fn key_suffix(self) -> &'static str {
        match self {
            Self::NeverStarted => "",
            Self::Loading => "-loading",
            Self::Completed => "",
            Self::Invalidated => "-invalidated",
        }
    }
}

/// The two selection steps (React `SchemaDiffSelectionSteps`).
pub(crate) struct SchemaDiffSelectionSteps<'a, S: 'static> {
    step: SelectionStep,
    endpoints: &'a SchemaDiffEndpointsVm,
    /// The cross-dialect note (the endpoints step's footer).
    endpoints_cross_dialect_note: Option<String>,
    loading: bool,
    table_picks: &'a [SchemaDiffTablePickVm],
    unified: &'a UnifiedObjectsVm<'a>,
    /// The unified picker (T290's seam): a prebuilt element, or a
    /// builder the host runs with the picker's own slots.
    unified_picker: Option<El>,
    // the endpoints step's six setters (the bar's on_*_change routes)
    on_source_change: Option<Rc<dyn Fn(&mut S, String)>>,
    on_target_change: Option<Rc<dyn Fn(&mut S, String)>>,
    on_source_database_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    on_target_database_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    on_source_schema_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    on_target_schema_change: Option<Rc<dyn Fn(&mut S, Option<String>)>>,
    // the objects step's callbacks
    on_toggle_table: Option<Rc<dyn Fn(&mut S, String)>>,
    on_select_all: Option<Rc<dyn Fn(&mut S)>>,
    on_select_none: Option<Rc<dyn Fn(&mut S)>>,
    on_toggle_unified_source: Option<Rc<dyn Fn(&mut S, String)>>,
    on_toggle_unified_target: Option<Rc<dyn Fn(&mut S, String)>>,
    on_select_all_unified: Option<Rc<dyn Fn(&mut S)>>,
    on_clear_object_selections: Option<Rc<dyn Fn(&mut S)>>,
    on_retry_objects: Option<Rc<dyn Fn(&mut S)>>,
}

impl<'a, S: 'static> SchemaDiffSelectionSteps<'a, S> {
    pub(crate) fn new(
        step: SelectionStep,
        endpoints: &'a SchemaDiffEndpointsVm,
        loading: bool,
        table_picks: &'a [SchemaDiffTablePickVm],
        unified: &'a UnifiedObjectsVm<'a>,
    ) -> Self {
        Self {
            step,
            endpoints,
            endpoints_cross_dialect_note: None,
            loading,
            table_picks,
            unified,
            unified_picker: None,
            on_source_change: None,
            on_target_change: None,
            on_source_database_change: None,
            on_target_database_change: None,
            on_source_schema_change: None,
            on_target_schema_change: None,
            on_toggle_table: None,
            on_select_all: None,
            on_select_none: None,
            on_toggle_unified_source: None,
            on_toggle_unified_target: None,
            on_select_all_unified: None,
            on_clear_object_selections: None,
            on_retry_objects: None,
        }
    }

    /// The endpoints step's footer note (`endpointsCrossDialectNote`).
    pub(crate) fn endpoints_cross_dialect_note(
        mut self,
        note: Option<String>,
    ) -> Self {
        self.endpoints_cross_dialect_note = note;
        self
    }

    /// The unified picker (T290's seam): the prebuilt element the
    /// objects step mounts below the table list.
    pub(crate) fn unified_picker(mut self, picker: Option<El>) -> Self {
        self.unified_picker = picker;
        self
    }

    /// `endpoints.setSourceId`.
    pub(crate) fn on_source_change(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_source_change = Some(Rc::new(f));
        self
    }

    /// `endpoints.setTargetId`.
    pub(crate) fn on_target_change(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_target_change = Some(Rc::new(f));
        self
    }

    /// `endpoints.setSourceDatabase`.
    pub(crate) fn on_source_database_change(
        mut self,
        f: impl Fn(&mut S, Option<String>) + 'static,
    ) -> Self {
        self.on_source_database_change = Some(Rc::new(f));
        self
    }

    /// `endpoints.setTargetDatabase`.
    pub(crate) fn on_target_database_change(
        mut self,
        f: impl Fn(&mut S, Option<String>) + 'static,
    ) -> Self {
        self.on_target_database_change = Some(Rc::new(f));
        self
    }

    /// `endpoints.setSourceSchema`.
    pub(crate) fn on_source_schema_change(
        mut self,
        f: impl Fn(&mut S, Option<String>) + 'static,
    ) -> Self {
        self.on_source_schema_change = Some(Rc::new(f));
        self
    }

    /// `endpoints.setTargetSchema`.
    pub(crate) fn on_target_schema_change(
        mut self,
        f: impl Fn(&mut S, Option<String>) + 'static,
    ) -> Self {
        self.on_target_schema_change = Some(Rc::new(f));
        self
    }

    /// `onToggleTable(name)`.
    pub(crate) fn on_toggle_table(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_toggle_table = Some(Rc::new(f));
        self
    }

    /// `onSelectAll`: every pick's `enabled` flips true (the host maps
    /// its own list; the callback is the decision).
    pub(crate) fn on_select_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_all = Some(Rc::new(f));
        self
    }

    /// `onSelectNone`: every pick's `enabled` flips false.
    pub(crate) fn on_select_none(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_none = Some(Rc::new(f));
        self
    }

    /// `onToggleUnifiedObject('source')(object)`.
    pub(crate) fn on_toggle_unified_source(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_toggle_unified_source = Some(Rc::new(f));
        self
    }

    /// `onToggleUnifiedObject('target')(object)`.
    pub(crate) fn on_toggle_unified_target(
        mut self,
        f: impl Fn(&mut S, String) + 'static,
    ) -> Self {
        self.on_toggle_unified_target = Some(Rc::new(f));
        self
    }

    /// `unifiedObjects.selectAll`.
    pub(crate) fn on_select_all_unified(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_select_all_unified = Some(Rc::new(f));
        self
    }

    /// `onClearObjectSelections`.
    pub(crate) fn on_clear_object_selections(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_clear_object_selections = Some(Rc::new(f));
        self
    }

    /// `onRetryObjects`.
    pub(crate) fn on_retry_objects(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_retry_objects = Some(Rc::new(f));
        self
    }

    /// Build the active step.
    pub(crate) fn build(self, dark: bool) -> El {
        match self.step {
            SelectionStep::Endpoints => self.build_endpoints_step(dark),
            SelectionStep::Objects => self.build_objects_step(dark),
        }
    }

    fn build_endpoints_step(&self, dark: bool) -> El {
        let k = kit(dark);
        // The wizard's bar configuration: grid layout, the schema-diff
        // testid prefix, the schemaDiff i18n prefix, no swap, no
        // compare, the cross-dialect note as the footer.
        let vm = EndpointsBarVm {
            test_id_prefix: "schema-diff".to_owned(),
            i18n_prefix: Some(MigrationI18nPrefix::SchemaDiff),
            layout: crate::components::migration::migration_endpoints_bar::EndpointsLayout::Grid,
            source: EndpointVm {
                connection_id: self.endpoints.source_id.clone(),
                database: self.endpoints.source_database.clone().unwrap_or_default(),
                schema: self.endpoints.source_schema.clone().unwrap_or_default(),
                databases: self.endpoints.source_databases.clone(),
                schemas: self.endpoints.source_schemas.clone(),
                options: self.endpoints.conn_options(),
                ..EndpointVm::default()
            },
            target: EndpointVm {
                connection_id: self.endpoints.target_id.clone(),
                database: self.endpoints.target_database.clone().unwrap_or_default(),
                schema: self.endpoints.target_schema.clone().unwrap_or_default(),
                databases: self.endpoints.target_databases.clone(),
                schemas: self.endpoints.target_schemas.clone(),
                options: self.endpoints.target_options(),
                ..EndpointVm::default()
            },
            busy: self.loading,
            show_swap: Some(false),
            show_compare: Some(false),
            footer_note: self.endpoints_cross_dialect_note.clone(),
            ..EndpointsBarVm::default()
        };

        let mut bar = MigrationEndpointsBar::<S>::new(vm);
        if let Some(on_change) = &self.on_source_change {
            let on_change = Rc::clone(on_change);
            bar = bar.on_source_change(move |s: &mut S, value: String| on_change(s, value));
        }
        if let Some(on_change) = &self.on_target_change {
            let on_change = Rc::clone(on_change);
            bar = bar.on_target_change(move |s: &mut S, value: String| on_change(s, value));
        }
        if let Some(on_change) = &self.on_source_database_change {
            let on_change = Rc::clone(on_change);
            bar = bar.on_source_database_change(move |s: &mut S, database| on_change(s, Some(database)));
        }
        if let Some(on_change) = &self.on_target_database_change {
            let on_change = Rc::clone(on_change);
            bar = bar.on_target_database_change(move |s: &mut S, database| on_change(s, Some(database)));
        }
        if let Some(on_change) = &self.on_source_schema_change {
            let on_change = Rc::clone(on_change);
            bar = bar.on_source_schema_change(move |s: &mut S, schema| on_change(s, Some(schema)));
        }
        if let Some(on_change) = &self.on_target_schema_change {
            let on_change = Rc::clone(on_change);
            bar = bar.on_target_schema_change(move |s: &mut S, schema| on_change(s, Some(schema)));
        }
        bar.build(dark)
    }

    fn build_objects_step(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut step = SchemaDiffObjectsStep::<S>::new(self.loading, self.table_picks);
        if let Some(on_toggle) = &self.on_toggle_table {
            let on_toggle = Rc::clone(on_toggle);
            step = step.on_toggle(move |s: &mut S, name: String| on_toggle(s, name));
        }
        if let Some(on_select_all) = &self.on_select_all {
            let on_select_all = Rc::clone(on_select_all);
            step = step.on_select_all(move |s: &mut S| on_select_all(s));
        }
        if let Some(on_select_none) = &self.on_select_none {
            let on_select_none = Rc::clone(on_select_none);
            step = step.on_select_none(move |s: &mut S| on_select_none(s));
        }

        let mut column = col()
            .key(format!(
                "schema-diff-objects-step{}",
                self.unified.catalog_load_state.key_suffix()
            ))
            .w_full()
            .grow(1.0)
            .gap(12.0);
        column = column.child(step.build(dark));
        if let Some(picker) = &self.unified_picker {
            column = column.child(picker.clone());
        }
        let _ = &k;
        column
    }
}
