//! Port of `packages/frontend/src/components/connection/ConnectionAdvancedSettings.tsx`.
//!
//! An accordion of advanced connection options: an "advanced" section (SSL mode,
//! read-only, colour tag and group) followed by a "tunnel" section (source,
//! saved/inline tunnel, save-as). Each section is a chevron toggle; the chevron
//! flips between `ICON_CHEVRON_RIGHT` (collapsed) and `ICON_CHEVRON_DOWN`
//! (expanded), exactly like React's `ChevronRight`/`ChevronDown`.
//!
//! ## Representable surface / boundary
//!
//! The React file depends on ports that are **separate tasks** and are not
//! ported here: `SshTunnelFields`, `HttpProxyTunnelFields`,
//! `WebSocketTunnelFields`, `SaveTunnelDialog`, `shared::{COLOR_KEYS, Label}`,
//! `openSettingsWindow` and `getDriverConnectionAdvanced`. They are modelled as
//! minimal view-model fields and callbacks:
//!
//! - the tunnel field body is represented by a zero-size, `a11y_hidden` marker
//!   `<key>-inline-fields-boundary` (the real field components will replace it);
//! - `openSettingsWindow('tunnels')` becomes `AdvancedAction::OpenSettings`;
//! - `setSaveDialogOpen(true)` becomes `AdvancedAction::SaveAs`;
//! - `getDriverConnectionAdvanced` is always `None` here, so the plain SSL-mode
//!   select is rendered whenever `vm.supports_ssl` (React's `else` branch);
//! - `COLOR_KEYS` and `Label` are local constants/kit tokens rather than imports.
//!
//! All state (which sections are expanded, which select is open, every field
//! value) lives in the owning `S` through [`AdvancedSettingsVm`]; every
//! interaction is funnelled through one [`on_action`](ConnectionAdvancedSettings::on_action)
//! callback so the caller's state stays the single source of truth.
//!
//! ## Key convention
//!
//! React `data-testid` values are kept verbatim with the default prefix
//! `new-conn` (override with [`key`](ConnectionAdvancedSettings::key)). Nested
//! kit selects derive `<key>-trigger` / `<key>-content` / `<key>-scrim` from the
//! field key, so the SSL select is `<prefix>-ssl-mode-field` and its trigger is
//! `<prefix>-ssl-mode-field-trigger`.
//!
//! | part                     | key                                  |
//! |--------------------------|--------------------------------------|
//! | root                     | `<prefix>`                           |
//! | advanced toggle/chevron  | `<prefix>-advanced-toggle[-chevron]` |
//! | advanced panel           | `<prefix>-advanced-panel`             |
//! | SSL block / select       | `<prefix>-ssl-mode` / `-ssl-mode-field` |
//! | read-only checkbox       | `<prefix>-read-only`                 |
//! | colour tag / group       | `<prefix>-color-tag` / `-group`      |
//! | tunnel toggle/chevron    | `<prefix>-tunnel-toggle[-chevron]`   |
//! | tunnel panel             | `<prefix>-tunnel-panel`              |
//! | tunnel-missing alert     | `<prefix>-tunnel-missing`            |
//! | tunnel source select     | `<prefix>-tunnel-source-field`       |
//! | tunnel empty / actions   | `<prefix>-tunnel-empty` / `-tunnel-create-entry` / `-tunnel-manage-entry` |
//! | saved tunnel summary     | `<prefix>-saved-tunnel-summary`      |
//! | unbind / save-as         | `<prefix>-tunnel-unbind` / `-tunnel-save-as` |
//! | error banners            | `<prefix>-tunnel-error-saved` / `-tunnel-error-inline` |
//!
//! No colour literal is authored here: chrome tones come from `kit(dark)`.
//! The seven colour-tag swatches parse the React hex **data** values with
//! `rune::color::hex`; they are option data, not style tokens.

use std::rc::Rc;

use rune::{col, el, icon, row, text, El, Tx};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT};
use crate::i18n::t;
use crate::ui::banner::{Banner, BannerElement, BannerVariant};
use crate::ui::select::Select;
use crate::ui::toggle::Checkbox;

/// React `shared.tsx::COLOR_KEYS`: the tag value (a hex colour) and its i18n key.
#[allow(dead_code)] // reusable ported data; consumed by this module's build
pub(crate) const COLOR_TAGS: [(&str, &str); 7] = [
    ("#ef4444", "newConn.colorRed"),
    ("#f59e0b", "newConn.colorOrange"),
    ("#22c55e", "newConn.colorGreen"),
    ("#3b82f6", "newConn.colorBlue"),
    ("#8b5cf6", "newConn.colorPurple"),
    ("#ec4899", "newConn.colorPink"),
    ("#64748b", "newConn.colorGray"),
];

/// React `TunnelSource` union.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum TunnelSource {
    None,
    Saved,
    Inline,
}

#[allow(dead_code)] // reusable VM surface
impl TunnelSource {
    pub(crate) fn from_value(value: &str) -> Self {
        match value {
            "saved" => TunnelSource::Saved,
            "inline" => TunnelSource::Inline,
            _ => TunnelSource::None,
        }
    }

    fn value(self) -> &'static str {
        match self {
            TunnelSource::None => "none",
            TunnelSource::Saved => "saved",
            TunnelSource::Inline => "inline",
        }
    }
}

/// React `TunnelKind` union.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum TunnelKind {
    Ssh,
    HttpProxy,
    WebSocket,
}

#[allow(dead_code)] // reusable VM surface
impl TunnelKind {
    pub(crate) fn from_value(value: &str) -> Option<Self> {
        match value {
            "ssh" => Some(TunnelKind::Ssh),
            "httpProxy" => Some(TunnelKind::HttpProxy),
            "websocket" => Some(TunnelKind::WebSocket),
            _ => None,
        }
    }

    fn value(self) -> &'static str {
        match self {
            TunnelKind::Ssh => "ssh",
            TunnelKind::HttpProxy => "httpProxy",
            TunnelKind::WebSocket => "websocket",
        }
    }

    /// React `tunnelBadge`: the short label on the tunnel toggle.
    pub(crate) fn badge(self) -> &'static str {
        match self {
            TunnelKind::Ssh => "SSH",
            TunnelKind::HttpProxy => "HTTP",
            TunnelKind::WebSocket => "WS",
        }
    }
}

/// React `form.savedTunnels[i]`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) struct SavedTunnel {
    pub id: String,
    pub name: String,
    pub kind: String,
}

/// Which select currently owns the app's open state. React lets each `Select`
/// manage this internally; here it lives in `S` so tree/key probes can observe it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum SelectKind {
    SslMode,
    Group,
    TunnelSource,
    SavedTunnel,
    TunnelKind,
}

/// Every interaction the widget can ask the host to apply to `S`.
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(dead_code)] // public VM surface; wired by later callers/tests
pub(crate) enum AdvancedAction {
    ToggleAdvanced,
    ToggleTunnel,
    /// `None` closes whichever select is open.
    OpenSelect(Option<SelectKind>),
    /// The chosen option's stored value (not its label).
    SslMode(String),
    ReadOnly(bool),
    /// A colour-tag hex value.
    ColorTag(String),
    /// A group option's stored value.
    Group(String),
    TunnelSource(TunnelSource),
    TunnelKind(TunnelKind),
    /// `None` is React's `setTunnelId(value || null)` for the empty option.
    TunnelId(Option<String>),
    CreateEntry,
    /// React `openSettingsWindow('tunnels')`.
    OpenSettings,
    Unbind,
    /// React `setSaveDialogOpen(true)` (dialog is a separate task).
    SaveAs,
}

/// The view model. A caller stores this in its `S` and rebuilds it every frame;
/// the widget never owns state.
#[derive(Clone, Debug)]
#[allow(dead_code)] // reusable VM surface; wired by later callers/tests
pub(crate) struct AdvancedSettingsVm {
    pub show_advanced: bool,
    pub show_tunnel: bool,
    /// React `variant === 'window'`.
    pub is_window: bool,
    pub open: Option<SelectKind>,
    pub supports_ssl: bool,
    pub ssl_mode: String,
    /// `(value, label)` in listbox order.
    pub ssl_options: Vec<(String, String)>,
    pub driver_read_only: bool,
    pub read_only: bool,
    pub color_tag: String,
    /// `None` renders only the colour tag (React's non-grid branch).
    pub group_options: Option<Vec<(String, String)>>,
    pub group: String,
    pub tunnel_source: TunnelSource,
    pub tunnel_kind: TunnelKind,
    /// React `form.effectiveTunnelKind` (the badge source).
    pub effective_tunnel_kind: Option<TunnelKind>,
    pub saved_tunnels: Vec<SavedTunnel>,
    pub tunnel_id: Option<String>,
    pub saved_tunnel: Option<SavedTunnel>,
    pub tunnel_ref_missing: bool,
    pub tunnel_busy: bool,
    pub tunnel_error: String,
    pub tunnel_inline_valid: bool,
    pub supports_ssh: bool,
}

#[allow(dead_code)] // test/initial-state convenience
impl Default for AdvancedSettingsVm {
    fn default() -> Self {
        AdvancedSettingsVm {
            show_advanced: false,
            show_tunnel: false,
            is_window: false,
            open: None,
            supports_ssl: true,
            ssl_mode: "prefer".to_owned(),
            // React `useConnectionForm.ts:705-712` labels are "Disable — …" etc.
            ssl_options: vec![
                (
                    "disable".to_owned(),
                    format!("Disable — {}", t("newConn.sslNone")),
                ),
                (
                    "prefer".to_owned(),
                    format!("Prefer — {}", t("newConn.sslPrefer")),
                ),
                (
                    "require".to_owned(),
                    format!("Require — {}", t("newConn.sslRequire")),
                ),
            ],
            driver_read_only: false,
            read_only: false,
            color_tag: COLOR_TAGS[0].0.to_owned(),
            group_options: None,
            group: String::new(),
            tunnel_source: TunnelSource::None,
            tunnel_kind: TunnelKind::HttpProxy,
            effective_tunnel_kind: None,
            saved_tunnels: Vec::new(),
            tunnel_id: None,
            saved_tunnel: None,
            tunnel_ref_missing: false,
            tunnel_busy: false,
            tunnel_error: String::new(),
            tunnel_inline_valid: true,
            supports_ssh: true,
        }
    }
}

type ActionFn<S> = Rc<dyn Fn(&mut S, AdvancedAction) + 'static>;

/// Wrap an action as a zero-argument press handler.
fn press<S: 'static>(f: &Option<ActionFn<S>>, a: AdvancedAction) -> impl Fn(&mut S) {
    let f = f.clone();
    move |s: &mut S| {
        if let Some(cb) = &f {
            cb(s, a.clone());
        }
    }
}

/// Builder for the ported component.
#[allow(dead_code)] // reusable component; wired by later callers/tasks
pub(crate) struct ConnectionAdvancedSettings<S: 'static> {
    key: String,
    vm: AdvancedSettingsVm,
    on_action: Option<ActionFn<S>>,
}

#[allow(dead_code)] // reusable component; wired by later callers/tasks
impl<S: 'static> ConnectionAdvancedSettings<S> {
    pub(crate) fn new(vm: AdvancedSettingsVm) -> Self {
        ConnectionAdvancedSettings {
            key: "new-conn".to_owned(),
            vm,
            on_action: None,
        }
    }

    /// Override the key prefix (defaults to the React test id prefix).
    pub(crate) fn key(mut self, key: impl Into<String>) -> Self {
        self.key = key.into();
        self
    }

    pub(crate) fn on_action(mut self, f: impl Fn(&mut S, AdvancedAction) + 'static) -> Self {
        self.on_action = Some(Rc::new(f));
        self
    }

    /// Builds the whole accordion, pushing any open select listbox into `layer`.
    pub(crate) fn build(self, k: &Kit, layer: &mut Vec<El>) -> El {
        let ConnectionAdvancedSettings { key: p, vm, on_action } = self;
        let label_font = k.ty.text_small;
        let hint_font = k.ty.text_xsmall;
        let base = k.c.text_base;
        let muted = k.c.text_muted;
        let faint = k.c.text_faint;
        let accent = k.c.text_accent;
        // React: `isWindow ? bg-surface : bg-surface-alt` for both panels.
        let panel_fill = if vm.is_window {
            k.c.surface_base
        } else {
            k.c.surface_weak
        };

        let mut root = col().key(p.clone()).w_full().gap(8.0);

        // ---- Advanced section ---------------------------------------------------------------
        let adv_open = vm.show_advanced;
        let adv_chevron = if adv_open {
            icon(&ICON_CHEVRON_DOWN, 14.0)
        } else {
            icon(&ICON_CHEVRON_RIGHT, 14.0)
        }
        .key(format!("{p}-advanced-toggle-chevron"))
        .color(muted);
        let adv_toggle = row()
            .key(format!("{p}-advanced-toggle"))
            .w_full()
            .items_center()
            .gap(8.0)
            .role("button")
            .focusable(true)
            .expanded(adv_open)
            .fill(k.c.surface_base)
            .border(1.0, k.alpha(faint, 0.5))
            .radius(k.radius.md)
            .px(12.0)
            .py(10.0)
            .on_press(press(&on_action, AdvancedAction::ToggleAdvanced))
            .child(adv_chevron)
            .child(
                text(t("newConn.advanced"))
                    .key(format!("{p}-advanced-toggle-label"))
                    .font(label_font)
                    .color(muted),
            );
        root = root.child(adv_toggle);

        if adv_open {
            let mut panel = col()
                .key(format!("{p}-advanced-panel"))
                .w_full()
                .gap(16.0)
                .fill(panel_fill)
                .border(1.0, k.alpha(faint, 0.4))
                .radius(k.radius.md)
                .padding(16.0);

            // DriverAdvanced is a separate task, so React's `else` branch (the plain
            // SSL-mode select) is the only one rendered here.
            if vm.supports_ssl {
                let labels: Vec<&str> = vm.ssl_options.iter().map(|o| o.1.as_str()).collect();
                let values: Vec<&str> = vm.ssl_options.iter().map(|o| o.0.as_str()).collect();
                let mut sel = Select::new(format!("{p}-ssl-mode-field"), &labels)
                    .values(&values)
                    .current(Some(vm.ssl_mode.as_str()))
                    .label(t("newConn.sslMode"))
                    .open(vm.open == Some(SelectKind::SslMode))
                    .on_toggle(press(
                        &on_action,
                        AdvancedAction::OpenSelect(Some(SelectKind::SslMode)),
                    ))
                    .on_close(press(&on_action, AdvancedAction::OpenSelect(None)));
                let f = on_action.clone();
                sel = sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                    if let Some(cb) = &f {
                        cb(s, AdvancedAction::SslMode(value.to_owned()));
                        cb(s, AdvancedAction::OpenSelect(None));
                    }
                });
                panel = panel.child(
                    col()
                        .key(format!("{p}-ssl-mode"))
                        .gap(6.0)
                        .child(
                            text(t("newConn.sslMode"))
                                .key(format!("{p}-ssl-label"))
                                .font(label_font)
                                .color(base),
                        )
                        .child(sel.build(k, layer)),
                );
            }

            // React's read-only checkbox: checked by the driver lock OR the user's
            // choice; a locked driver disables it and swaps the hint text.
            let hint = if vm.driver_read_only {
                t("newConn.driverReadOnlyLocked")
            } else {
                t("newConn.readOnlyHint")
            };
            panel = panel.child(
                Checkbox::new(format!("{p}-read-only"), vm.driver_read_only || vm.read_only)
                    .label(t("newConn.readOnly"))
                    .description(hint)
                    .disabled(vm.driver_read_only)
                    .on_toggle(press(&on_action, AdvancedAction::ReadOnly(!vm.read_only)))
                    .build(k),
            );

            // Colour tag swatches (React maps over COLOR_KEYS).
            let mut swatches = row().gap(8.0).flex_wrap();
            for (i, (value, i18n_key)) in COLOR_TAGS.iter().enumerate() {
                let swatch_color = rune::color::hex(value).unwrap_or(k.c.text_muted);
                let selected = vm.color_tag == *value;
                let border = if selected {
                    k.c.text_base
                } else {
                    k.alpha(k.c.text_base, 0.0)
                };
                swatches = swatches.child(
                    el()
                        .key(format!("{p}-color-{i}"))
                        .size(24.0, 24.0)
                        .shrink()
                        .radius(12.0)
                        .fill(swatch_color)
                        .border(2.0, border)
                        .label(t(i18n_key))
                        .on_press(press(&on_action, AdvancedAction::ColorTag((*value).to_owned()))),
                );
            }
            let colors = col()
                .key(format!("{p}-color-tag"))
                .gap(6.0)
                .child(
                    text(t("newConn.colorTag"))
                        .key(format!("{p}-color-tag-label"))
                        .font(label_font)
                        .color(base),
                )
                .child(swatches);

            if let Some(groups) = &vm.group_options {
                let labels: Vec<&str> = groups.iter().map(|o| o.1.as_str()).collect();
                let values: Vec<&str> = groups.iter().map(|o| o.0.as_str()).collect();
                let mut sel = Select::new(format!("{p}-group-field"), &labels)
                    .values(&values)
                    .current(Some(vm.group.as_str()))
                    .label(t("newConn.group"))
                    .open(vm.open == Some(SelectKind::Group))
                    .on_toggle(press(
                        &on_action,
                        AdvancedAction::OpenSelect(Some(SelectKind::Group)),
                    ))
                    .on_close(press(&on_action, AdvancedAction::OpenSelect(None)));
                let f = on_action.clone();
                sel = sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                    if let Some(cb) = &f {
                        cb(s, AdvancedAction::Group(value.to_owned()));
                        cb(s, AdvancedAction::OpenSelect(None));
                    }
                });
                let group = col()
                    .key(format!("{p}-group"))
                    .gap(6.0)
                    .child(
                        text(t("newConn.group"))
                            .key(format!("{p}-group-label"))
                            .font(label_font)
                            .color(base),
                    )
                    .child(sel.build(k, layer))
                    .grow(1.0)
                    .min_w(160.0);
                // React uses a two-column grid at md and up; a wrapping row keeps both
                // blocks inside the minimum 960pt window.
                panel = panel.child(
                    row()
                        .w_full()
                        .gap(16.0)
                        .flex_wrap()
                        .child(colors.grow(1.0).min_w(160.0))
                        .child(group),
                );
            } else {
                panel = panel.child(colors);
            }
            root = root.child(panel);
        }

        // ---- Tunnel section -----------------------------------------------------------------
        let tunnel_open = vm.show_tunnel;
        let tunnel_chevron = if tunnel_open {
            icon(&ICON_CHEVRON_DOWN, 14.0)
        } else {
            icon(&ICON_CHEVRON_RIGHT, 14.0)
        }
        .key(format!("{p}-tunnel-toggle-chevron"))
        .color(muted);
        let mut tunnel_toggle = row()
            .key(format!("{p}-tunnel-toggle"))
            .w_full()
            .items_center()
            .gap(8.0)
            .role("button")
            .focusable(true)
            .expanded(tunnel_open)
            .fill(k.c.surface_base)
            .border(1.0, k.alpha(faint, 0.5))
            .radius(k.radius.md)
            .px(12.0)
            .py(10.0)
            .on_press(press(&on_action, AdvancedAction::ToggleTunnel))
            .child(tunnel_chevron)
            .child(
                text(t("newConn.tunnel"))
                    .key(format!("{p}-tunnel-toggle-label"))
                    .font(label_font)
                    .color(muted),
            );
        if let Some(badge_kind) = vm.effective_tunnel_kind {
            tunnel_toggle = tunnel_toggle.child(
                text(badge_kind.badge())
                    .key(format!("{p}-tunnel-badge"))
                    .font(k.ty.text_xsmall_medium)
                    .color(accent)
                    .fill(k.alpha(accent, 0.2))
                    .radius(4.0)
                    .px(6.0)
                    .py(2.0),
            );
        }
        root = root.child(tunnel_toggle);

        if tunnel_open {
            let mut tpanel = col()
                .key(format!("{p}-tunnel-panel"))
                .w_full()
                .gap(16.0)
                .fill(panel_fill)
                .border(1.0, k.alpha(faint, 0.4))
                .radius(k.radius.md)
                .padding(16.0);

            // Deliberately NOT an ErrorBanner: React uses an amber *warning* here
            // ("you picked a tunnel that no longer exists"). Routing it through the
            // error component would mislabel it. The second sentence only appears
            // when a saved tunnel is actually listed (React BUG-002).
            if vm.tunnel_ref_missing {
                let mut message = t("newConn.tunnelMissing").to_owned();
                if !vm.saved_tunnels.is_empty() {
                    message.push(' ');
                    message.push_str(t("newConn.tunnelMissingAlt"));
                }
                tpanel = tpanel.child(
                    col()
                        .key(format!("{p}-tunnel-missing"))
                        .role("alert")
                        .gap(4.0)
                        .fill(k.alpha(k.c.warning.fg, 0.10))
                        .border(1.0, k.alpha(k.c.warning.fg, 0.30))
                        .radius(k.radius.sm)
                        .padding(10.0)
                        .child(
                            text(message)
                                .key(format!("{p}-tunnel-missing-text"))
                                .font(hint_font)
                                .color(k.c.warning.fg)
                                .wrap(),
                        ),
                );
            }

            // Tunnel source is the master control. React offers "saved" but disables
            // it with no saved tunnels; the kit has no per-option disabled state, so
            // the row is left out (same decision as DefaultOrderSelect).
            let has_saved = !vm.saved_tunnels.is_empty();
            let (src_labels, src_values): (Vec<&str>, Vec<&str>) = if has_saved {
                (
                    vec![
                        t("newConn.tunnelNone"),
                        t("newConn.savedTunnel"),
                        t("newConn.savedTunnelNone"),
                    ],
                    vec!["none", "saved", "inline"],
                )
            } else {
                (
                    vec![t("newConn.tunnelNone"), t("newConn.savedTunnelNone")],
                    vec!["none", "inline"],
                )
            };
            let mut src_sel = Select::new(format!("{p}-tunnel-source-field"), &src_labels)
                .values(&src_values)
                .current(Some(vm.tunnel_source.value()))
                .label(t("newConn.tunnelSource"))
                .open(vm.open == Some(SelectKind::TunnelSource))
                .on_toggle(press(
                    &on_action,
                    AdvancedAction::OpenSelect(Some(SelectKind::TunnelSource)),
                ))
                .on_close(press(&on_action, AdvancedAction::OpenSelect(None)));
            let f = on_action.clone();
            src_sel = src_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                if let Some(cb) = &f {
                    cb(s, AdvancedAction::TunnelSource(TunnelSource::from_value(value)));
                    cb(s, AdvancedAction::OpenSelect(None));
                }
            });
            tpanel = tpanel.child(
                col()
                    .key(format!("{p}-tunnel-source"))
                    .gap(6.0)
                    .child(
                        text(t("newConn.tunnelSource"))
                            .key(format!("{p}-tunnel-source-label"))
                            .font(label_font)
                            .color(base),
                    )
                    .child(src_sel.build(k, layer)),
            );

            // Empty state: no saved tunnel and not yet configuring inline.
            if vm.tunnel_source != TunnelSource::Inline && vm.saved_tunnels.is_empty() {
                let create = Button::new(t("newConn.tunnelCreateEntry"))
                    .variant(ButtonVariant::Neutral)
                    .control_size(ControlSize::Small)
                    .on_press(press(&on_action, AdvancedAction::CreateEntry))
                    .build(k)
                    .key(format!("{p}-tunnel-create-entry"));
                let manage = Button::new(t("newConn.tunnelManage"))
                    .variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small)
                    .on_press(press(&on_action, AdvancedAction::OpenSettings))
                    .build(k)
                    .key(format!("{p}-tunnel-manage-entry"));
                tpanel = tpanel.child(
                    col()
                        .key(format!("{p}-tunnel-empty"))
                        .gap(8.0)
                        .border(1.0, k.alpha(faint, 0.5))
                        .radius(k.radius.sm)
                        .padding(12.0)
                        .child(
                            text(t("newConn.tunnelEmptyHint"))
                                .key(format!("{p}-tunnel-empty-hint"))
                                .font(hint_font)
                                .color(muted)
                                .wrap(),
                        )
                        .child(row().gap(8.0).flex_wrap().child(create).child(manage)),
                );
            }

            // Saved-tunnel branch.
            if vm.tunnel_source == TunnelSource::Saved {
                let labels: Vec<String> = vm
                    .saved_tunnels
                    .iter()
                    .map(|tunnel| format!("{} ({})", tunnel.name, tunnel.kind))
                    .collect();
                let values: Vec<&str> = vm
                    .saved_tunnels
                    .iter()
                    .map(|tunnel| tunnel.id.as_str())
                    .collect();
                let label_refs: Vec<&str> = labels.iter().map(|s| s.as_str()).collect();
                let mut saved_sel = Select::new(format!("{p}-saved-tunnel-field"), &label_refs)
                    .values(&values)
                    .current(vm.tunnel_id.as_deref())
                    .label(t("newConn.savedTunnel"))
                    .open(vm.open == Some(SelectKind::SavedTunnel))
                    .on_toggle(press(
                        &on_action,
                        AdvancedAction::OpenSelect(Some(SelectKind::SavedTunnel)),
                    ))
                    .on_close(press(&on_action, AdvancedAction::OpenSelect(None)));
                let f = on_action.clone();
                saved_sel = saved_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                    if let Some(cb) = &f {
                        cb(
                            s,
                            AdvancedAction::TunnelId(if value.is_empty() {
                                None
                            } else {
                                Some(value.to_owned())
                            }),
                        );
                        cb(s, AdvancedAction::OpenSelect(None));
                    }
                });
                let mut saved = col()
                    .key(format!("{p}-saved-tunnel"))
                    .gap(12.0)
                    .child(
                        text(t("newConn.tunnelSavedHint"))
                            .key(format!("{p}-saved-tunnel-hint"))
                            .font(hint_font)
                            .color(muted)
                            .wrap(),
                    )
                    .child(
                        col()
                            .gap(6.0)
                            .child(
                                text(t("newConn.savedTunnel"))
                                    .key(format!("{p}-saved-tunnel-label"))
                                    .font(label_font)
                                    .color(base),
                            )
                            .child(saved_sel.build(k, layer)),
                    );
                if let Some(st) = &vm.saved_tunnel {
                    saved = saved.child(
                        row()
                            .key(format!("{p}-saved-tunnel-summary"))
                            .items_center()
                            .gap(8.0)
                            .flex_wrap()
                            .child(
                                text(st.name.clone())
                                    .key(format!("{p}-saved-tunnel-name"))
                                    .font(hint_font)
                                    .color(base),
                            )
                            .child(
                                text(st.kind.clone())
                                    .key(format!("{p}-saved-tunnel-kind"))
                                    .font(k.ty.text_xsmall_medium)
                                    .color(accent)
                                    .fill(k.alpha(accent, 0.2))
                                    .radius(4.0)
                                    .px(6.0)
                                    .py(2.0),
                            ),
                    );
                }
                // Unbinding would refill inline fields from the stored entity; when it is
                // gone there is nothing to refill, so the action is not offered (React
                // tunnel-form-BUG-002). "None (direct)" remains the reachable exit.
                if !vm.tunnel_ref_missing {
                    saved = saved.child(
                        Button::new(t("newConn.tunnelUnbind"))
                            .variant(ButtonVariant::Neutral)
                            .control_size(ControlSize::Small)
                            .disabled(vm.tunnel_busy)
                            .on_press(press(&on_action, AdvancedAction::Unbind))
                            .build(k)
                            .key(format!("{p}-tunnel-unbind")),
                    );
                }
                if let Some(banner) = Banner::new(
                    format!("{p}-tunnel-error-saved"),
                    vm.tunnel_error.clone(),
                )
                .variant(BannerVariant::Boxed)
                .build(k)
                {
                    saved = saved.child(banner);
                }
                tpanel = tpanel.child(saved);
            }

            // Inline-tunnel branch. The three field components are separate tasks; only
            // the kind selector, save-as action and error banner are representable here.
            if vm.tunnel_source == TunnelSource::Inline {
                let mut kind_labels: Vec<&str> = Vec::new();
                let mut kind_values: Vec<&str> = Vec::new();
                if vm.supports_ssh {
                    kind_labels.push(t("newConn.tunnelSsh"));
                    kind_values.push("ssh");
                }
                kind_labels.push(t("newConn.tunnelHttpProxy"));
                kind_values.push("httpProxy");
                kind_labels.push(t("newConn.tunnelWebSocket"));
                kind_values.push("websocket");
                let mut kind_sel = Select::new(format!("{p}-tunnel-kind-field"), &kind_labels)
                    .values(&kind_values)
                    .current(Some(vm.tunnel_kind.value()))
                    .label(t("newConn.tunnelKind"))
                    .open(vm.open == Some(SelectKind::TunnelKind))
                    .on_toggle(press(
                        &on_action,
                        AdvancedAction::OpenSelect(Some(SelectKind::TunnelKind)),
                    ))
                    .on_close(press(&on_action, AdvancedAction::OpenSelect(None)));
                let f = on_action.clone();
                kind_sel = kind_sel.on_choose(move |s: &mut S, value: &str, _tx: &mut Tx| {
                    if let Some(cb) = &f {
                        if let Some(kind) = TunnelKind::from_value(value) {
                            cb(s, AdvancedAction::TunnelKind(kind));
                        }
                        cb(s, AdvancedAction::OpenSelect(None));
                    }
                });
                let mut inline = col()
                    .key(format!("{p}-inline-tunnel"))
                    .gap(12.0)
                    .child(
                        col()
                            .key(format!("{p}-tunnel-kind"))
                            .gap(6.0)
                            .child(
                                text(t("newConn.tunnelKind"))
                                    .key(format!("{p}-tunnel-kind-label"))
                                    .font(label_font)
                                    .color(base),
                            )
                            .child(kind_sel.build(k, layer)),
                    )
                    // Boundary marker for the not-yet-ported tunnel field components.
                    .child(el().key(format!("{p}-inline-fields-boundary")).a11y_hidden());
                let save_as = Button::new(t("newConn.tunnelSaveAs"))
                    .variant(ButtonVariant::Neutral)
                    .control_size(ControlSize::Small)
                    .disabled(!vm.tunnel_inline_valid || vm.tunnel_busy)
                    .on_press(press(&on_action, AdvancedAction::SaveAs))
                    .build(k)
                    .key(format!("{p}-tunnel-save-as"));
                let mut actions = row().gap(8.0).flex_wrap().items_center().child(save_as);
                if let Some(banner) = Banner::new(
                    format!("{p}-tunnel-error-inline"),
                    vm.tunnel_error.clone(),
                )
                .variant(BannerVariant::Plain)
                .as_element(BannerElement::Span)
                .build(k)
                {
                    actions = actions.child(banner);
                }
                inline = inline.child(actions);
                tpanel = tpanel.child(inline);
            }

            root = root.child(tpanel);
        }

        root
    }
}
