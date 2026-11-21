//! `PanelTabBar`, ported 1:1 from
//! `packages/frontend/src/windows/connection/PanelTabBar.tsx` and the pure
//! `contentViewHelpers.getPanelTabLabel` / `getPanelIcon` helpers it uses.
//!
//! React renders nothing for an empty panel list; otherwise a 48pt (`h-12`)
//! strip whose single child is a horizontally scrollable tablist. Each tab is
//! a full-height row with an icon, a `connection · database · object` label, a
//! close button and a 2pt active underline. Selection, close and context-menu
//! requests are emitted as [`PanelTabAction`]s through an injected host port —
//! no panel store, command or thread is faked here.
//!
//! ## Documented deviations
//!
//! * React's `ThemedIcon` ids are not Rune registry glyphs. The tab icon uses
//!   the closest `icons.rs` entry per panel type; the close button uses the
//!   registry `ICON_X` (React's `X`). Swap when the themed-icon registry lands.
//! * The React component has **no dirty marker** and no drag-reorder; neither
//!   is invented here.
//! * Arrow/Home/End keyboard movement calls `Tx::focus` on the next tab (React
//!   focuses its `tabRefs` entry) and emits the same `Select` action.
//!
//! ## Consumer / removal stage
//!
//! No production host mounts this strip yet. The connection-page host will own
//! the panel slice and translate the three actions into its store calls; until
//! then every reusable item carries a per-item `#[allow(dead_code)]` with this
//! removal stage.

use std::rc::Rc;

use rune::{el, icon, row, text, Color, El, Key, KeyEvent, Tx};
use rune_kit::{IconButton, IconButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_CONNECTIONS,ICON_LAYOUT, ICON_REFRESH, ICON_SETTINGS, ICON_TABLE, ICON_TERMINAL, ICON_X};
use crate::ui::kit::kit;

const ROOT: &str = "panel-tab-bar";
/// React's `h-12` (48pt) pinned by `PanelTabBarHeight.test.tsx`.
pub(crate) const BAR_HEIGHT: f32 = 48.0;
const UNDERLINE_HEIGHT: f32 = 2.0;

/// React's `Panel['type']` union (the variants that render an icon/label).
#[allow(dead_code)] // VM mirror; consumed by the label/icon helpers and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PanelType {
    Table,
    View,
    #[default]
    Query,
    CreateTable,
    ErDiagram,
    Objects,
    Privileges,
    ServerStatus,
    Processes,
    DbObject,
    RedisDb,
}

impl PanelType {
    /// React `getPanelShortLabel` for panel types whose label is a locale key.
    #[allow(dead_code)] // pure helper; consumed by tab_label and the tests.
    pub(crate) fn localized_short(self) -> Option<&'static str> {
        match self {
            Self::CreateTable => Some("common.newTable"),
            Self::ErDiagram => Some("common.erDiagram"),
            Self::Objects => Some("objects.title"),
            Self::Privileges => Some("privileges.title"),
            Self::ServerStatus => Some("serverStatus.dashboardTitle"),
            Self::Processes => Some("common.processList"),
            _ => None,
        }
    }

    /// Closest registry glyph for React's `ThemedIcon` id (see module docs).
    #[allow(dead_code)] // pure helper; consumed by the tab builder and tests.
    pub(crate) fn icon(self) -> &'static rune::Icon {
        match self {
            Self::Table | Self::View | Self::CreateTable | Self::DbObject => &ICON_TABLE,
            Self::Query => &ICON_TERMINAL,
            Self::ErDiagram => &ICON_LAYOUT,
            Self::Objects => &ICON_CONNECTIONS,
            Self::Privileges => &ICON_SETTINGS,
            Self::ServerStatus | Self::Processes | Self::RedisDb => &ICON_REFRESH,
        }
    }
}

/// Host-owned projection of one React `Panel` plus the session database the
/// tab label may fall back to.
#[allow(dead_code)] // VM surface; supplied by the connection-page host.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PanelTabVm {
    pub(crate) id: String,
    pub(crate) panel_type: PanelType,
    /// React's panel-specific `title` / `tableName` / `viewName` / `objectName`.
    pub(crate) name: String,
    pub(crate) database: Option<String>,
    pub(crate) connection_name: String,
    pub(crate) session_database: Option<String>,
}

impl PanelTabVm {
    /// React `getPanelTabLabel`: `connection · database · object`, with the
    /// database omitted when unknown and the object from a locale key for the
    /// fixed panel types.
    #[allow(dead_code)] // pure helper; consumed by the tab builder and tests.
    pub(crate) fn tab_label(&self) -> String {
        let short = self
            .panel_type
            .localized_short()
            .map(t)
            .unwrap_or(self.name.as_str())
            .to_owned();
        let database = self
            .database
            .as_deref()
            .or(self.session_database.as_deref())
            .map(str::trim)
            .filter(|value| !value.is_empty());
        let mut parts = vec![self.connection_name.as_str()];
        if let Some(database) = database {
            parts.push(database);
        }
        if !short.is_empty() {
            parts.push(short.as_str());
        }
        parts.join(" · ")
    }
}

/// Discrete interactions the strip asks the host to apply to `S`.
#[allow(dead_code)] // host port; remove when the connection-page host lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PanelTabAction {
    Select(String),
    Close(String),
    ContextMenu(String),
}

type ActionFn<S> = Rc<dyn Fn(&mut S, PanelTabAction) + 'static>;

fn emit<S: 'static>(on_action: &Option<ActionFn<S>>, action: PanelTabAction) -> impl Fn(&mut S) + Clone {
    let callback = on_action.clone();
    move |s: &mut S| {
        if let Some(callback) = &callback {
            callback(s, action.clone());
        }
    }
}

/// A zero-size, inert marker carrying a React `data-testid` that has no Rune
/// element of its own. Keeps React's observable keys without colliding with
/// the per-id keys used to drive keyboard focus.
fn react_key(key: &str) -> El {
    el().key(key).a11y_hidden()
}

/// Build the strip, or `None` when React would `return null` (no panels).
#[allow(dead_code)] // consumer: the connection-page host (not landed).
pub(crate) fn build<S: 'static>(
    panels: &[PanelTabVm],
    active_panel_id: Option<&str>,
    on_action: Option<ActionFn<S>>,
    dark: bool,
) -> Option<El> {
    if panels.is_empty() {
        return None;
    }
    let k = kit(dark);
    let ids: Rc<Vec<String>> = Rc::new(panels.iter().map(|panel| panel.id.clone()).collect());
    let mut list = row()
        .key("panel-tab-list")
        .role("tablist")
        .label(t("panel.tabListLabel"))
        .w_full()
        .min_w(0.0)
        .scroll_x()
        .items_stretch();
    for (index, panel) in panels.iter().enumerate() {
        list = list.child(build_tab(panel, index, active_panel_id, &ids, &on_action, dark));
    }
    Some(
        row()
            .key(ROOT)
            .w_full()
            .h(BAR_HEIGHT)
            .shrink()
            .fill(k.c.layer_01)
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base)
            .border_layout()
            .items_stretch()
            .child(list.grow(1.0).min_w(0.0)),
    )
}

fn build_tab<S: 'static>(
    panel: &PanelTabVm,
    index: usize,
    active_panel_id: Option<&str>,
    ids: &Rc<Vec<String>>,
    on_action: &Option<ActionFn<S>>,
    dark: bool,
) -> El {
    let k = kit(dark);
    let active = active_panel_id == Some(panel.id.as_str());
    let label = panel.tab_label();
    let select_key = format!("panel-tab-select-{}", panel.id);
    let close_key = format!("panel-tab-close-{}", panel.id);

    let mut select = row()
        .key(select_key.clone())
        .role("tab")
        .focusable(active)
        .focus_selected(active)
        .label(label.clone())
        .h_full()
        .gap(6.0)
        .center_y()
        .on_press(emit(on_action, PanelTabAction::Select(panel.id.clone())));
    if let Some(callback) = on_action.clone() {
        let ids = ids.clone();
        select = select.on_key(move |s: &mut S, ev: &KeyEvent, tx: &mut Tx| -> bool {
            let len = ids.len();
            if len == 0 {
                return false;
            }
            let target = match ev.key {
                Key::Right => (index + 1) % len,
                Key::Left => (index + len - 1) % len,
                Key::Home => 0,
                Key::End => len - 1,
                _ => return false,
            };
            let next_id = ids[target].clone();
            tx.focus(format!("panel-tab-select-{next_id}"), false);
            callback(s, PanelTabAction::Select(next_id));
            true
        });
    }
    select = select
        .child(icon(panel.panel_type.icon(), 14.0).color(k.c.icon_base))
        .child(
            text(label.clone())
                .max_w(200.0)
                .color(if active { k.c.text_base } else { k.c.text_muted })
                .truncate(),
        );

    let close = IconButton::new(&ICON_X)
        .variant(IconButtonVariant::Ghost)
        .control_size(ControlSize::Mini)
        .label(t("panel.closeTab").replace("{title}", &label))
        .on_press(emit(on_action, PanelTabAction::Close(panel.id.clone())))
        .build(k)
        .key(close_key);

    let mut tab = row()
        .key(format!("panel-tab-{}", panel.id))
        .h_full()
        .gap(6.0)
        .px(12.0)
        .center_y()
        .fill(if active { k.c.layer_02 } else { Color::TRANSPARENT })
        .hover(if active { k.c.layer_02 } else { k.c.hover })
        .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_base)
        .on_secondary_press({
            let callback = on_action.clone();
            let id = panel.id.clone();
            move |s: &mut S, _press, _tx: &mut Tx| {
                if let Some(callback) = &callback {
                    callback(s, PanelTabAction::ContextMenu(id.clone()));
                }
            }
        })
        .child(react_key("panel-tab"))
        .child(select.child(react_key("panel-tab-select")))
        .child(close.child(react_key("panel-tab-close")));
    if active {
        tab = tab.child(
            row()
                .key(format!("panel-tab-underline-{}", panel.id))
                .h(UNDERLINE_HEIGHT)
                .w_full()
                .fill(k.c.border_focus),
        );
    }
    tab
}
