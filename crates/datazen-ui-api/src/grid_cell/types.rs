use serde_json::Value;
use std::collections::BTreeMap;

pub const CELL_TEST_ID: &str = "data-table-cell";
pub const ROLE_GRIDCELL: &str = "gridcell";
pub const EDITOR_KIND_ENUM: &str = "enum";
pub const CLASS_BASE: &str = "flex shrink-0 items-center overflow-hidden border-r border-edge/30 px-2";
pub const CLASS_DIRTY_BORDER: &str = "relative border-l-2 border-l-dt-dirty-border";
pub const CLASS_DIRTY_BG: &str = "bg-dt-dirty-bg";
pub const CLASS_STICKY: &str = "relative bg-surface";
pub const CLASS_STICKY_OVERLAY: &str = "pointer-events-none absolute inset-0";
pub const CLASS_FK_JUMP: &str = "ml-1 flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-muted hover:bg-surface-raised hover:text-accent disabled:opacity-60";
pub const CLASS_FK_PICK: &str = "ml-0.5 flex h-4 w-4 shrink-0 items-center justify-center rounded text-fg-muted hover:bg-surface-raised hover:text-accent";
pub const CLASS_DIRTY_MARK: &str = "pointer-events-none absolute right-0 top-0 h-1 w-1 rotate-45 bg-dt-dirty-mark";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FkTextKind {
    Jump,
    Resolving,
    Selector,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkCellAffordance {
    pub target_label: String,
    pub navigable: bool,
    pub resolving: bool,
    pub pickable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StickyInfo {
    pub left: i64,
    pub bg_class: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColumnCellFacts {
    pub nullable: bool,
    pub integer_only: bool,
    pub read_only_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorCellFacts {
    pub panel_id: Option<String>,
    pub has_value_handler: bool,
    pub has_load_candidates: bool,
    pub read_only_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridCellInput {
    pub row_index: usize,
    pub column_name: String,
    pub column_type: String,
    pub value: Option<Value>,
    pub width: u32,
    pub sticky: Option<StickyInfo>,
    pub selected: bool,
    pub error: bool,
    pub active: bool,
    pub editing: bool,
    pub cell_type: Option<String>,
    pub column_info: Option<ColumnCellFacts>,
    pub editor: Option<EditorCellFacts>,
    pub id: Option<String>,
    pub grid_semantics: bool,
    pub dirty: bool,
    pub dirty_tooltip: Option<String>,
    pub dirty_aria_label: Option<String>,
    pub fk: Option<FkCellAffordance>,
    pub has_fk_activate: bool,
    pub has_quick_look: bool,
    pub presentation_plan: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CellWriteTarget {
    Editor,
    Legacy,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellRendererView {
    pub column_name: String,
    pub data_type: String,
    pub value: Option<Value>,
    pub is_editing: bool,
    pub cell_type: Option<String>,
    pub nullable: bool,
    pub integer_only: bool,
    pub panel_id: Option<String>,
    pub has_load_candidates: bool,
    pub editor_kind_override: Option<&'static str>,
    pub presentation_plan: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StickyStyle {
    pub left: i64,
    pub z_index: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkButtonView {
    pub test_marker: &'static str,
    pub title: Option<String>,
    pub disabled: bool,
    pub classes: Vec<String>,
    pub resolving: bool,
    pub pick: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CellChild {
    StickyOverlay { classes: Vec<String> },
    Renderer(CellRendererView),
    FkJump(FkButtonView),
    FkPick(FkButtonView),
    DirtyMark { classes: Vec<String> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct GridCellView {
    pub id: Option<String>,
    pub role: Option<&'static str>,
    pub attributes: BTreeMap<String, String>,
    pub aria_selected: Option<&'static str>,
    pub aria_label: Option<String>,
    pub title: Option<String>,
    pub classes: Vec<String>,
    pub width: u32,
    pub sticky_style: Option<StickyStyle>,
    pub children: Vec<CellChild>,
    pub write_target: CellWriteTarget,
}
