//! Tabs: tablist/tab strip with roving focus and optional tabpanel.
//!
//! Ported from `packages/ui/src/Tabs.tsx`. Two modes:
//! - **panel mode** (`panel` set): renders bar + tablist + `role="tabpanel"` body.
//! - **bar-only mode** (`panel` absent): tablist is the root (no extra wrapper).
//!
//! Roving tabindex: active tab is the single Tab stop (`focusable(true)`), others
//! are not focusable. Left/Right/Home/End move selection (automatic activation)
//! and invoke `on_change`; the host rebuilds with the new active tab so roving
//! focus follows the active item. Enter/Space activate the focused tab.
//!
//! ## Accessibility
//!
//! Tab selection is exposed with `focus_selected(selected)` on each tab (the active
//! tab reports selected, others unselected) while roving tabindex keeps only the
//! active tab focusable. In panel mode the tabpanel is associated by name via
//! `role("tabpanel")` and a label matching the active tab's accessible label; rune
//! does not currently expose `aria-controls`, so this name-based association is used
//! until upstream provides it.
//!
//! ## Key convention
//!
//! | part        | key                |
//! |-------------|--------------------|
//! | root        | `<key>`            |
//! | tablist     | `<key>-tablist`    |
//! | tab         | `<key>-tab-{id}`   |
//! | panel       | `<key>-panel`      |
//!
// No `views/` caller uses this yet; it is a reusable base for later migration tasks.

use rune::{col, row, text, Color, El, Key, KeyEvent};
use std::sync::Arc;

#[allow(dead_code)] // reusable base; remove when view callers land
#[derive(Clone)]
pub(crate) struct TabItem {
    pub id: String,
    pub label: String,
    pub aria_label: Option<String>,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl TabItem {
    pub(crate) fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            aria_label: None,
        }
    }

    pub(crate) fn aria_label(mut self, label: impl Into<String>) -> Self {
        self.aria_label = Some(label.into());
        self
    }
}

type OnChange<S> = Box<dyn Fn(&mut S, String) + 'static>;

#[allow(dead_code)] // reusable base; remove when view callers land
pub(crate) struct Tabs<S: 'static> {
    key: String,
    items: Vec<TabItem>,
    active_id: String,
    panel: Option<El>,
    on_change: Option<OnChange<S>>,
    aria_label: Option<String>,
}

#[allow(dead_code)] // reusable base; remove when view callers land
impl<S: 'static> Tabs<S> {
    pub(crate) fn new(
        key: impl Into<String>,
        items: Vec<TabItem>,
        active_id: impl Into<String>,
    ) -> Self {
        Self {
            key: key.into(),
            items,
            active_id: active_id.into(),
            panel: None,
            on_change: None,
            aria_label: None,
        }
    }

    /// Panel content for the active tab. If omitted, renders bar-only strip.
    pub(crate) fn panel(mut self, panel: El) -> Self {
        self.panel = Some(panel);
        self
    }

    pub(crate) fn on_change(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    pub(crate) fn aria_label(mut self, label: impl Into<String>) -> Self {
        self.aria_label = Some(label.into());
        self
    }

    pub(crate) fn build(self, dark: bool) -> El {
        let k = crate::ui::kit::kit(dark);
        let active_index = self
            .items
            .iter()
            .position(|i| i.id == self.active_id)
            .unwrap_or(0);
        let active_label = self
            .items
            .get(active_index)
            .map(|item| {
                item.aria_label
                    .clone()
                    .unwrap_or_else(|| item.label.clone())
            })
            .unwrap_or_default();
        let panel_key = format!("{}-panel", self.key);

        let on_change = Arc::new(self.on_change);
        let items = Arc::new(self.items);

        let mut list = row()
            .key(format!("{}-tablist", self.key))
            .role("tablist")
            .items_center()
            .gap(4.0)
            .min_w(0.0)
            .scroll(rune::ScrollSpec {
                stick_bottom: false,
            });
        if let Some(label) = &self.aria_label {
            list = list.label(label.clone());
        }

        for (index, item) in items.iter().enumerate() {
            let selected = index == active_index;
            let id = item.id.clone();
            let ids = items.clone();
            let handler = on_change.clone();

            let mut tab = row()
                .key(format!("{}-tab-{}", self.key, item.id))
                .role("tab")
                .focusable(selected) // roving tabindex: only active tab is a Tab stop
                .focus_selected(selected)
                .h(28.0)
                .px(10.0)
                .center()
                .radius(k.radius.sm)
                .fill(if selected {
                    k.c.layer_02
                } else {
                    Color::TRANSPARENT
                })
                .hover(if selected { k.c.layer_02 } else { k.c.hover })
                .label(
                    item.aria_label
                        .clone()
                        .unwrap_or_else(|| item.label.clone()),
                )
                .child(
                    text(item.label.clone())
                        .font(if selected {
                            k.ty.text_small_medium
                        } else {
                            k.ty.text_small
                        })
                        .color(if selected {
                            k.c.text_base
                        } else {
                            k.c.text_muted
                        })
                        .a11y_hidden(),
                );

            if selected {
                tab = tab.border(1.0, k.c.border_base);
            }

            if let Some(_) = handler.as_ref() {
                let id_press = id.clone();
                let h_press = on_change.clone();
                tab = tab.on_press(move |s: &mut S| {
                    if let Some(ref f) = *h_press {
                        f(s, id_press.clone());
                    }
                });
                let ids_k = ids.clone();
                let id_key = id.clone();
                let h_key = on_change.clone();
                tab = tab.on_key(move |s: &mut S, ev: &KeyEvent, _tx: &mut rune::Tx| {
                    if ids_k.is_empty() {
                        return false;
                    }
                    let len = ids_k.len();
                    let target = match ev.key {
                        Key::Right => Some(ids_k[(index + 1) % len].id.clone()),
                        Key::Left => Some(ids_k[(index + len - 1) % len].id.clone()),
                        Key::Home => Some(ids_k[0].id.clone()),
                        Key::End => Some(ids_k[len - 1].id.clone()),
                        Key::Enter => Some(id_key.clone()),
                        Key::Text(ref t) if t == " " => Some(id_key.clone()),
                        _ => None,
                    };
                    if let Some(next_id) = target {
                        if let Some(ref f) = *h_key {
                            f(s, next_id);
                        }
                        return true;
                    }
                    false
                });
            }

            list = list.child(tab);
        }

        if let Some(panel) = self.panel {
            col()
                .key(self.key)
                .w_full()
                .min_w(0.0)
                .grow(1.0)
                .min_h(0.0)
                .child(
                    row()
                        .w_full()
                        .h(40.0)
                        .px(8.0)
                        .center_y()
                        .fill(k.c.layer_01)
                        .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
                        .border_layout()
                        .child(list.grow(1.0).min_w(0.0)),
                )
                .child(
                    col()
                        .key(panel_key)
                        .role("tabpanel")
                        .label(active_label)
                        .w_full()
                        .grow(1.0)
                        .min_h(0.0)
                        .min_w(0.0)
                        .child(panel),
                )
        } else {
            // Bar-only: root is the tablist container with required root key.
            row()
                .key(self.key)
                .w_full()
                .min_w(0.0)
                .child(list.grow(1.0).min_w(0.0))
        }
    }
}
