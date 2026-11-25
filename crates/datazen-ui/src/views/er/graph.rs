//! The ER canvas's shared graph model — the Rust stand-in for the
//! `Node`/`Edge` DTOs React Flow hands between `buildErGraph`,
//! `interactionState` and `layoutErGraph`.
//!
//! React gets these types from `@xyflow/react`; Rune has no canvas library,
//! so the three ported modules trade this one vocabulary instead. The fields
//! are exactly the ones the React sources read or write: position and size
//! for layout, the paint contract (stroke/dash/opacity/marker/label chrome)
//! for the diagram's look, and the join metadata (kind, columns, handles)
//! for what a line *means*. Everything the ports do not observe was left out
//! deliberately.
//!
//! Not wired into `Section` dispatch: the ER canvas host mounts the diagram;
//! until then every item is `#[allow(dead_code)]` at its use site.

use crate::views::er::table_node::TableNodeVm;

/// React `Node['position']` / React Flow's top-left corner convention.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ErPosition {
    pub x: f32,
    pub y: f32,
}

impl ErPosition {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// How an edge between two tables was established.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErRelationKind {
    /// A foreign key constraint the database declares.
    Declared,
    #[default]
    /// A relationship the engine inferred from structure and naming.
    Predicted,
}

impl ErRelationKind {
    /// The `data.kind` string the React sources tag edges with.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Declared => "declared",
            Self::Predicted => "predicted",
        }
    }
}

/// The paint contract of a line. `None` keeps whatever the field means when
/// absent in React (`strokeDasharray: undefined`, no hover styling yet).
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErEdgeStyle {
    pub stroke: Option<String>,
    pub stroke_dasharray: Option<String>,
    pub opacity: Option<f32>,
    pub stroke_width: Option<f32>,
}

/// An arrowhead, painted by a marker the line's own opacity does not reach.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub struct ErMarkerEnd {
    /// Always `MarkerType.ArrowClosed` in the React sources.
    pub arrow_closed: bool,
    pub color: String,
}

impl Default for ErMarkerEnd {
    fn default() -> Self {
        Self {
            arrow_closed: true,
            color: String::new(),
        }
    }
}

/// The label's own ink.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub struct ErLabelStyle {
    pub font_size: f32,
    pub fill: String,
}

impl Default for ErLabelStyle {
    fn default() -> Self {
        Self {
            font_size: 10.0,
            fill: String::new(),
        }
    }
}

/// The chip a label sits on, so it knocks out the line behind it.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq)]
pub struct ErLabelBg {
    pub fill: String,
    pub padding: (f32, f32),
    pub radius: f32,
}

impl Default for ErLabelBg {
    fn default() -> Self {
        // React's shared label chrome: surface-token chip, 4×2 padding,
        // 3pt corners.
        Self {
            fill: String::new(),
            padding: (4.0, 2.0),
            radius: 3.0,
        }
    }
}

/// What a line means: which kind of relationship it carries and the join it
/// anchors on.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErEdgeData {
    pub kind: ErRelationKind,
    /// Only an inference carries a score.
    pub score: Option<f64>,
    pub ambiguous: bool,
    pub source_column: Option<String>,
    pub target_column: Option<String>,
    /// Every column the relationship spans (label text; composite keys list
    /// them all).
    pub columns: Vec<String>,
}

/// React `Edge`, reduced to the fields the ports observe.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErEdge {
    pub id: String,
    pub source: String,
    pub target: String,
    /// Always `'smoothstep'` in the React sources.
    pub edge_type: &'static str,
    pub animated: bool,
    pub style: ErEdgeStyle,
    pub marker: ErMarkerEnd,
    pub label: Option<String>,
    pub label_style: ErLabelStyle,
    pub label_bg: ErLabelBg,
    /// Draw order; hover emphasis raises a line above the dimmed ones.
    pub z_index: Option<i32>,
    pub source_handle: Option<String>,
    pub target_handle: Option<String>,
    pub data: ErEdgeData,
}

/// React `Node`, reduced to the fields the ports observe. `data` is the
/// `TableNodeData` the rendered table consumes (T361's `TableNodeVm`).
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErNode {
    pub id: String,
    /// Always `'tableNode'` in the React sources.
    pub node_type: &'static str,
    pub position: ErPosition,
    pub width: f32,
    pub height: f32,
    pub data: TableNodeVm,
}

/// What `buildErGraph` hands back.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ErGraph {
    pub nodes: Vec<ErNode>,
    pub edges: Vec<ErEdge>,
}
