//! `GlobalObjectSearch`, ported 1:1 from
//! `packages/frontend/src/windows/connection/navigator/GlobalObjectSearch.tsx`.
//!
//! React searches the *already loaded* schema slices with the pure
//! `lib/schemaObjectSearch.ts` helper, then shows a dialog with a live query
//! field and result rows; table/view hits also offer five SQL-opening actions.
//! The search is ported verbatim in Rust below (no command and no SQL is run).
//! Opening a result or an action is emitted as an [`ObjectSearchAction`] for an
//! injected host port — no fake IPC, promise or thread.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this dialog yet. The navigator host will own the
//! query `TextEdit` and translate the two intents into `TablePanel` opens;
//! until then every reusable item carries a per-item `#[allow(dead_code)]`
//! with this removal stage, and the attributes can be dropped together.
//!
//! ## Keys
//!
//! `global-object-search` (React `testId`), `-input`, `-clear`, `-results`,
//! `global-object-search-result` (React testid on every row) and
//! `object-search-action-<kind>`. All copy uses exact locale keys; the five
//! action tooltips are the English literals from React's `tableSqlActions.ts`.

use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_SEARCH, ICON_X};
use crate::ui::dialog::Dialog;
use crate::ui::field::TextField;
use crate::ui::kit::kit;

const ROOT: &str = "global-object-search";
/// React `results.slice(0, 100)`.
const MAX_RESULTS: usize = 100;

/// React `ObjectSearchObjectType`.
#[allow(dead_code)] // VM mirror; consumed by the search, view and tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectType {
    Table,
    View,
    Column,
    Function,
    Routine,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl ObjectType {
    pub fn key(self) -> &'static str {
        match self {
            Self::Table => "objectSearch.type.table",
            Self::View => "objectSearch.type.view",
            Self::Column => "objectSearch.type.column",
            Self::Function => "objectSearch.type.function",
            Self::Routine => "objectSearch.type.routine",
            Self::Procedure => "objectSearch.type.procedure",
            Self::Trigger => "objectSearch.type.trigger",
            Self::Sequence => "objectSearch.type.sequence",
            Self::Type => "objectSearch.type.type",
        }
    }

    fn order(self) -> usize {
        match self {
            Self::Table => 0,
            Self::View => 1,
            Self::Column => 2,
            Self::Function => 3,
            Self::Routine => 4,
            Self::Procedure => 5,
            Self::Trigger => 6,
            Self::Sequence => 7,
            Self::Type => 8,
        }
    }

    fn is_relation(self) -> bool {
        matches!(self, Self::Table | Self::View)
    }

    /// React `objectObjectType` / `relationObjectType`.
    fn from_kind(kind: &str) -> Self {
        match kind.to_ascii_lowercase().as_str() {
            "view" | "materializedview" => Self::View,
            "column" => Self::Column,
            "function" => Self::Function,
            "procedure" => Self::Routine,
            "trigger" => Self::Trigger,
            "sequence" => Self::Sequence,
            "type" => Self::Type,
            _ => Self::Table,
        }
    }
}

/// React `TableSqlActionKind` (the five table/view actions).
#[allow(dead_code)] // VM mirror; consumed by the result rows and the tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableActionKind {
    OpenData,
    Select,
    Insert,
    Update,
    Ddl,
}

impl TableActionKind {
    pub const ALL: [Self; 5] = [Self::OpenData, Self::Select, Self::Insert, Self::Update, Self::Ddl];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OpenData => "openData",
            Self::Select => "select",
            Self::Insert => "insert",
            Self::Update => "update",
            Self::Ddl => "ddl",
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Self::OpenData => "objectSearch.openData",
            Self::Select => "objectSearch.select",
            Self::Insert => "objectSearch.insert",
            Self::Update => "objectSearch.update",
            Self::Ddl => "objectSearch.ddl",
        }
    }

    /// React `ACTION_COPY[kind].description` — a literal tooltip, not a locale key.
    pub fn description(self) -> &'static str {
        match self {
            Self::OpenData => "Open the existing Table Panel data view.",
            Self::Select => "Open a read-only SELECT draft for the table.",
            Self::Insert => "Open an INSERT draft with driver-specific columns and values to be supplied.",
            Self::Update => "Open an UPDATE draft with a driver-specific assignment and predicate.",
            Self::Ddl => "Open the driver-generated DDL entry point for the table.",
        }
    }
}

#[allow(dead_code)] // index mirror; supplied by the host schema cache.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RelationInfo {
    pub name: String,
    pub schema: Option<String>,
    pub table_type: String,
}

#[allow(dead_code)] // index mirror; supplied by the host schema cache.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ObjectInfo {
    pub name: String,
    pub schema: Option<String>,
    pub kind: String,
}

/// One loaded `SchemaObjectIndexEntry` slice. `columns` is the schemaStore
/// table-name → column-names map (React accepts `columns` / `columnMap`).
#[allow(dead_code)] // index mirror; supplied by the host schema cache.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchemaIndexEntry {
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub connection_name: Option<String>,
    pub host: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub relations: Vec<RelationInfo>,
    pub objects: Vec<ObjectInfo>,
    pub columns: Vec<(String, Vec<String>)>,
}

/// React `ObjectSearchResult` (without the SQL payload — the host builds it).
#[allow(dead_code)] // VM mirror; consumed by result rows, actions and tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObjectSearchResult {
    pub id: String,
    pub connection_id: String,
    pub db_session_id: String,
    pub database_type: String,
    pub connection_name: Option<String>,
    pub host: Option<String>,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub object_type: ObjectType,
    pub name: String,
    pub table_name: Option<String>,
    pub match_reason: Option<String>,
    pub actions: Vec<TableActionKind>,
}

fn normalized(value: Option<&str>) -> Option<String> {
    value.and_then(|text| {
        let trimmed = text.trim();
        if trimmed.is_empty() { None } else { Some(trimmed.to_owned()) }
    })
}

fn lower(value: Option<&str>) -> String {
    value.unwrap_or("").to_ascii_lowercase()
}

fn result_id(result: &ObjectSearchResult) -> String {
    // A stable identity, mirroring React's JSON.stringify tuple.
    format!(
        "{}|{}|{}|{:?}|{}|{}",
        result.connection_id,
        result.database.as_deref().unwrap_or(""),
        result.schema.as_deref().unwrap_or(""),
        result.object_type,
        result.table_name.as_deref().unwrap_or(""),
        result.name
    )
}

/// React `matchingFieldsForQuery`, in its stable display/search order.
fn matching_fields(result: &ObjectSearchResult, query: &str) -> Vec<String> {
    if query.is_empty() {
        return Vec::new();
    }
    let q = query.to_ascii_lowercase();
    let fields: [(&str, Option<&str>); 8] = [
        ("name", result.connection_name.as_deref()),
        ("host", result.host.as_deref()),
        ("database", result.database.as_deref()),
        ("type", Some(result.database_type.as_str())),
        ("schema", result.schema.as_deref()),
        ("object", if result.object_type == ObjectType::Column { None } else { Some(result.name.as_str()) }),
        ("table", result.table_name.as_deref()),
        ("column", if result.object_type == ObjectType::Column { Some(result.name.as_str()) } else { None }),
    ];
    fields
        .into_iter()
        .filter(|(_, value)| lower(*value).contains(&q))
        .map(|(field, _)| field.to_owned())
        .collect()
}

fn compare_results(a: &ObjectSearchResult, b: &ObjectSearchResult) -> std::cmp::Ordering {
    for (left, right) in [
        (lower(a.connection_name.as_deref()), lower(b.connection_name.as_deref())),
        (lower(Some(&a.connection_id)), lower(Some(&b.connection_id))),
        (lower(a.database.as_deref()), lower(b.database.as_deref())),
        (lower(a.schema.as_deref()), lower(b.schema.as_deref())),
    ] {
        if left != right {
            return left.cmp(&right);
        }
    }
    if a.object_type.order() != b.object_type.order() {
        return a.object_type.order().cmp(&b.object_type.order());
    }
    let a_table = lower(a.table_name.as_deref().or(Some(&a.name)));
    let b_table = lower(b.table_name.as_deref().or(Some(&b.name)));
    if a_table != b_table {
        return a_table.cmp(&b_table);
    }
    lower(Some(&a.name)).cmp(&lower(Some(&b.name)))
}

/// React `searchSchemaObjects`: relations, their columns and standalone
/// objects from loaded slices only. No command, credential or SQL execution.
#[allow(dead_code)] // pure helper; consumed by the view and the tests.
pub fn search_schema_objects(
    index: &[SchemaIndexEntry],
    query: &str,
) -> Vec<ObjectSearchResult> {
    let query = query.trim();
    let mut results = Vec::new();
    for entry in index {
        let database = normalized(entry.database.as_deref());
        let entry_schema = normalized(entry.schema.as_deref());
        let mut relation_keys: Vec<String> = Vec::new();
        for relation in &entry.relations {
            let table_name = normalized(Some(&relation.name));
            let Some(table_name) = table_name else { continue };
            let schema = normalized(Some(relation.schema.as_deref().unwrap_or("")))
                .or_else(|| entry_schema.clone());
            let object_type = ObjectType::from_kind(&relation.table_type);
            let key = format!(
                "{}|{}|{:?}|{}",
                database.as_deref().unwrap_or(""),
                schema.as_deref().unwrap_or(""),
                object_type,
                table_name
            );
            if relation_keys.contains(&key) {
                continue;
            }
            relation_keys.push(key);

            let mut relation_result = ObjectSearchResult {
                id: String::new(),
                connection_id: entry.connection_id.clone(),
                db_session_id: entry.db_session_id.clone(),
                database_type: entry.database_type.clone(),
                connection_name: entry.connection_name.clone(),
                host: entry.host.clone(),
                database: database.clone(),
                schema: schema.clone(),
                object_type,
                name: table_name.clone(),
                table_name: Some(table_name.clone()),
                match_reason: None,
                actions: TableActionKind::ALL.to_vec(),
            };
            relation_result.match_reason = matching_fields(&relation_result, query).into_iter().next();
            if query.is_empty() || relation_result.match_reason.is_some() {
                relation_result.id = result_id(&relation_result);
                results.push(relation_result);
            }

            // React `columnNamesFor`: try db.schema.table, schema.table, table.
            let keys = [
                database.as_deref().zip(schema.as_deref()).map(|(db, sch)| format!("{db}.{sch}.{table_name}")),
                schema.as_deref().map(|sch| format!("{sch}.{table_name}")),
                Some(table_name.clone()),
            ];
            let mut column_names: Vec<String> = Vec::new();
            'outer: for (map_key, names) in &entry.columns {
                for candidate in keys.iter().flatten() {
                    if map_key == candidate {
                        for name in names {
                            let trimmed = name.trim();
                            if !trimmed.is_empty() && !column_names.iter().any(|n| n == trimmed) {
                                column_names.push(trimmed.to_owned());
                            }
                        }
                        break 'outer;
                    }
                }
            }
            for column_name in column_names {
                let mut column_result = ObjectSearchResult {
                    id: String::new(),
                    connection_id: entry.connection_id.clone(),
                    db_session_id: entry.db_session_id.clone(),
                    database_type: entry.database_type.clone(),
                    connection_name: entry.connection_name.clone(),
                    host: entry.host.clone(),
                    database: database.clone(),
                    schema: schema.clone(),
                    object_type: ObjectType::Column,
                    name: column_name,
                    table_name: Some(table_name.clone()),
                    match_reason: None,
                    actions: TableActionKind::ALL.to_vec(),
                };
                column_result.match_reason = matching_fields(&column_result, query).into_iter().next();
                if query.is_empty() || column_result.match_reason.is_some() {
                    column_result.id = result_id(&column_result);
                    results.push(column_result);
                }
            }
        }

        for object in &entry.objects {
            let Some(object_name) = normalized(Some(&object.name)) else { continue };
            let schema = normalized(Some(object.schema.as_deref().unwrap_or("")))
                .or_else(|| entry_schema.clone());
            let mut object_result = ObjectSearchResult {
                id: String::new(),
                connection_id: entry.connection_id.clone(),
                db_session_id: entry.db_session_id.clone(),
                database_type: entry.database_type.clone(),
                connection_name: entry.connection_name.clone(),
                host: entry.host.clone(),
                database: database.clone(),
                schema,
                object_type: ObjectType::from_kind(&object.kind),
                name: object_name,
                table_name: None,
                match_reason: None,
                actions: vec![TableActionKind::Ddl],
            };
            object_result.match_reason = matching_fields(&object_result, query).into_iter().next();
            if query.is_empty() || object_result.match_reason.is_some() {
                object_result.id = result_id(&object_result);
                results.push(object_result);
            }
        }
    }
    results.sort_by(compare_results);
    results
}

/// Host-owned projection of the dialog props.
#[allow(dead_code)] // VM surface; owned by the navigator host.
#[derive(Clone, Debug, Default)]
pub struct GlobalObjectSearchVm {
    pub open: bool,
    pub query: TextEdit,
    pub index: Vec<SchemaIndexEntry>,
}

impl GlobalObjectSearchVm {
    /// React `searchSchemaObjects(index, query)` memo.
    #[allow(dead_code)] // pure projection; consumed by the view and tests.
    pub fn results(&self) -> Vec<ObjectSearchResult> {
        search_schema_objects(&self.index, self.query.text.trim())
    }
}

/// The two intents the view asks the host to perform.
#[allow(dead_code)] // host port; remove when the navigator host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ObjectSearchAction {
    Close,
    ClearQuery,
    OpenResult(ObjectSearchResult),
    OpenTableAction(ObjectSearchResult, TableActionKind),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, ObjectSearchAction) + 'static>;
type EditFn<S> = Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: ObjectSearchAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// Render the dialog into `layer`. A closed React `Dialog` mounts nothing.
#[allow(dead_code)] // consumer: the connection navigator host (not landed).
pub fn build<S: 'static>(
    vm: &GlobalObjectSearchVm,
    query_edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
) {
    build_dialog(vm, query_edit, on_action, dark, layer, None);
}

/// Native only has the current session's catalog and the relation-open action.
pub(crate) fn build_native<S: 'static>(
    vm: &GlobalObjectSearchVm,
    query_edit: EditFn<S>,
    on_action: ActionFn<S>,
    dark: bool,
    layer: &mut Vec<El>,
    epoch: u64,
) {
    build_dialog(vm, Some(query_edit), Some(on_action), dark, layer, Some(epoch));
}

fn build_dialog<S: 'static>(
    vm: &GlobalObjectSearchVm,
    query_edit: Option<EditFn<S>>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
    layer: &mut Vec<El>,
    native_epoch: Option<u64>,
) {
    if !vm.open {
        return;
    }
    let native = native_epoch.is_some();
    let k = kit(dark);
    let results = vm.results();
    let mut field = TextField::new(format!("{ROOT}-input"), &vm.query)
        .height(36.0)
        .grow()
        .pad_x(8.0)
        .text_style(TextStyle::new(14.0, 400.0))
        .placeholder(t(if native { "objectSearch.native.placeholder" } else { "objectSearch.placeholder" }));
    if native {
        field = field.label(t("objectSearch.native.placeholder"));
    }
    if let Some(edit) = query_edit {
        let accessor = edit.clone();
        field = field.on_edit(move |s: &mut S| accessor(s));
    } else {
        field = field.disabled(true);
    }
    let input_row = row()
        .key(format!("{ROOT}-input-row"))
        .w_full()
        .gap(6.0)
        .center_y()
        .child(icon(&ICON_SEARCH, 16.0).color(k.c.text_muted))
        .child(field.build(dark));
    let mut input_row = input_row;
    if !vm.query.text.is_empty() {
        input_row = input_row.child(
            icon(&ICON_X, 16.0)
                .key(format!("{ROOT}-clear"))
                .color(k.c.text_muted)
                .role("button")
                .focusable(true)
                .label(t(if native { "filter.clear" } else { "common.close" }))
                .on_press(emit(&on_action, ObjectSearchAction::ClearQuery)),
        );
    }

    let mut list = col()
        .key(format!("{ROOT}-results"))
        .w_full()
        .min_w(0.0)
        .max_h(320.0)
        .border(1.0, k.c.border_muted)
        .radius(6.0)
        .scroll(rune::ScrollSpec { stick_bottom: false });
    if results.is_empty() {
        let empty = if vm.index.is_empty() {
            t(if native { "objectSearch.native.empty" } else { "objectSearch.noLoadedSchema" })
        } else {
            t("objectSearch.noResults")
        };
        list = list.child(
            text(empty)
                .key(format!("{ROOT}-empty"))
                .color(k.c.text_muted)
                .wrap()
                .min_w(0.0),
        );
    } else {
        for result in results.into_iter().take(MAX_RESULTS) {
            list = list.child(result_row(&result, &on_action, dark, native));
        }
    }

    let body = col()
        .key(format!("{ROOT}-body"))
        .w_full()
        .min_w(0.0)
        .gap(12.0)
        .child(input_row)
        .child(list);
    Dialog::new(ROOT, t(if native { "objectSearch.native.title" } else { "objectSearch.title" }))
        .open(true)
        .open_epoch(native_epoch.unwrap_or_default())
        .description(t(if native { "objectSearch.native.description" } else { "objectSearch.description" }))
        .fit()
        .width(720.0)
        .body_max_height(420.0)
        .child(body)
        .on_close::<S>(emit(&on_action, ObjectSearchAction::Close))
        .build(k, layer);
}

fn result_row<S: 'static>(
    result: &ObjectSearchResult,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
    native: bool,
) -> El {
    let k = kit(dark);
    // Native only supplies unique names; encode the raw bytes in a key-safe
    // identity so quoted, non-ASCII and slash identifiers cannot collide.
    let identity = native.then(|| result.name.as_bytes().iter()
        .map(|byte| format!("{byte:02x}")).collect::<Vec<_>>().join(""));
    let context = [
        result.connection_name.as_deref(),
        result.database.as_deref(),
        result.schema.as_deref(),
        result.table_name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.is_empty())
    .collect::<Vec<_>>()
    .join(" / ");
    let matched = t("objectSearch.matched")
        .replace("{field}", result.match_reason.as_deref().unwrap_or("name"));
    let open = text(result.name.clone())
        .key(identity.as_ref().map_or_else(
            || format!("{ROOT}-result-name"),
            |id| format!("native-object-search-open-{id}"),
        ))
        .color(k.c.text_base)
        .wrap()
        .min_w(0.0)
        .grow(1.0)
        .role("button")
        .focusable(true)
        .on_press(emit(on_action, ObjectSearchAction::OpenResult(result.clone())));
    let name_row = row()
        .w_full()
        .gap(8.0)
        .center_y()
        .child(open)
        .child(
            text(t(result.object_type.key()))
                .key(format!("{ROOT}-result-type"))
                .color(k.c.text_weak),
        );
    let mut item = col()
        .key(identity.as_ref().map_or_else(
            || "global-object-search-result".to_owned(),
            |id| format!("{ROOT}-result-{id}"),
        ))
        .w_full()
        .min_w(0.0)
        .gap(2.0)
        .child(name_row)
        .child(
            text(format!("{context} · {matched}"))
                .key(format!("{ROOT}-result-context"))
                .color(k.c.text_muted)
                .wrap()
                .min_w(0.0),
        );

    if !native && result.object_type.is_relation() {
        let mut actions = row().w_full().gap(4.0).flex_wrap().center_y();
        for action in &result.actions {
            actions = actions.child(
                Button::new(t(action.key()))
                    .variant(if *action == TableActionKind::OpenData {
                        ButtonVariant::Neutral
                    } else {
                        ButtonVariant::Ghost
                    })
                    .control_size(ControlSize::Small)
                    .help(action.description())
                    .on_press(emit(
                        on_action,
                        ObjectSearchAction::OpenTableAction(result.clone(), *action),
                    ))
                    .build(k)
                    .key(format!("object-search-action-{}", action.as_str())),
            );
        }
        item = item.child(actions);
    }
    item
}
