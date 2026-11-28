//! Native saved definitions and the supported HTTP proxy editor. The Settings
//! page does not imply transport probes or connection-form binding.
use datazen_ui_api::{
    types::SavedTunnelSummary, HttpProxyScheme, HttpProxyTunnelConfig, NativeHttpProxyDefinition,
    NativeTunnelPassword, NativeTunnelStoreError, SavedTunnelKind,
};
use rune::{col, row, text, El, TextEdit, TextStyle};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use super::{settings_ui::section_title, tunnel_draft::tunnel_kind_label_key};
use crate::{
    components::connection::http_proxy_tunnel_fields::{
        HttpProxyScheme as FieldScheme, HttpProxyTunnelFields, HttpProxyValue,
    },
    i18n::t,
    types::Section,
    ui::{dialog::Dialog, field::TextField, kit::kit, label::Label},
    AppState,
};

const KEY: &str = "tunnel-edit-dialog";

#[derive(Default)]
pub(crate) struct NativeTunnels {
    rows: Vec<SavedTunnelSummary>,
    loaded: bool,
    list_error: bool,
    epoch: u64,
    form: HttpProxyEditor,
}

#[derive(Default)]
struct HttpProxyEditor {
    open: bool,
    id: Option<String>,
    name: TextEdit,
    value: HttpProxyValue,
    password_configured: bool,
    remove_password: bool,
    save_error: Option<NativeTunnelStoreError>,
}

impl HttpProxyEditor {
    fn from_saved(saved: NativeHttpProxyDefinition) -> Self {
        let proxy = saved.proxy;
        Self {
            open: true,
            id: Some(saved.id),
            name: TextEdit::new(saved.name),
            value: HttpProxyValue {
                host: TextEdit::new(proxy.host),
                port: TextEdit::new(proxy.port.to_string()),
                scheme: FieldScheme::from_value(proxy.scheme.as_str()),
                username: TextEdit::new(proxy.username.unwrap_or_default()),
                timeout: TextEdit::new(proxy.connect_timeout_secs.unwrap_or(30).to_string()),
                ..HttpProxyValue::default()
            },
            password_configured: saved.password_configured,
            ..Self::default()
        }
    }

    fn validation_message(&self) -> Option<&'static str> {
        let fields = &self.value;
        if self.name.text.trim().is_empty() {
            Some("settings.tunnels.validation.name")
        } else if fields.host.text.trim().is_empty() {
            Some("settings.tunnels.validation.httpProxyHost")
        } else if fields.port.text.trim().parse::<u16>().map_or(true, |port| port == 0) {
            Some("settings.tunnels.validation.httpProxyPort")
        } else if !fields.timeout.text.trim().is_empty()
            && fields.timeout.text.trim().parse::<u32>().map_or(true, |timeout| timeout == 0)
        {
            Some("settings.nativeTunnels.timeout")
        } else {
            None
        }
    }

    fn proxy(&self) -> Option<HttpProxyTunnelConfig> {
        let host = self.value.host.text.trim();
        let username = self.value.username.text.trim();
        let port: u16 = self.value.port.text.trim().parse().ok()?;
        let timeout = self.value.timeout.text.trim();
        let timeout: Option<u32> = if timeout.is_empty() { None } else { Some(timeout.parse().ok()?) };
        if self.name.text.trim().is_empty() || host.is_empty() || port == 0 || timeout == Some(0) {
            return None;
        }
        Some(HttpProxyTunnelConfig {
            enabled: true,
            host: host.to_owned(),
            port,
            scheme: match self.value.scheme {
                FieldScheme::Http => HttpProxyScheme::Http,
                FieldScheme::Https => HttpProxyScheme::Https,
            },
            username: (!username.is_empty()).then(|| username.to_owned()),
            password: None,
            headers: None,
            connect_timeout_secs: timeout,
        })
    }
}

impl AppState {
    pub(crate) fn refresh_native_tunnels(&mut self) {
        if !self.desktop.supports_native_saved_tunnels() {
            return;
        }
        match self.desktop.native_saved_tunnel_summaries() {
            Ok(rows) => {
                self.native_tunnels.rows = rows;
                self.native_tunnels.loaded = true;
                self.native_tunnels.list_error = false;
            }
            Err(_) => {
                self.native_tunnels.rows.clear();
                self.native_tunnels.loaded = false;
                self.native_tunnels.list_error = true;
            }
        }
    }

    pub(crate) fn retire_native_tunnel_editor(&mut self) {
        // Drop in-memory authentication input on dismiss/change of route.
        self.native_tunnels.form.value.password.replace_text(String::new());
        self.native_tunnels.form = HttpProxyEditor::default();
    }
}

fn is_current(state: &AppState) -> bool {
    state.active == Section::Settings && state.settings_section() == "tunnels"
        && state.desktop.supports_native_saved_tunnels()
}

fn open(state: &mut AppState, id: Option<&str>) {
    if !is_current(state) || !state.native_tunnels.loaded {
        return;
    }
    state.native_tunnels.form = match id {
        None => HttpProxyEditor { open: true, ..HttpProxyEditor::default() },
        Some(id) => match state.desktop.native_http_proxy_definition(id) {
            Ok(record) => HttpProxyEditor::from_saved(record),
            Err(_) => {
                state.refresh_native_tunnels();
                state.native_tunnels.list_error = true;
                return;
            }
        },
    };
    state.native_tunnels.epoch = state.native_tunnels.epoch.wrapping_add(1);
}

fn save(state: &mut AppState) {
    if !is_current(state) || !state.native_tunnels.form.open {
        return;
    }
    let form = &state.native_tunnels.form;
    let Some(proxy) = form.proxy() else { return };
    let password = if !form.value.password.text.is_empty() {
        NativeTunnelPassword::Replace(form.value.password.text.clone())
    } else if form.remove_password {
        NativeTunnelPassword::Clear
    } else {
        NativeTunnelPassword::Keep
    };
    match state.desktop.save_native_http_proxy_definition(
        form.id.as_deref(), &form.name.text, proxy, password,
    ) {
        Ok(()) => {
            state.retire_native_tunnel_editor();
            state.refresh_native_tunnels();
        }
        Err(error) => state.native_tunnels.form.save_error = Some(error),
    }
}

fn dialog(state: &AppState, dark: bool, k: &Kit, layer: &mut Vec<El>) {
    let form = &state.native_tunnels.form;
    if !form.open {
        return;
    }
    let mut field_overlays = Vec::new();
    let fields = HttpProxyTunnelFields::new(
        &form.value,
        |s: &mut AppState| &mut s.native_tunnels.form.value.host,
        |s: &mut AppState| &mut s.native_tunnels.form.value.port,
        |s: &mut AppState| &mut s.native_tunnels.form.value.username,
        |s: &mut AppState| &mut s.native_tunnels.form.value.password,
        |s: &mut AppState| &mut s.native_tunnels.form.value.timeout,
    )
    .scheme_open(|s| &mut s.native_tunnels.form.value.scheme_open)
    .on_scheme(|s, value| s.native_tunnels.form.value.scheme = value)
    .build(dark, &mut field_overlays);
    let title = if form.id.is_some() { "settings.tunnels.editor.editTitle" }
        else { "settings.tunnels.editor.createTitle" };
    let name_label = t("settings.tunnels.editor.name");
    let name = col().key("tunnel-edit-name-block").gap(4.0)
        .child(Label::new("tunnel-edit-name-label", name_label).required(true).build(dark))
        .child(TextField::new("tunnel-edit-name", &form.name)
            .height(32.0).label(name_label)
            .placeholder(t("settings.tunnels.editor.namePlaceholder"))
            .on_edit::<AppState>(rune::edit_access(|s: &mut AppState| &mut s.native_tunnels.form.name))
            .build(dark));
    // Only HTTP proxy definitions are editable here. Show the fixed value
    // as text, never an unavailable kind picker or a faux form control.
    let kind = col().key("tunnel-edit-kind-block").gap(4.0)
        .child(Label::new("tunnel-edit-kind-label", t("settings.tunnels.editor.kind")).build(dark))
        .child(row().w_full().h(32.0).center_y().child(
            text(t(tunnel_kind_label_key(SavedTunnelKind::HttpProxy)))
                .key("tunnel-edit-kind-readable")
                .font(TextStyle::new(14.0, 400.0).lh(20.0)).color(k.c.text_base),
        ));

    // The original grid uses the viewport's md (768px) breakpoint. Track
    // just that threshold on the full-window dialog layer; its settled
    // width is independent of the form height or scroll position.
    let wide = rune::memo::layout_with("tunnel-edit-dialog-layer", |rect| rect[2] >= 768.0)
        .unwrap_or(false);
    let identity = if wide {
        row().items_start()
            .child(name.min_w(0.0).grow(1.0))
            .child(kind.min_w(0.0).grow(1.0))
    } else {
        col().child(name.w_full()).child(kind.w_full())
    };
    let mut body = col().key("tunnel-edit-content").w_full().gap(12.0)
        .child(identity.key("tunnel-edit-identity").w_full().gap(12.0))
        .child(fields);
    if form.password_configured {
        let (hint, action) = if form.remove_password {
            ("settings.nativeTunnels.clearingPassword", "settings.nativeTunnels.keepPassword")
        } else {
            ("settings.nativeTunnels.savedPassword", "settings.nativeTunnels.clearPassword")
        };
        body = body.child(text(t(hint)).key("tunnel-password-state").wrap()
            .font(k.ty.text_xsmall).color(k.c.text_muted))
            .child(row().child(Button::new(t(action)).variant(ButtonVariant::Outline)
                .control_size(ControlSize::Small).on_press(|s: &mut AppState| {
                    let form = &mut s.native_tunnels.form;
                    form.remove_password = !form.remove_password;
                    if form.remove_password {
                        form.value.password.replace_text(String::new());
                    }
                }).build(k).key("tunnel-clear-password")));
    }
    if !form.name.text.is_empty() || !form.value.host.text.is_empty() || form.value.port.text != "8080" {
        if let Some(message) = form.validation_message() {
            body = body.child(text(t(message)).key("tunnel-edit-validation").role("status").wrap()
                .font(k.ty.text_xsmall).color(k.c.text_muted));
        }
    }
    if let Some(error) = form.save_error {
        let key = match error {
            NativeTunnelStoreError::Credential => "settings.nativeTunnels.keyUnavailable",
            _ => "settings.tunnels.editor.saveFailed",
        };
        body = body.child(text(t(key)).key("tunnel-edit-error").role("status").wrap()
            .font(k.ty.text_xsmall).color(k.c.danger.fg));
    }

    Dialog::new(KEY, t(title))
        .open(true).open_epoch(state.native_tunnels.epoch).width(540.0).fill_height().max_height(680.0)
        .description(t("settings.tunnels.editor.description"))
        .on_close(AppState::retire_native_tunnel_editor)
        .child(body)
        .footer(row().gap(8.0).justify_end()
            .child(Button::new(t("common.cancel")).variant(ButtonVariant::Outline)
                .control_size(ControlSize::Small).on_press(AppState::retire_native_tunnel_editor)
                .build(k).key("tunnel-edit-cancel"))
            .child(Button::new(t("common.save")).control_size(ControlSize::Small)
                .disabled(form.proxy().is_none()).on_press(save).build(k).key("tunnel-edit-save")))
        .build(k, layer);
    layer.extend(field_overlays);
}

pub(super) fn view(state: &AppState, dark: bool, layer: &mut Vec<El>) -> El {
    let k = kit(dark);
    let info = &state.native_tunnels;
    let mut heading = row().w_full().gap(16.0).justify_between().items_start()
        .child(section_title(k, layer, "tunnels-title".to_owned(),
                             t("settings.tunnels.title"), Some(t("settings.tunnels.description")))
            .grow(1.0).min_w(0.0));
    if info.loaded && !info.list_error {
        heading = heading.child(Button::new(t("settings.tunnels.create"))
            .variant(ButtonVariant::Outline).control_size(ControlSize::Small)
            .on_press(|s: &mut AppState| open(s, None)).build(k).key("tunnel-create")
            .h(28.0).px(9.0).radius(9.0));
    }
    let scope = text(t("settings.nativeTunnels.scope")).key("tunnel-supported-scope").wrap()
        .font(TextStyle::new(12.0, 400.0).lh(16.0)).color(k.c.text_faint);
    let mut panel = col().key("settings-tunnels-section").w_full().gap(16.0).child(heading);
    if info.list_error {
        panel = panel.child(row().key("tunnel-load-error").w_full().gap(8.0).center_y()
            .child(text(t("settings.nativeTunnels.loadFailed")).role("status").font(k.ty.text_xsmall)
                .color(k.c.danger.fg).wrap().grow(1.0))
            .child(Button::new(t("common.retry")).variant(ButtonVariant::Ghost)
                .control_size(ControlSize::Small).on_press(AppState::refresh_native_tunnels)
                .build(k).key("tunnel-retry"))).child(scope);
    } else if info.loaded && info.rows.is_empty() {
        panel = panel.child(col().key("tunnel-list-empty").w_full().gap(8.0).padding(17.0)
            .radius(6.0).border(1.0, k.c.border_muted)
            .child(text(t("settings.tunnels.empty"))
                .font(TextStyle::new(14.0, 400.0).lh(20.0)).color(k.c.text_base))
            .child(scope));
    } else if info.loaded {
        let mut list = col().key("tunnel-list").w_full().gap(8.0);
        for summary in &info.rows {
            let id = summary.id.clone();
            let mut item = row().key(format!("tunnel-row-{}", id)).w_full().gap(12.0).center_y()
                .px(13.0).py(9.0).radius(6.0).border(1.0, k.c.border_base).fill(k.c.surface_base)
                .child(col().gap(2.0).grow(1.0).min_w(0.0)
                    .child(text(summary.name.clone()).key(format!("tunnel-row-{}-name", id))
                        .font(TextStyle::new(14.0, 400.0).lh(20.0)).color(k.c.text_base))
                    .child(row().child(row().px(6.0).py(2.0).radius(4.0)
                        .fill(k.alpha(k.c.text_accent, 0.2))
                        .child(text(t(tunnel_kind_label_key(summary.kind))).key(format!("tunnel-row-{}-kind", id))
                            .font(TextStyle::new(11.0, 400.0).lh(16.5)).color(k.c.text_accent)))));
            if summary.kind == SavedTunnelKind::HttpProxy {
                let button_key = format!("tunnel-edit-{}", id);
                item = item.child(Button::new(t("settings.tunnels.edit")).variant(ButtonVariant::Ghost)
                    .control_size(ControlSize::Small).on_press(move |s: &mut AppState| open(s, Some(&id)))
                    .build(k).key(button_key));
            }
            list = list.child(item);
        }
        panel = panel.child(list).child(scope);
    }
    // Settings content scrolls; the bounded dialog and its dropdowns belong
    // to the shared window overlay, not to this short section panel.
    dialog(state, dark, k, layer);
    col().key("settings-section-tunnels").w_full().child(panel)
}
