//! T030: `ExplainPlanTree.tsx` ported to Rune.
//!
//! React renders nothing when `planTree` is absent. Otherwise a small uppercase,
//! muted title and a scrollable container (`max-h 280`, `overflow auto`, border,
//! surface-alt fill, padding) hold a recursive `PlanTreeNode`. A branch row
//! toggles on click and shows a chevron (down when open, right when closed); a
//! leaf reserves a 12×12 empty lane and cannot be toggled. Rows carry a
//! depth-driven left indent of 12pt per level, an optional `cost {:.2}` accent
//! badge, an optional `rows {}` purple badge, and up to six `key: value` detail
//! lines (key in fg-secondary, value in fg-muted).
//!
//! Rune is immediate mode, so there is no component-local `useState`. Expansion
//! lives in the app's state `S` as a set of node ids that have been explicitly
//! toggled away from their lazy default (`depth < 2`). [`is_node_expanded`]
//! XORs that set with the depth default; [`toggle_node`] inserts/removes one id.
//! The set is empty until the first click, so no tree walk is needed to seed it.
//!
//! This component has no production consumer yet; it will be wired by the
//! Explain/query-view track. Reusable API items carry per-item
//! `#[allow(dead_code)]` (not a module-wide blanket) until that caller lands.
//!
//! ## Keys (prefix defaults to `explain-plan-tree`)
//!
//! | part | key |
//! |------|-----|
//! | root | `<prefix>` (absent when the plan is `None`) |
//! | title | `<prefix>-title` |
//! | scroll container | `<prefix>-scroll` |
//! | node wrapper | `<prefix>-node-<id>` |
//! | row | `<prefix>-node-<id>-row` |
//! | chevron lane | `<prefix>-node-<id>-chevron` |
//! | chevron glyph | `<prefix>-node-<id>-glyph-down` / `-glyph-right` |
//! | label | `<prefix>-node-<id>-label` |
//! | cost badge | `<prefix>-node-<id>-cost` |
//! | rows badge | `<prefix>-node-<id>-rows` |
//! | body column | `<prefix>-node-<id>-body` |
//! | header | `<prefix>-node-<id>-header` |
//! | details column | `<prefix>-node-<id>-details` |
//! | detail line | `<prefix>-node-<id>-detail-<i>` (first six only) |
//! | detail key / value | `<prefix>-node-<id>-detail-<i>-key` / `-value` |
//!
//! Detail lines are keyed by index, not by `detail.key`, so two details with
//! the same key cannot collide into duplicate element keys.

use std::collections::HashSet;

use rune::{col, el, icon, row, text, Color, El, Key, KeyEvent, ScrollSpec, TextStyle};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT};
use crate::ui::kit::kit;
use crate::ui::palette::code_palette;

/// Node ids explicitly toggled away from their depth default. Empty means
/// every node uses `depth < 2`.
#[allow(dead_code)] // reusable component; wired by the Explain view track
pub(crate) type ExpandOverrides = HashSet<String>;

/// One `ExplainPlanNode` view model (React `types.ExplainPlanNode`).
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable component; wired by the Explain view track
pub(crate) struct ExplainPlanNode {
    pub id: String,
    pub label: String,
    pub cost: Option<f64>,
    pub rows: Option<i64>,
    /// `(key, value)` pairs; only the first six are rendered, like React.
    pub details: Vec<(String, String)>,
    pub children: Vec<ExplainPlanNode>,
}

/// React's `useState(depth < 2)` default.
#[allow(dead_code)] // reusable component; wired by the Explain view track
pub(crate) fn default_expanded(depth: u32) -> bool {
    depth < 2
}

/// Whether a node is expanded: its lazy depth default, flipped when the id is
/// present in the override set. A node with no children is never rendered as
/// expandable, but the answer is still well-defined for tests and callers.
#[allow(dead_code)] // reusable component; wired by the Explain view track
pub(crate) fn is_node_expanded(overrides: &ExpandOverrides, id: &str, depth: u32) -> bool {
    default_expanded(depth) ^ overrides.contains(id)
}

/// Flip one node's expansion in `S` without seeding the whole tree.
#[allow(dead_code)] // reusable component; wired by the Explain view track
pub(crate) fn toggle_node(overrides: &mut ExpandOverrides, id: impl Into<String>) {
    let id = id.into();
    if !overrides.remove(&id) {
        overrides.insert(id);
    }
}

type ToggleFn<S> = fn(&mut S, String);

/// The explain plan tree. `read_overrides` and `toggle` locate the expansion
/// state in the app's `S`; no expansion state is stored on this struct.
#[allow(dead_code)] // reusable component; wired by the Explain view track
pub(crate) struct ExplainPlanTree<S: 'static> {
    key: String,
    plan_tree: Option<ExplainPlanNode>,
    read_overrides: fn(&S) -> &ExpandOverrides,
    toggle: ToggleFn<S>,
}

#[allow(dead_code)] // reusable component; wired by the Explain view track
impl<S: 'static> ExplainPlanTree<S> {
    pub(crate) fn new(
        plan_tree: Option<ExplainPlanNode>,
        read_overrides: fn(&S) -> &ExpandOverrides,
        toggle: ToggleFn<S>,
    ) -> Self {
        Self {
            key: "explain-plan-tree".to_string(),
            plan_tree,
            read_overrides,
            toggle,
        }
    }

    /// Override the key prefix (two trees can then share one state slot safely
    /// only if their node ids differ; ids are otherwise the identity).
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// Builds the tree, or `el()` (nothing) when there is no plan.
    pub(crate) fn build(self, s: &S, dark: bool) -> El {
        let Some(root) = self.plan_tree else {
            return el();
        };
        let k = kit(dark);
        let overrides = (self.read_overrides)(s);
        // kit has no purple token; the syntax palette's violet ink is the
        // closest reusable token for React's `purple-400` rows badge.
        let purple = code_palette(dark).keyword;
        let ctx = RenderCtx {
            k,
            prefix: &self.key,
            overrides,
            toggle: self.toggle,
            purple,
            purple_fill: purple.alpha(0.10),
            badge_font: TextStyle {
                size: 10.0,
                line_height: 13.0,
                ..k.ty.text_xsmall
            },
            detail_font: TextStyle {
                size: 10.0,
                line_height: 13.0,
                ..k.ty.text_xsmall
            },
        };

        let title = text(t("explain.planTree"))
            .key(format!("{}-title", self.key))
            .font(TextStyle {
                tracking: 0.6,
                ..k.ty.small_medium
            })
            .color(k.c.text_muted);

        let scroll = col()
            .key(format!("{}-scroll", self.key))
            .w_full()
            .max_h(280.0)
            .padding(8.0)
            .radius(k.radius.sm)
            .border(1.0, k.c.border_base)
            .fill(k.c.layer_01)
            .scroll(ScrollSpec { stick_bottom: false })
            .child(render_node(&root, 0, &ctx));

        col()
            .key(self.key)
            .w_full()
            .gap(6.0)
            .child(title)
            .child(scroll)
    }
}

/// Immutable render inputs shared by the recursive row builder.
struct RenderCtx<'a, S: 'static> {
    k: &'a Kit,
    prefix: &'a str,
    overrides: &'a ExpandOverrides,
    toggle: ToggleFn<S>,
    purple: Color,
    purple_fill: Color,
    badge_font: TextStyle,
    detail_font: TextStyle,
}

/// One `PlanTreeNode`: the wrapper owns the row and its (conditionally present)
/// children, mirroring React's `<div className="selectable">`.
fn render_node<S: 'static>(node: &ExplainPlanNode, depth: u32, ctx: &RenderCtx<'_, S>) -> El {
    let base = format!("{}-node-{}", ctx.prefix, node.id);
    let has_children = !node.children.is_empty();
    let expanded = has_children && is_node_expanded(ctx.overrides, &node.id, depth);
    let k = ctx.k;

    // Chevron lane: a 12×12 glyph for branches, an empty 12×12 span for leaves.
    // The glyph's key and type both change when a branch is toggled.
    let chevron = if has_children {
        let (glyph, glyph_key) = if expanded {
            (&ICON_CHEVRON_DOWN, format!("{base}-glyph-down"))
        } else {
            (&ICON_CHEVRON_RIGHT, format!("{base}-glyph-right"))
        };
        el()
            .key(format!("{base}-chevron"))
            .size(12.0, 12.0)
            .shrink()
            .child(icon(glyph, 12.0).key(glyph_key).color(k.c.icon_muted))
    } else {
        el()
            .key(format!("{base}-chevron"))
            .size(12.0, 12.0)
            .shrink()
    };

    // Header: label + optional cost/rows badges.
    let mut header = row()
        .key(format!("{base}-header"))
        .w_full()
        .flex_wrap()
        .items_center()
        .gap(6.0)
        .child(
            text(node.label.clone())
                .key(format!("{base}-label"))
                .font(k.ty.text_xsmall_medium)
                .color(k.c.text_base)
                .shrink(),
        );
    if let Some(cost) = node.cost {
        header = header.child(
            text(format!("cost {cost:.2}"))
                .key(format!("{base}-cost"))
                .font(ctx.badge_font)
                .color(k.c.text_accent)
                .fill(k.alpha(k.c.bg_accent, 0.10))
                .radius(4.0)
                .px(6.0)
                .py(2.0),
        );
    }
    if let Some(rows) = node.rows {
        header = header.child(
            text(format!("rows {rows}"))
                .key(format!("{base}-rows"))
                .font(ctx.badge_font)
                .color(ctx.purple)
                .fill(ctx.purple_fill)
                .radius(4.0)
                .px(6.0)
                .py(2.0),
        );
    }

    // Body: header followed by at most six `key: value` detail lines.
    let mut body = col()
        .key(format!("{base}-body"))
        .min_w(0.0)
        .grow(1.0)
        .gap(2.0)
        .child(header);
    if !node.details.is_empty() {
        let mut details = col()
            .key(format!("{base}-details"))
            .w_full()
            .min_w(0.0)
            .mt(2.0)
            .gap(2.0);
        for (i, (detail_key, detail_value)) in node.details.iter().take(6).enumerate() {
            let line = row()
                .key(format!("{base}-detail-{i}"))
                .w_full()
                .items_start()
                .gap(4.0)
                .child(
                    text(format!("{detail_key}: "))
                        .key(format!("{base}-detail-{i}-key"))
                        .font(ctx.detail_font)
                        .color(k.c.text_muted)
                        .shrink(),
                )
                .child(
                    text(detail_value.clone())
                        .key(format!("{base}-detail-{i}-value"))
                        .font(ctx.detail_font)
                        .color(k.c.text_faint)
                        .wrap()
                        .min_w(0.0)
                        .grow(1.0),
                );
            details = details.child(line);
        }
        body = body.child(details);
    }

    // Only a branch is clickable/focusable; a leaf row is inert as a toggle.
    let mut row_el = row()
        .key(format!("{base}-row"))
        .w_full()
        .items_start()
        .gap(4.0)
        .pl(depth as f32 * 12.0)
        .pr(4.0)
        .py(2.0)
        .radius(k.radius.sm)
        .child(chevron)
        .child(body);
    if has_children {
        let id = node.id.clone();
        let toggle = ctx.toggle;
        row_el = row_el
            .focusable(true)
            .hover(k.c.hover)
            .expanded(expanded)
            .on_press(move |s: &mut S| toggle(s, id.clone()));
        // Rune already routes Space on a focused row to `on_press`; Enter is
        // delivered only here, so the two paths cannot double-toggle.
        let id = node.id.clone();
        row_el = row_el.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut rune::Tx| {
            if matches!(ev.key, Key::Enter) {
                toggle(s, id.clone());
                true
            } else {
                false
            }
        });
    }

    // The wrapper is a column: React's `<div className="selectable">` stacks
    // the row above its children. `el()` would default to a horizontal layout.
    let mut wrapper = col().key(base.clone()).w_full().child(row_el);
    if expanded {
        let mut children = col().key(format!("{base}-children")).w_full();
        for child in &node.children {
            children = children.child(render_node(child, depth + 1, ctx));
        }
        wrapper = wrapper.child(children);
    }
    wrapper
}
