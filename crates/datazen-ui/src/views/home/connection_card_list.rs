//! Port of `packages/frontend/src/windows/connection/home/ConnectionCardList.tsx`.
//!
//! React owns the filter text and the expanded flag; Rune keeps them in this
//! view's VM and reports every change through [`ConnectionCardListHostPort`].
//! Connect / new / import are host requests — no IPC, store or async here.
//!
//! Keys match both surfaces the existing home tests pin: Rune's press targets
//! (`connection-{id}`, `conn-open-{id}`, `connections-*`) and the React markers
//! (`home-conn-card-{id}`, `home-conn-connect-{id}`, `home-connections-*`).
//!
//! Leftover L1 (removal stage): when `views/connections_home.rs` is rewritten to
//! mount this component, feed it from `AppState`, bind the host port to the
//! existing actions, and drop the old inlined card helpers plus these allows.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, icon, row, text, El, TextEdit};
use rune_kit::{Button, ButtonVariant, ControlSize};

use crate::i18n::t;
use crate::icons::{ICON_DOWNLOAD, ICON_LOADER2, ICON_PLUS, ICON_PLAY, ICON_SEARCH, ICON_X};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::views::engine_label;

/// React `COLLAPSED_COUNT`.
pub(crate) const COLLAPSED_COUNT: usize = 6;

/// React `ConnectionEntry['status']` subset this view reads.
#[allow(dead_code)] // view VM; consumer: the home host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum CardConnectionStatus {
    #[default]
    Offline,
    Connecting,
    Connected,
}

/// The `ConnectionConfig` fields the card renders.
#[allow(dead_code)] // view VM; consumer: the home host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ConnectionCardVm {
    pub id: String,
    pub name: String,
    pub engine: String,
    pub host: Option<String>,
    pub port: Option<String>,
    pub database: Option<String>,
    pub group: Option<String>,
    pub pinned: bool,
    /// ISO timestamp; sorting only needs its lexicographic order.
    pub last_connected_at: Option<String>,
}

/// React `ConnectionCardListProps` as a host-owned snapshot.
#[allow(dead_code)] // view VM; consumer: the home host (not wired).
#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectionCardListVm {
    pub connections: Vec<ConnectionCardVm>,
    pub active: BTreeMap<String, CardConnectionStatus>,
    pub filter: TextEdit,
    pub expanded: bool,
    pub can_import: bool,
    pub loading: bool,
}

/// Host requests behind the three actions and the filter/paging state.
#[allow(dead_code)] // host port; consumer: the home host (not wired).
pub(crate) struct ConnectionCardListHostPort<S> {
    pub on_connect: Option<Rc<dyn Fn(&mut S, String) + 'static>>,
    pub on_new_connection: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_import: Option<Rc<dyn Fn(&mut S) + 'static>>,
    pub on_filter: Option<Rc<dyn Fn(&mut S, String) + 'static>>,
    pub on_expand: Option<Rc<dyn Fn(&mut S, bool) + 'static>>,
    /// Accessor for the host-owned filter text so keystrokes route back into
    /// the caller's state slice.
    pub filter_edit: Option<Rc<dyn for<'b> Fn(&'b mut S) -> &'b mut TextEdit + 'static>>,
}

impl<S> Default for ConnectionCardListHostPort<S> {
    fn default() -> Self {
        Self {
            on_connect: None,
            on_new_connection: None,
            on_import: None,
            on_filter: None,
            on_expand: None,
            filter_edit: None,
        }
    }
}

/// React's `sortedConnections`: pinned first, newest `lastConnectedAt`, name.
#[allow(dead_code)] // pure rule; pinned by the card-list tests.
pub(crate) fn sorted_connections(
    connections: &[ConnectionCardVm],
) -> Vec<ConnectionCardVm> {
    let mut sorted = connections.to_vec();
    sorted.sort_by(|a, b| {
        if a.pinned != b.pinned {
            return b.pinned.cmp(&a.pinned);
        }
        match (&b.last_connected_at, &a.last_connected_at) {
            (Some(later), Some(earlier)) if later != earlier => later.cmp(earlier),
            // A timestamp sorts above a missing one (React uses 0 for absent).
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            _ => a.name.cmp(&b.name),
        }
    });
    sorted
}

/// React's filter haystack: name, host, database, group and the engine label.
#[allow(dead_code)] // pure rule; pinned by the card-list tests.
pub(crate) fn filtered_connections(
    connections: &[ConnectionCardVm],
    needle: &str,
) -> Vec<ConnectionCardVm> {
    let normalized = needle.trim().to_lowercase();
    if normalized.is_empty() {
        return connections.to_vec();
    }
    connections
        .iter()
        .filter(|conn| {
            [
                conn.name.as_str(),
                conn.host.as_deref().unwrap_or(""),
                conn.database.as_deref().unwrap_or(""),
                conn.group.as_deref().unwrap_or(""),
                conn.engine.as_str(),
            ]
            .join(" ")
            .to_lowercase()
            .contains(&normalized)
        })
        .cloned()
        .collect()
}

/// React's `hostPort` fallback chain.
#[allow(dead_code)] // pure rule; pinned by the card-list tests.
pub(crate) fn host_port_label(conn: &ConnectionCardVm) -> String {
    match conn.host.as_deref() {
        Some(host) if !host.is_empty() => match conn.port.as_deref() {
            Some(port) if !port.is_empty() => format!("{host}:{port}"),
            _ => host.to_owned(),
        },
        _ => conn
            .database
            .clone()
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| conn.engine.clone()),
    }
}

fn status_of(vm: &ConnectionCardListVm, id: &str) -> CardConnectionStatus {
    vm.active.get(id).copied().unwrap_or_default()
}

fn status_pill(status: CardConnectionStatus, dark: bool) -> El {
    let k = kit(dark);
    let (label, ink, dot) = match status {
        CardConnectionStatus::Connected => (
            t("connWin.home.status.connected"),
            k.c.success.fg,
            k.c.success.fg,
        ),
        CardConnectionStatus::Connecting => (t("conn.connecting"), k.c.text_accent, k.c.text_accent),
        CardConnectionStatus::Offline => (t("connWin.home.status.offline"), k.c.text_muted, k.c.text_muted),
    };
    row()
        .gap(6.0)
        .center_y()
        .child(row().w(6.0).h(6.0).radius(3.0).fill(dot))
        .child(text(label).font(k.ty.text_xsmall).color(ink))
}

fn card<S: 'static>(
    conn: &ConnectionCardVm,
    status: CardConnectionStatus,
    dark: bool,
    host: &ConnectionCardListHostPort<S>,
) -> El {
    let k = kit(dark);
    let id = conn.id.clone();
    let meta = {
        let base = host_port_label(conn);
        match conn.group.as_deref() {
            Some(group) if !group.is_empty() => format!("{base} · {group}"),
            _ => base,
        }
    };
    let mut row_el = row()
        .key(format!("connection-{}", conn.id))
        .child(row().key(format!("home-conn-card-{}", conn.id)).a11y_hidden())
        .w_full()
        .min_h(62.0)
        .gap(12.0)
        .center_y()
        .role("button")
        .label(format!("Connect {}", conn.name))
        .focusable(true);
    if let Some(on_connect) = host.on_connect.clone() {
        let id = id.clone();
        row_el = row_el.on_press(move |s: &mut S| on_connect(s, id.clone()));
    }
    row_el = row_el
        .child(
            col()
                .key(format!("conn-chip-{}", conn.id))
                .w(34.0)
                .h(34.0)
                .radius(8.0)
                .fill(k.c.surface_base)
                .border(1.0, k.c.border_base)
                .center()
                .child(
                    text(conn.engine.chars().take(2).collect::<String>().to_uppercase())
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_base),
                ),
        )
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(4.0)
                .child(
                    row()
                        .key(format!("conn-name-row-{}", conn.id))
                        .gap(8.0)
                        .center_y()
                        .child(text(conn.name.clone()).font(k.ty.text_small).color(k.c.text_base))
                        // React renders `getDbLabel(databaseType)` beside the
                        // name; the monogram chip above is decorative only.
                        .child(
                            text(engine_label(&conn.engine))
                                .font(k.ty.text_xsmall)
                                .color(k.c.text_muted)
                                .wrap(),
                        ),
                )
                .child(
                    text(meta)
                        .key(format!("conn-meta-{}", conn.id))
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                ),
        )
        .child(
            row()
                .key(format!("conn-status-{}", conn.id))
                .child(status_pill(status, dark)),
        );

    // React swaps in a disabled Spinner button (with no data-testid) while
    // connecting; the Connect/Open affordance only exists for other states.
    let action = if status == CardConnectionStatus::Connecting {
        row()
            .key(format!("conn-connecting-{}", conn.id))
            .w(28.0)
            .h(28.0)
            .center()
            .label(t("conn.connecting"))
            .disabled(true)
            .opacity(0.6)
            .child(icon(&ICON_LOADER2, 14.0).color(k.c.text_muted))
    } else {
        let label = if status == CardConnectionStatus::Connected {
            t("connWin.home.connections.open")
        } else {
            t("connWin.home.connections.connect")
        };
        let mut action = row()
            .key(format!("conn-open-{}", conn.id))
            .child(row().key(format!("home-conn-connect-{}", conn.id)).a11y_hidden())
            .h(28.0)
            .gap(6.0)
            .center_y()
            .role("button")
            .label(label)
            .focusable(true);
        if let Some(on_connect) = host.on_connect.clone() {
            let id = id.clone();
            action = action.on_press(move |s: &mut S| on_connect(s, id.clone()));
        }
        action
            .child(icon(&ICON_PLAY, 12.0).color(k.c.text_accent))
            .child(text(label).font(k.ty.text_xsmall).color(k.c.text_accent))
    };
    row_el.child(action)
}

/// Build the section. `loading` and empty results are honest states, never
/// faked data or IPC calls.
#[allow(dead_code)] // view entry point; consumer: the home host (not wired).
pub(crate) fn connection_card_list<S: 'static>(
    vm: &ConnectionCardListVm,
    dark: bool,
    host: ConnectionCardListHostPort<S>,
) -> El {
    let k = kit(dark);
    let sorted = sorted_connections(&vm.connections);
    let filtered = filtered_connections(&sorted, &vm.filter.text);
    let visible_count = if vm.expanded {
        filtered.len()
    } else {
        filtered.len().min(COLLAPSED_COUNT)
    };

    let mut header = row()
        .key("connections-section-header")
        .w_full()
        .gap(8.0)
        .center_y()
        .child(
            text(t("connWin.home.yourConnections"))
                .key("connections-section-title")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );

    let mut filter_field = TextField::new("home-connections-filter", &vm.filter)
        .height(28.0)
        .grow()
        .placeholder(t("connWin.home.connections.filter"));
    if let Some(accessor) = host.filter_edit.clone() {
        filter_field = filter_field.on_edit(rune::edit_access(move |s: &mut S| accessor(s)));
    } else {
        filter_field = filter_field.disabled(true);
    }
    let mut filter = row()
        .w(220.0)
        .h(28.0)
        .gap(6.0)
        .center_y()
        .fill(k.c.surface_base)
        .border(1.0, k.c.border_base)
        .child(icon(&ICON_SEARCH, 13.0).color(k.c.text_muted))
        .child(filter_field.build(dark));
    if !vm.filter.text.is_empty() {
        let mut clear_button = Button::new("×")
            .variant(ButtonVariant::Ghost)
            .control_size(ControlSize::Small)
            .icon(&ICON_X)
            .help(t("connWin.home.connections.clearFilter").to_owned());
        if let Some(on_filter) = host.on_filter.clone() {
            clear_button = clear_button.on_press(move |s: &mut S| on_filter(s, String::new()));
        }
        let clear = clear_button
            .build(k)
            .key("home-connections-filter-clear")
            .w(16.0)
            .h(16.0);
        filter = filter.child(clear);
    }
    header = header.child(filter);

    if vm.can_import {
        let mut import = row()
            .key("import-connections")
            .child(row().key("empty-import-connections-button").a11y_hidden())
            .h(28.0)
            .gap(6.0)
            .center_y()
            .label(t("common.importConnections"))
            .child(icon(&ICON_DOWNLOAD, 13.0).color(k.c.text_muted))
            .child(text(t("common.importConnections")).font(k.ty.text_xsmall).color(k.c.text_base));
        if let Some(on_import) = host.on_import.clone() {
            import = import.on_press(move |s: &mut S| on_import(s));
        }
        header = header.child(import);
    }
    let mut new_connection = row()
        .key("connection-new")
        .child(row().key("empty-new-connection-button").a11y_hidden())
        .h(28.0)
        .gap(6.0)
        .center_y()
        .label(t("common.newConnection"))
        .fill(k.c.text_accent)
        .child(icon(&ICON_PLUS, 13.0).color(k.c.text_inverse))
        .child(text(t("common.newConnection")).font(k.ty.text_xsmall).color(k.c.text_inverse));
    if let Some(on_new) = host.on_new_connection.clone() {
        new_connection = new_connection.on_press(move |s: &mut S| on_new(s));
    }
    header = header.child(new_connection);

    let mut section = col()
        .key("home-connections-section")
        .w_full()
        .gap(10.0)
        .child(header);

    let mut list = col()
        .key("connections-card-list")
        .w_full()
        .radius(12.0)
        .border(1.0, k.c.border_base);
    for conn in filtered.iter().take(visible_count) {
        let status = status_of(vm, &conn.id);
        list = list.child(card(conn, status, dark, &host));
    }
    if filtered.is_empty() {
        let mut empty = col()
            .key("home-connections-no-match")
            .w_full()
            .gap(4.0)
            .center()
            .child(icon(&ICON_SEARCH, 20.0).color(k.c.text_muted));
        empty = empty.child(
            text(
                t("connWin.home.connections.noMatch")
                    .replace("{query}", vm.filter.text.trim()),
            )
            .font(k.ty.text_xsmall)
            .color(k.c.text_muted),
        );
        if vm.can_import {
            empty = empty.child(
                text(t("common.importConnections")).font(k.ty.text_xsmall).color(k.c.text_muted),
            );
        }
        list = list.child(empty);
    }
    if vm.loading {
        list = list.child(
            text(t("conn.connecting"))
                .key("connections-loading")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );
    }
    section = section.child(list);

    if filtered.len() > COLLAPSED_COUNT || (vm.expanded && !filtered.is_empty()) {
        let mut show_all = row()
            .key("connections-show-all")
            .child(row().key("home-connections-show-all").a11y_hidden())
            .h(20.0)
            .px(6.0)
            .fill(k.c.surface_base)
            .gap(6.0)
            .label(if vm.expanded {
                t("connWin.home.connections.showLess").to_owned()
            } else {
                t("connWin.home.connections.showAll").replace("{count}", &filtered.len().to_string())
            })
            .focusable(true);
        let show_all_label = if vm.expanded {
            t("connWin.home.connections.showLess").to_owned()
        } else {
            t("connWin.home.connections.showAll").replace("{count}", &filtered.len().to_string())
        };
        show_all = show_all.child(
            text(show_all_label)
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent),
        );
        if let Some(on_expand) = host.on_expand.clone() {
            let expanded = vm.expanded;
            show_all = show_all.on_press(move |s: &mut S| on_expand(s, !expanded));
        }
        section = section.child(
            row()
                .w_full()
                .gap(8.0)
                .center_y()
                .child(
                    text(
                        t("connWin.home.connections.showing")
                            .replace("{visible}", &visible_count.to_string())
                            .replace("{total}", &filtered.len().to_string()),
                    )
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_muted),
                )
                .child(show_all),
        );
    }
    section
}
