//! The query context selectors, ported from
//! `packages/frontend/src/components/query/QueryContextSelectors.tsx`
//! (228 lines, T100 with `QueryErrorPanel`).
//!
//! The toolbar's database / catalog / schema selectors: a database glyph
//! and a row of compact selects (or fixed labels when a level has only one
//! option), for both the flat multi-database model and the path-hierarchy
//! model (Superset-style `558/hive/snap` namespaces).
//!
//! ## Input boundary
//!
//! React computes the path-hierarchy segments inside the component with
//! `pathHierarchySelectorSegmentsForUi(namespaceTree, pathAliases,
//! databases, contextPath)` from `lib/queryContextPath.ts` — a segment-2
//! task (T575, with `lib/sqlNamespace.ts` in T617). This port therefore
//! consumes the SEGMENTS as input ([`PathHierarchySegment`]): the segment
//! builder belongs to that task, and the wiring hands its output here.
//! The types live in this module until T575 re-homes them beside the
//! builder.
//!
//! ## Key convention
//!
//! React's `data-testid`s become the element keys:
//!
//! | part | key |
//! |---|---|
//! | shell root | `query-context-selectors` |
//! | database select | `query-context-database` |
//! | level select | `query-context-level-{index}` |
//! | fixed path label | `query-context-path-label-{index}` (React repeats one testid; rune keys must be unique) |
//! | flat schema label | `query-context-schema` |
//! | `/` separators | `query-context-separator-{index}` (new; React gives them none) |
//!
//! ## Deliberate boundaries
//!
//! * **The compact select is the `ui/Select` wrapper** (T001). React's
//!   `!h-6 !text-[11px] !w-auto !min-w-[4rem] !max-w-[7rem]` maps onto
//!   `.width(64)` (the `min-w` floor; a fixed width cannot be
//!   content-driven) and the wrapper's field appearance; the
//!   `listMinWidth 176`, `searchable`, `blurOnSelect` and `fitContent`
//!   props are Select base residuals the wrapper does not model.
//! * **`loading` is visual-only in React** (`loading={namespaceLoading &&
//!   options.length === 0}` rides on `disabled={options.length === 0}`):
//!   the port carries the disabled state; the loading glyph belongs to
//!   the Select base, so `namespaceLoading` is not part of this port's
//!   surface.
//! * **The focus advance.** React focuses the next level's input after a
//!   choose (`pendingFocusLevelRef` + `requestAnimationFrame`). The
//!   record exists ([`QueryContextSelectorsState::focus_level`]) so the
//!   wiring can act on it, but the kit Select has no programmatic focus
//!   to drive here.
//! * **Null renders.** `(!isMultiDb || databases empty) && !contextSchema`
//!   renders nothing (React `null`): [`QueryContextSelectors::build`]
//!   returns `Option<El>`.

// The production consumer is the query-workspace wiring task; until then
// the component tests are the only callers.
#![allow(dead_code)]

use std::rc::Rc;

use rune::{icon, row, text, El};

use crate::icons::ICON_CONNECTIONS;
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::ui::select::Select;

/// One path-hierarchy selector segment (`lib/queryContextPath.ts:216`):
/// a fixed label for single-option levels, a select for multi-option
/// ones. Owned by T575 (segment-2) once the builder lands; the shape here
/// is the contract this component renders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PathHierarchySegment {
    /// `{ kind: 'label', name }` — the level has exactly one option.
    Label { name: String },
    /// `{ kind: 'select', levelIndex, options, value }`.
    Select {
        level_index: usize,
        options: Vec<String>,
        value: String,
    },
}

/// The placeholder segments React shows while the namespace tree is
/// loading or has no roots (`PATH_HIERARCHY_PLACEHOLDER_SELECTOR_SEGMENTS`):
/// two empty selects at levels 0 and 1.
pub fn path_hierarchy_placeholder_segments() -> Vec<PathHierarchySegment> {
    vec![
        PathHierarchySegment::Select {
            level_index: 0,
            options: Vec::new(),
            value: String::new(),
        },
        PathHierarchySegment::Select {
            level_index: 1,
            options: Vec::new(),
            value: String::new(),
        },
    ]
}

/// React's `levelLabelKey` (`QueryContextSelectors.tsx:21`): which
/// placeholder key a level uses, by hierarchy mode and index.
pub fn level_label_key(index: usize, path_hierarchy: bool) -> &'static str {
    if path_hierarchy {
        if index == 0 {
            "query.catalog"
        } else {
            "query.schema"
        }
    } else if index == 0 {
        "query.database"
    } else if index == 1 {
        "query.catalog"
    } else {
        "query.schema"
    }
}

/// The host state the selectors own: which level's listbox is open, and
/// the pending focus level from the last choose. The app state embeds
/// this and hands the component a reference.
#[derive(Debug, Default)]
pub struct QueryContextSelectorsState {
    /// The level whose listbox is open (`None`: closed). React's Select
    /// keeps its own open state; the port's host state carries it.
    pub open_level: Option<usize>,
    /// React's `pendingFocusLevelRef`: the level to focus after the
    /// requestAnimationFrame — recorded on choose, cleared when acted on.
    pub focus_level: Option<usize>,
}

impl QueryContextSelectorsState {
    /// Close the open listbox (a choose or an outside press).
    pub fn close(&mut self) {
        self.open_level = None;
    }
}

/// The selectors. Path-hierarchy mode takes the caller-computed
/// segments (see the module docs); flat mode takes the databases, the
/// current database and the current schema, and optionally the host
/// state that carries the database listbox's open flag.
pub struct QueryContextSelectors<'a, S: 'static> {
    is_multi_db: bool,
    databases: &'a [String],
    current_database: Option<&'a str>,
    context_schema: Option<&'a str>,
    path_segments: Option<&'a [PathHierarchySegment]>,
    state: Option<&'a QueryContextSelectorsState>,
    host: Option<Rc<dyn Fn(&mut S) -> &mut QueryContextSelectorsState>>,
    on_select_level: Option<Rc<dyn Fn(&mut S, usize, &str)>>,
}

impl<'a, S: 'static> QueryContextSelectors<'a, S> {
    /// The flat form: `isMultiDb`, the databases, the current database
    /// and the current schema.
    pub fn new(
        is_multi_db: bool,
        databases: &'a [String],
        current_database: Option<&'a str>,
        context_schema: Option<&'a str>,
    ) -> Self {
        Self {
            is_multi_db,
            databases,
            current_database,
            context_schema,
            path_segments: None,
            state: None,
            host: None,
            on_select_level: None,
        }
    }

    /// The path-hierarchy form: the caller-computed segments (T575's
    /// builder output) plus the host state.
    pub fn path_hierarchy(
        segments: &'a [PathHierarchySegment],
        state: &'a QueryContextSelectorsState,
    ) -> Self {
        Self {
            is_multi_db: false,
            databases: &[],
            current_database: None,
            context_schema: None,
            path_segments: Some(segments),
            state: Some(state),
            host: None,
            on_select_level: None,
        }
    }

    /// The flat form's host state (the database listbox's open flag).
    pub fn flat_state(mut self, state: &'a QueryContextSelectorsState) -> Self {
        self.state = Some(state);
        self
    }

    /// The state accessor, so listbox toggles and closes run without the
    /// component owning the host state.
    pub fn host_mut(
        mut self,
        f: impl Fn(&mut S) -> &mut QueryContextSelectorsState + 'static,
    ) -> Self {
        self.host = Some(Rc::new(f));
        self
    }

    /// React's `onSelectLevel(index, value)`.
    pub fn on_select_level(
        mut self,
        f: impl Fn(&mut S, usize, &str) + 'static,
    ) -> Self {
        self.on_select_level = Some(Rc::new(f));
        self
    }

    /// The selectors, or `None` for React's `null` return. The shell is
    /// `flex shrink-0 items-center gap-0.5 min-w-0` with the database
    /// glyph (React's `Database`, the registry's database lucide glyph,
    /// 14pt muted).
    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> Option<El> {
        let k = kit(dark);
        let mut children: Vec<El> = Vec::new();
        if let Some(segments) = self.path_segments {
            children.extend(self.segment_children(&k, segments, layer));
            return Some(self.shell(dark, children));
        }
        // React: `(!isMultiDb || databases.length === 0) && !contextSchema`
        // returns null.
        if (!self.is_multi_db || self.databases.is_empty()) && self.context_schema.is_none() {
            return None;
        }
        if self.is_multi_db && !self.databases.is_empty() {
            // React's flat database select delivers `onSelectLevel(0, db)`
            // — level 0, no focus advance (that effect lives in the
            // path-hierarchy variant only).
            children.push(self.compact_select(
                &k,
                "query-context-database",
                Some(0),
                self.databases,
                self.current_database.unwrap_or(""),
                "query.database",
                layer,
            ));
        }
        if let Some(schema) = self.context_schema {
            // `/ {schema}`: `max-w-[140px] truncate text-[11px]
            // text-fg-muted`.
            children.push(
                text(format!("/ {schema}"))
                    .key("query-context-schema")
                    .max_w(140.0)
                    .truncate()
                    .help(schema)
                    .font(k.ty.small)
                    .color(k.c.text_muted),
            );
        }
        Some(self.shell(dark, children))
    }

    /// The shell row.
    fn shell(&self, dark: bool, children: Vec<El>) -> El {
        let k = kit(dark);
        row()
            .key("query-context-selectors")
            .shrink()
            .items_center()
            .gap(2.0)
            .min_w(0.0)
            .child(
                icon(&ICON_CONNECTIONS, 14.0)
                    .key("database-icon")
                    .color(k.c.text_muted)
                    .a11y_hidden(),
            )
            .children(children)
    }

    /// The path-hierarchy children: a `/` separator between segments, a
    /// fixed label or a compact select per segment
    /// (`PathHierarchyQueryContextSelectors`).
    fn segment_children(
        &self,
        k: &rune_kit::Kit,
        segments: &[PathHierarchySegment],
        layer: &mut Vec<El>,
    ) -> Vec<El> {
        let mut children = Vec::new();
        for (index, segment) in segments.iter().enumerate() {
            if index > 0 {
                // `px-0.5 text-[10px] text-fg-muted/70`, hidden from AT.
                children.push(
                    text("/")
                        .key(format!("query-context-separator-{index}"))
                        .px(2.0)
                        .font(k.ty.small)
                        .color(k.alpha(k.c.text_muted, 0.7))
                        .a11y_hidden(),
                );
            }
            match segment {
                PathHierarchySegment::Label { name } => {
                    // `max-w-[4.5rem] truncate text-[11px] text-fg-muted`
                    // with the name as title.
                    children.push(
                        text(name.clone())
                            .key(format!("query-context-path-label-{index}"))
                            .max_w(72.0)
                            .truncate()
                            .help(name)
                            .font(k.ty.small)
                            .color(k.c.text_muted),
                    );
                }
                PathHierarchySegment::Select { level_index, options, value } => {
                    children.push(self.compact_select(
                        k,
                        &format!("query-context-level-{level_index}"),
                        Some(*level_index),
                        options,
                        value,
                        level_label_key(*level_index, true),
                        layer,
                    ));
                }
            }
        }
        children
    }

    /// `QueryContextCompactSelect`: the `ui/Select` wrapper at the compact
    /// size — 64pt wide (React's `!min-w-[4rem]` floor), the level's
    /// placeholder, disabled with no options, open from the host state,
    /// and a choose that delivers `onSelectLevel(level, value)` and
    /// closes. The kit names the trigger element `{key}-trigger`.
    fn compact_select(
        &self,
        k: &rune_kit::Kit,
        key: &str,
        level_index: Option<usize>,
        options: &[String],
        value: &str,
        placeholder_key: &'static str,
        layer: &mut Vec<El>,
    ) -> El {
        let placeholder = t(placeholder_key);
        let mut select = Select::new(key, options)
            .placeholder(placeholder.clone())
            .label(placeholder)
            .width(64.0)
            .disabled(options.is_empty());
        if !value.is_empty() {
            select = select.current(Some(value));
        }
        let level = level_index.unwrap_or(0);
        if let Some(state) = self.state {
            select = select.open(state.open_level == Some(level));
        }
        // The listbox toggle and close run through the host state accessor.
        if let Some(host) = &self.host {
            let toggle_host = Rc::clone(host);
            let toggle_level = level;
            select = select.on_toggle(move |s: &mut S| {
                let state = toggle_host(s);
                state.open_level =
                    if state.open_level == Some(toggle_level) { None } else { Some(toggle_level) };
            });
            let close_host = Rc::clone(host);
            select = select.on_close(move |s: &mut S| close_host(s).close());
        }
        // The choose: React's `handlePathLevelSelect` sets the pending
        // focus level (index + 1) and calls `onSelectLevel(index, value)`
        // — the focus advance only exists in the path-hierarchy variant.
        if let Some(on_select_level) = &self.on_select_level {
            let on_select_level = Rc::clone(on_select_level);
            let host = self.host.clone();
            let advance_focus = self.path_segments.is_some();
            let level = level;
            select = select.on_choose(move |s: &mut S, value: &str, _tx: &mut rune::Tx| {
                if let Some(host) = &host {
                    host(s).open_level = None;
                    if advance_focus {
                        host(s).focus_level = Some(level + 1);
                    }
                }
                on_select_level(s, level, value);
            });
        }
        select.build(k, layer)
    }
}
