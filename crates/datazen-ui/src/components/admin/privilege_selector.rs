//! T064: `components/admin/PrivilegeSelector.tsx` ported to Rune.
//!
//! React renders the `grant_privileges` command's privilege groups
//! (`PrivilegeSelector.tsx:17–47`): one labelled section per group —
//! the label in 11pt medium muted uppercase with wide tracking, the
//! group spaced 12pt below when it has a label — over a grid of
//! checkbox rows (the privilege name next to its box, text 14pt in
//! the foreground colour, the whole row clickable). The groups come
//! from `privilegeGroups` (`lib/commandSchema.ts:65–85`), which reads
//! `inputSchema['x-datazen'].privilegeGroups`: non-object entries are
//! skipped, only string privileges are kept, groups with no privileges
//! are dropped, and a non-string label becomes `""`. `allPrivileges`
//! (`:86–90`) flattens the groups in order, and
//! `usePrivilegeOptions` (`PrivilegeSelector.tsx:49–53`) returns both
//! — the two callers (`CreateUserDialog`, `PrivilegeView`) use `all`
//! for their select-all / deselect-all buttons.
//!
//! The component is stateless, like React: the host owns the groups
//! (parsed with [`privilege_groups`]) and the selection. Selection
//! arrives as an `is_selected` predicate over the app's `S` and
//! toggles flow back through `on_toggle`, the same shape
//! `ExportColumnPicker` uses. The production consumers are the
//! `CreateUserDialog` / `PrivilegeView` wiring tasks; until then the
//! component tests are the only callers, so reusable API items carry
//! per-item `#[allow(dead_code)]` (not a module-wide blanket).
//!
//! ## Keys (prefix defaults to `privilege-selector`)
//!
//! | part | key |
//! |---|---|
//! | root | `<prefix>` |
//! | group `G` section | `<prefix>-group-{G}` |
//! | group `G` label | `<prefix>-group-{G}-label` |
//! | group `G` grid | `<prefix>-group-{G}-grid` |
//! | group `G` privilege `P` row | `<prefix>-group-{G}-item-{P}` |
//! | group `G` privilege `P` checkbox | `<prefix>-group-{G}-item-{P}-checkbox` |
//! | group `G` privilege `P` name | `<prefix>-group-{G}-item-{P}-name` |
//!
//! `G` is the group label, or `"default"` when the group has none —
//! React's `key={group.label || 'default'}`. Two groups with the same
//! label collide here exactly as they do in React.
//!
//! ## Deliberate deviations
//!
//! * **No `className` / `itemClassName` strings.** React takes
//!   Tailwind classes; Rune has no class strings, so the two known
//!   call sites' values are builder options instead: the defaults
//!   match `PrivilegeSelector`'s own defaults (2 columns, 8pt grid
//!   gap, 14pt items, 8pt item gap) and `PrivilegeView`'s overrides
//!   map to `.columns(3).grid_gap(6.0).item_font_size(11.0)
//!   .item_gap(6.0)`.
//! * **The grid is `equal_columns`, not CSS grid.** `grid-cols-2` /
//!   `grid-cols-3` map to `equal_columns(2)` / `equal_columns(3)`
//!   (`repeat(n, 1fr)`), and `gap-2` / `gap-1.5` to the row gap.
//!   Items fill the cells in order, as in CSS grid.
//! * **The name toggles through its own press handler, not the
//!   `<label>`.** React wraps the checkbox and the name in one
//!   `<label>`, so clicking anywhere on the row toggles it. Here the
//!   checkbox box and the name text each call `on_toggle` — the same
//!   shape `ExportColumnPicker` uses, because one press handler on
//!   the row would double-fire together with the checkbox's own
//!   toggle. Clicking the checkbox fires once; clicking the name
//!   fires once.
//! * **`tracking-wide` is a `TextStyle` tracking, not a class.**
//!   Tailwind's `tracking-wide` is 0.025em, so the 11pt group label
//!   carries `tracking(0.275)`.
//! * **No `definition?: DriverCommandDefinition` prop.** React takes
//!   the whole definition and renders nothing when there are no
//!   groups; here the host parses it first (with
//!   [`privilege_groups`]) and passes the groups. An empty group list
//!   renders the empty root — the same nothing React renders.

use rune::{El, TextCase, TextStyle, col, row, text};
use rune_kit::Kit;
use serde_json::Value;

use crate::ui::kit::kit;
use crate::ui::toggle::Checkbox;

/// The group label's tracking: Tailwind `tracking-wide` is 0.025em, at
/// 11pt that is 0.275pt.
const GROUP_LABEL_TRACKING: f32 = 0.275;
/// The labelled group's bottom margin (`mb-3`).
const GROUP_MARGIN_BOTTOM: f32 = 12.0;
/// The label-to-grid gap (`mb-1.5`).
const GROUP_LABEL_GAP: f32 = 6.0;

/// One privilege group (`lib/commandSchema.ts:9–12` `PrivilegeGroup`):
/// a label ("" when the driver sent none) and its privileges, in
/// driver order.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) struct PrivilegeGroup {
    pub label: String,
    pub privileges: Vec<String>,
}

#[allow(dead_code)] // reusable component; wired by the grant dialogs
impl PrivilegeGroup {
    pub(crate) fn new(
        label: impl Into<String>,
        privileges: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            label: label.into(),
            privileges: privileges.into_iter().map(Into::into).collect(),
        }
    }

    /// React's group key (`PrivilegeSelector.tsx:23`):
    /// the label, or `"default"` when the group has none.
    pub(crate) fn key(&self) -> &str {
        if self.label.is_empty() {
            "default"
        } else {
            &self.label
        }
    }
}

/// `privilegeGroups` (`lib/commandSchema.ts:65–85`): the definition's
/// `inputSchema['x-datazen'].privilegeGroups`. A missing or
/// non-object `x-datazen`, or a non-array `privilegeGroups`, yields no
/// groups; a non-object entry is skipped; only string privileges are
/// kept; a group with no privileges is dropped; a non-string label
/// becomes `""`.
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) fn privilege_groups(input_schema: &Value) -> Vec<PrivilegeGroup> {
    let Some(raw) = input_schema
        .get("x-datazen")
        .and_then(Value::as_object)
        .and_then(|ext| ext.get("privilegeGroups"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let mut groups = Vec::new();
    for item in raw {
        let Some(record) = item.as_object() else {
            continue;
        };
        let privileges: Vec<String> = record
            .get("privileges")
            .and_then(Value::as_array)
            .map(|array| {
                array
                    .iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        if privileges.is_empty() {
            continue;
        }
        groups.push(PrivilegeGroup {
            label: record
                .get("label")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string(),
            privileges,
        });
    }
    groups
}

/// The groups the component renders: the parsed groups minus the
/// empty ones (`privilegeGroups` already drops them at parse time; the
/// filter keeps the rule true for groups built by hand, too).
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) fn visible_privilege_groups<'a>(
    groups: &'a [PrivilegeGroup],
) -> Vec<&'a PrivilegeGroup> {
    groups
        .iter()
        .filter(|group| !group.privileges.is_empty())
        .collect()
}

/// `allPrivileges` (`lib/commandSchema.ts:86–90`): every privilege of
/// every visible group, flattened in group order — what
/// `usePrivilegeOptions` returns as `all` for the callers'
/// select-all / deselect-all buttons.
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) fn all_privileges(groups: &[PrivilegeGroup]) -> Vec<&str> {
    visible_privilege_groups(groups)
        .into_iter()
        .flat_map(|group| group.privileges.iter().map(String::as_str))
        .collect()
}

/// React's `selected.has(privilege)` on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) type IsPrivilegeSelectedFn<S> = fn(&S, &str) -> bool;

/// React's `onToggle(privilege)` on the app's `S`.
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) type TogglePrivilegeFn<S> = fn(&mut S, String);

/// The privilege selector: one labelled section per group over a grid
/// of checkbox rows. The host owns the groups and the selection; this
/// component only renders them.
#[allow(dead_code)] // reusable component; wired by the grant dialogs
pub(crate) struct PrivilegeSelector<S: 'static> {
    key: String,
    groups: Vec<PrivilegeGroup>,
    is_selected: IsPrivilegeSelectedFn<S>,
    on_toggle: TogglePrivilegeFn<S>,
    columns: usize,
    grid_gap: f32,
    item_font_size: f32,
    item_gap: f32,
}

#[allow(dead_code)] // reusable component; wired by the grant dialogs
impl<S: 'static> PrivilegeSelector<S> {
    /// React `<PrivilegeSelector definition={…} selected={…}
    /// onToggle={…} />` with the definition already parsed (see
    /// [`privilege_groups`]).
    pub(crate) fn new(
        groups: Vec<PrivilegeGroup>,
        is_selected: IsPrivilegeSelectedFn<S>,
        on_toggle: TogglePrivilegeFn<S>,
    ) -> Self {
        Self {
            key: "privilege-selector".to_string(),
            groups,
            is_selected,
            on_toggle,
            // React's `className` default: `grid grid-cols-2 gap-2`.
            columns: 2,
            grid_gap: 8.0,
            // React's `itemClassName` default: `flex items-center gap-2
            // text-sm text-fg cursor-pointer`.
            item_font_size: 14.0,
            item_gap: 8.0,
        }
    }

    /// Override the key prefix (two selectors in one dialog need
    /// distinct keys; the default is one fixed string).
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    /// The grid's column count: React's `grid-cols-2` default,
    /// `grid-cols-3` in `PrivilegeView`.
    pub(crate) fn columns(mut self, columns: usize) -> Self {
        self.columns = columns;
        self
    }

    /// The grid's gap: React's `gap-2` default, `gap-1.5` in
    /// `PrivilegeView`.
    pub(crate) fn grid_gap(mut self, grid_gap: f32) -> Self {
        self.grid_gap = grid_gap;
        self
    }

    /// The item text size: React's `text-sm` (14pt) default,
    /// `text-[11px]` in `PrivilegeView`.
    pub(crate) fn item_font_size(mut self, item_font_size: f32) -> Self {
        self.item_font_size = item_font_size;
        self
    }

    /// The item's checkbox-to-name gap: React's `gap-2` default,
    /// `gap-1.5` in `PrivilegeView`.
    pub(crate) fn item_gap(mut self, item_gap: f32) -> Self {
        self.item_gap = item_gap;
        self
    }

    /// Builds the selector: one section per visible group, in group
    /// order (`PrivilegeSelector.tsx:20–46`). No visible groups renders
    /// the empty root, as React returns `null` when `groups.length`
    /// is 0.
    pub(crate) fn build(self, s: &S, dark: bool) -> El {
        let k = kit(dark);
        let mut root = col().key(self.key.clone()).w_full();
        for group in visible_privilege_groups(&self.groups) {
            root = root.child(self.group_el(s, group, &k));
        }
        root
    }

    /// One group (`PrivilegeSelector.tsx:22–44`): the label when the
    /// group has one — 11pt medium muted uppercase with wide tracking,
    /// 6pt above the grid, 12pt below the section — over the grid.
    fn group_el(&self, s: &S, group: &PrivilegeGroup, k: &Kit) -> El {
        let gkey = format!("{}-group-{}", self.key, group.key());
        let mut section = col().key(gkey.clone()).w_full().gap(GROUP_LABEL_GAP);
        if !group.label.is_empty() {
            section = section.mb(GROUP_MARGIN_BOTTOM).child(
                text(group.label.clone())
                    .key(format!("{gkey}-label"))
                    .font(k.ty.small_medium.tracking(GROUP_LABEL_TRACKING))
                    .color(k.c.text_muted)
                    .text_case(TextCase::Uppercase),
            );
        }
        section.child(self.grid_el(s, group, &gkey, k))
    }

    /// The group's grid (`PrivilegeSelector.tsx:30–41`): the
    /// privileges in order, filling `equal_columns` cells left to
    /// right, top to bottom.
    fn grid_el(&self, s: &S, group: &PrivilegeGroup, gkey: &str, k: &Kit) -> El {
        let mut grid = row()
            .key(format!("{gkey}-grid"))
            .w_full()
            .equal_columns(self.columns)
            .gap(self.grid_gap);
        for privilege in &group.privileges {
            grid = grid.child(self.item_el(s, group, privilege, k));
        }
        grid
    }

    /// One privilege (`PrivilegeSelector.tsx:32–40`): the checkbox and
    /// the name, both toggling it. The checkbox box and the name text
    /// each call `on_toggle` (see the module docs: one handler on the
    /// row would double-fire with the checkbox's own toggle).
    fn item_el(&self, s: &S, group: &PrivilegeGroup, privilege: &str, k: &Kit) -> El {
        let base = format!("{}-group-{}-item-{}", self.key, group.key(), privilege);
        let checked = (self.is_selected)(s, privilege);
        let toggle = self.on_toggle;

        let name = privilege.to_string();
        let mut checkbox = Checkbox::new(format!("{base}-checkbox"), checked);
        checkbox = checkbox.on_toggle(move |state: &mut S| toggle(state, name.clone()));

        let toggle_name = privilege.to_string();
        row()
            .key(base.clone())
            .w_full()
            .items_center()
            .gap(self.item_gap)
            .child(checkbox.build(k))
            .child(
                text(privilege.to_string())
                    .key(format!("{base}-name"))
                    .font(TextStyle::new(self.item_font_size, 400.0))
                    .color(k.c.text_base)
                    .on_press(move |state: &mut S| toggle(state, toggle_name.clone())),
            )
    }
}
