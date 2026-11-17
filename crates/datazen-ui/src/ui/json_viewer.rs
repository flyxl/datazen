//! The JSON viewer: one raw text source rendered as tree/raw/pretty/minify.
//!
//! This is a reusable base ported from `packages/ui/src/JsonViewer.tsx`. No
//! `views/` caller uses it yet; it exists so later tasks can compose it instead
//! of re-implementing JSON display by hand.
//!
//! ## What it owns
//!
//! - Four views from the same original text: `tree` (collapsible, read-only),
//!   `raw` (exact original), `pretty` (two-space indent), and `minify` (single line).
//! - A mode bar with focusable mode buttons and an optional `on_mode` callback.
//! - Parse-on-build via `serde_json::Value`; unparseable text shows an invalid
//!   branch (`{key}-invalid`) instead of a blank panel.
//! - Default-open depth (default 2): nodes at depth `< open_depth` start expanded.
//!   Collapsed branch children are not generated at all, so large documents only
//!   render opened nodes.
//!
//! Everything visual comes from [`Kit`] (`c.*`, `k.ty.*`, `k.radius.*`); no
//! colour literals live here.
//!
//! ## What it leaves out
//!
//! - Persistence of mode/toggle state. The caller owns state and rebuilds the
//!   view: pass `mode(...)`, `toggled(...)` (paths whose open state differs from
//!   the default), `on_mode(...)`, and `on_toggle(...)`.
//! - Editing, selection/copy affordances beyond selectable text, and keyboard
//!   navigation inside the tree (arrow-key traversal) — recorded as follow-up
//!   gaps; mode buttons are focusable and activatable.
//!
//! ## Key convention
//!
//! | part        | key                    |
//! |-------------|------------------------|
//! | root        | `<key>`                |
//! | mode bar    | `<key>-modes`          |
//! | mode button | `<key>-mode-{id}`      | (`tree`, `raw`, `pretty`, `minify`)
//! | body        | `<key>-body`           |
//! | invalid     | `<key>-invalid`        |
//! | tree        | `<key>-tree`           |
//! | node        | `<key>-node-{path}`    |
//! | toggle      | `<key>-toggle-{path}`  |
//!
//! `path` is `root` for the document root, then `.<name>` for object members and
//! `[i]` for array items (for example `root.a[0]`).
// No `views/` caller uses this yet; it is a reusable base for later migration tasks.
#![allow(dead_code)]

use std::collections::BTreeSet;

use rune::{col, row, text, El};
use rune_kit::Kit;
use serde_json::Value;

/// Display mode for a JSON document.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum JsonDisplayMode {
    #[default]
    Tree,
    Raw,
    Pretty,
    Minify,
}

impl JsonDisplayMode {
    const ALL: [JsonDisplayMode; 4] = [
        JsonDisplayMode::Tree,
        JsonDisplayMode::Raw,
        JsonDisplayMode::Pretty,
        JsonDisplayMode::Minify,
    ];

    fn id(self) -> &'static str {
        match self {
            JsonDisplayMode::Tree => "tree",
            JsonDisplayMode::Raw => "raw",
            JsonDisplayMode::Pretty => "pretty",
            JsonDisplayMode::Minify => "minify",
        }
    }

    fn label(self) -> &'static str {
        match self {
            JsonDisplayMode::Tree => "Tree",
            JsonDisplayMode::Raw => "Raw",
            JsonDisplayMode::Pretty => "Pretty",
            JsonDisplayMode::Minify => "Minify",
        }
    }
}

/// Default number of levels opened in tree mode.
pub(crate) const DEFAULT_OPEN_DEPTH: usize = 2;

type ModeBind = Box<dyn Fn(El, JsonDisplayMode) -> El>;
type ToggleBind = Box<dyn Fn(El, String) -> El>;

/// A read-only JSON viewer with tree/raw/pretty/minify modes.
pub(crate) struct JsonViewer {
    key: String,
    text: String,
    mode: JsonDisplayMode,
    open_depth: usize,
    toggled: BTreeSet<String>,
    grow: bool,
    height: Option<f32>,
    on_mode: Option<ModeBind>,
    on_toggle: Option<ToggleBind>,
}

impl JsonViewer {
    pub(crate) fn new(key: impl Into<String>, text: &str) -> Self {
        Self {
            key: key.into(),
            text: text.to_owned(),
            mode: JsonDisplayMode::Tree,
            open_depth: DEFAULT_OPEN_DEPTH,
            toggled: BTreeSet::new(),
            grow: false,
            height: None,
            on_mode: None,
            on_toggle: None,
        }
    }

    pub(crate) fn mode(mut self, mode: JsonDisplayMode) -> Self {
        self.mode = mode;
        self
    }

    pub(crate) fn open_depth(mut self, depth: usize) -> Self {
        self.open_depth = depth;
        self
    }

    /// Paths whose open state is flipped from the default (`depth < open_depth`).
    pub(crate) fn toggled<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<String>,
    {
        self.toggled = paths.into_iter().map(Into::into).collect();
        self
    }

    pub(crate) fn grow(mut self) -> Self {
        self.grow = true;
        self
    }

    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Called when a mode button is pressed.
    pub(crate) fn on_mode<S: 'static>(
        mut self,
        f: impl Fn(&mut S, JsonDisplayMode) + Clone + 'static,
    ) -> Self {
        self.on_mode = Some(Box::new(move |el, mode| {
            let f = f.clone();
            el.on_press(move |s: &mut S| f(s, mode))
        }));
        self
    }

    /// Called when a tree node toggle is pressed, with the node path.
    pub(crate) fn on_toggle<S: 'static>(
        mut self,
        f: impl Fn(&mut S, String) + Clone + 'static,
    ) -> Self {
        self.on_toggle = Some(Box::new(move |el, path| {
            let f = f.clone();
            el.on_press(move |s: &mut S| f(s, path.clone()))
        }));
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let parsed = serde_json::from_str::<Value>(&self.text);
        let (value, parse_error) = match parsed {
            Ok(v) => (Some(v), None),
            Err(e) => (None, Some(e.to_string())),
        };

        let available_modes: Vec<JsonDisplayMode> = if parse_error.is_some() {
            vec![JsonDisplayMode::Raw]
        } else {
            JsonDisplayMode::ALL.to_vec()
        };
        let active_mode = if available_modes.contains(&self.mode) {
            self.mode
        } else {
            JsonDisplayMode::Raw
        };

        let modes_bar = self.build_modes_bar(k, &available_modes, active_mode);

        let mut root = col().key(self.key.clone()).w_full().gap(8.0).child(modes_bar);
        if self.grow {
            root = root.grow(1.0).min_h(0.0);
        }
        if let Some(h) = self.height {
            root = root.h(h);
        }

        if let Some(err) = parse_error {
            // Unparseable: only {key}-invalid, no {key}-body.
            let invalid = text(format!("Invalid JSON · {err}"))
                .key(format!("{}-invalid", self.key))
                .font(k.ty.text_xsmall)
                .color(k.c.warning.fg)
                .wrap_anywhere();
            return col()
                .key(self.key.clone())
                .w_full()
                .gap(8.0)
                .child(invalid);
        }

        if let Some(val) = value {
            let body = match active_mode {
                JsonDisplayMode::Tree => self.build_tree_body(k, &val),
                JsonDisplayMode::Raw => self.build_text_body(k, &self.text),
                JsonDisplayMode::Pretty => {
                    let pretty = pretty_json(&val);
                    self.build_text_body(k, &pretty)
                }
                JsonDisplayMode::Minify => {
                    let min = serde_json::to_string(&val).unwrap_or_else(|_| self.text.clone());
                    self.build_text_body(k, &min)
                }
            };
            root = root.child(body);
        }

        root
    }

    fn build_modes_bar(
        &self,
        k: &Kit,
        modes: &[JsonDisplayMode],
        active: JsonDisplayMode,
    ) -> El {
        let mut bar = row()
            .key(format!("{}-modes", self.key))
            .gap(4.0)
            .items_start();
        for mode in modes {
            let selected = *mode == active;
            let mut btn = row()
                .key(format!("{}-mode-{}", self.key, mode.id()))
                .focusable(true)
                .role("button")
                .label(mode.label())
                .px(8.0)
                .py(2.0)
                .radius(k.radius.sm)
                .child(
                    text(mode.label())
                        .font(k.ty.text_xsmall)
                        .color(if selected { k.c.text_accent } else { k.c.text_muted }),
                );
            if selected {
                btn = btn
                    .fill(k.alpha(k.c.bg_accent, 0.15))
                    .border(1.0, k.c.border_active);
            } else {
                btn = btn.hover(k.c.hover);
            }
            if let Some(bind) = &self.on_mode {
                btn = bind(btn, *mode);
            }
            bar = bar.child(btn);
        }
        bar
    }

    fn build_text_body(&self, k: &Kit, content: &str) -> El {
        col()
            .key(format!("{}-body", self.key))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(rune::ScrollSpec { stick_bottom: false })
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_base)
            .radius(k.radius.md)
            .padding(8.0)
            .child(
                text(content.to_owned())
                    .font(k.ty.mono)
                    .color(k.c.text_base)
                    .selectable(true)
                    .wrap_anywhere(),
            )
    }

    fn build_tree_body(&self, k: &Kit, value: &Value) -> El {
        let nodes = self.build_tree_node(k, "root", None, value, 0);
        col()
            .key(format!("{}-body", self.key))
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(rune::ScrollSpec { stick_bottom: false })
            .fill(k.c.layer_01)
            .border(1.0, k.c.border_base)
            .radius(k.radius.md)
            .padding(8.0)
            .child(
                col()
                    .key(format!("{}-tree", self.key))
                    .w_full()
                    .gap(2.0)
                    .child(nodes),
            )
    }

    fn is_expanded(&self, path: &str, depth: usize) -> bool {
        let default_open = depth < self.open_depth;
        if self.toggled.contains(path) {
            !default_open
        } else {
            default_open
        }
    }

    fn build_tree_node(
        &self,
        k: &Kit,
        path: &str,
        name: Option<&str>,
        value: &Value,
        depth: usize,
    ) -> El {
        let is_branch = matches!(value, Value::Array(_) | Value::Object(_));
        let expanded = is_branch && self.is_expanded(path, depth);

        let mut head = row().w_full().items_start().gap(4.0);
        if is_branch {
            let marker = if expanded { "▾" } else { "▸" };
            let mut toggle = row()
                .key(format!("{}-toggle-{}", self.key, path))
                .focusable(true)
                .role("button")
                .label(if expanded { "Collapse" } else { "Expand" })
                .px(2.0)
                .child(text(marker).font(k.ty.text_xsmall).color(k.c.text_muted));
            if let Some(bind) = &self.on_toggle {
                toggle = bind(toggle, path.to_owned());
            }
            head = head.child(toggle);
        } else {
            head = head.child(text("·").font(k.ty.text_xsmall).color(k.c.text_muted).px(2.0));
        }

        if let Some(n) = name {
            head = head.child(
                text(format!("{n}:"))
                    .font(k.ty.mono)
                    .color(k.c.text_base),
            );
        }

        if is_branch {
            let (open_ch, close_ch) = if value.is_array() { ("[…]", "[ ]") } else { ("{…}", "{ }") };
            let summary = child_summary(value);
            let preview = if expanded { close_ch } else { open_ch };
            head = head.child(text(preview).font(k.ty.mono).color(k.c.text_muted));
            if let Some(s) = summary {
                head = head.child(text(s).font(k.ty.text_xsmall).color(k.c.text_muted));
            }
        } else {
            head = head.child(scalar_text(k, value));
        }

        let mut node = col()
            .key(format!("{}-node-{}", self.key, path))
            .w_full()
            .gap(2.0)
            .child(head);

        if expanded {
            let mut children = col().w_full().gap(2.0).pl(16.0);
            children = match value {
                Value::Array(items) => {
                    let mut c = children;
                    for (i, item) in items.iter().enumerate() {
                        let child_path = format!("{path}[{i}]");
                        c = c.child(self.build_tree_node(k, &child_path, Some(&i.to_string()), item, depth + 1));
                    }
                    c
                }
                Value::Object(map) => {
                    let mut c = children;
                    for (kname, item) in map.iter() {
                        let child_path = format!("{path}.{kname}");
                        c = c.child(self.build_tree_node(k, &child_path, Some(kname), item, depth + 1));
                    }
                    c
                }
                _ => children,
            };
            node = node.child(children.key(format!("{}-slot-{}-open", self.key, path)));
        } else {
            // Keep a stable empty slot when collapsed so reconciliation drops the open subtree.
            node = node.child(col().key(format!("{}-slot-{}-closed", self.key, path)));
        }

        node
    }
}

fn child_summary(value: &Value) -> Option<String> {
    match value {
        Value::Array(items) => {
            let n = items.len();
            Some(if n == 1 {
                "1 item".to_owned()
            } else {
                format!("{n} items")
            })
        }
        Value::Object(map) => {
            let n = map.len();
            Some(if n == 1 {
                "1 key".to_owned()
            } else {
                format!("{n} keys")
            })
        }
        _ => None,
    }
}

fn scalar_text(k: &Kit, value: &Value) -> El {
    match value {
        Value::Null => text("null").font(k.ty.mono).color(k.c.text_muted),
        Value::Bool(b) => text(b.to_string()).font(k.ty.mono).color(k.c.text_accent),
        Value::Number(n) => text(n.to_string()).font(k.ty.mono).color(k.c.text_base),
        Value::String(s) => text(format!("\"{s}\"")).font(k.ty.mono).color(k.c.success.fg),
        _ => text(value.to_string()).font(k.ty.mono).color(k.c.text_base),
    }
}

fn pretty_json(value: &Value) -> String {
    let mut out = String::new();
    write_pretty(value, 0, &mut out);
    out
}

fn write_pretty(value: &Value, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    let child_indent = "  ".repeat(depth + 1);
    match value {
        Value::Array(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&child_indent);
                write_pretty(item, depth + 1, out);
                if i + 1 < items.len() {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&indent);
            out.push(']');
        }
        Value::Object(map) => {
            if map.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push_str("{\n");
            let len = map.len();
            for (i, (key, item)) in map.iter().enumerate() {
                out.push_str(&child_indent);
                out.push_str(&escape_json_string(key));
                out.push_str(": ");
                write_pretty(item, depth + 1, out);
                if i + 1 < len {
                    out.push(',');
                }
                out.push('\n');
            }
            out.push_str(&indent);
            out.push('}');
        }
        _ => out.push_str(&value.to_string()),
    }
}

fn escape_json_string(s: &str) -> String {
    // serde_json::to_string produces a properly escaped JSON string literal.
    serde_json::to_string(s).unwrap_or_else(|_| format!("\"{}\"", s.replace('"', "\\\"")))
}
