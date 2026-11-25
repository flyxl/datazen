//! Redis-only controls inside the native connection dialog.
use super::{field_label, form_input, AppState, El};
use datazen_ui_api::native_kv::RedisTopology;
use rune::{
    col, row, text,
    tok::{self, type_},
    TextEdit,
};

pub(super) fn view(state: &AppState, theme: tok::Theme, dark: bool) -> El {
    if state.connection_engine.text.trim() != "redis" {
        return col();
    }
    let option = |key: &str, title: String| {
        row()
            .key(key.to_owned())
            .h(28.0)
            .px(8.0)
            .center_y()
            .radius(6.0)
            .fill(theme.control_off)
            .label(&title)
            .role("button")
            .focusable(true)
            .child(text(title).font(type_::LABEL).color(theme.text))
    };
    let input =
        |key: &str, label: &str, field: &TextEdit, edit: fn(&mut AppState) -> &mut TextEdit| {
            col()
                .gap(4.0)
                .child(field_label(label, theme))
                .child(form_input(key, field, theme, dark, edit))
        };
    let settings = &state.redis_form_options;
    let mut section = col().key("native-redis-connection-options").gap(7.0).child(
        row()
            .gap(8.0)
            .flex_wrap()
            .child(
                option(
                    "native-redis-topology",
                    format!("Topology: {}", settings.topology.label()),
                )
                .on_press(|s: &mut AppState| {
                    s.redis_form_options.topology = s.redis_form_options.topology.next()
                }),
            )
            .child(
                option(
                    "native-redis-tls",
                    format!("TLS: {}", if settings.tls { "on" } else { "off" }),
                )
                .on_press(|s: &mut AppState| s.redis_form_options.tls = !s.redis_form_options.tls),
            )
            .child(
                option(
                    "native-redis-read-only",
                    format!(
                        "Read only: {}",
                        if settings.read_only { "on" } else { "off" }
                    ),
                )
                .on_press(|s: &mut AppState| {
                    s.redis_form_options.read_only = !s.redis_form_options.read_only
                }),
            ),
    );
    if settings.topology == RedisTopology::Sentinel {
        section = section.child(input(
            "native-redis-sentinel-master",
            "Sentinel master name",
            &state.redis_form_master,
            |s| &mut s.redis_form_master,
        ));
    }
    if settings.topology != RedisTopology::Standalone {
        section = section.child(input(
            "native-redis-nodes",
            "Optional node addresses (host:port, comma-separated)",
            &state.redis_form_nodes,
            |s| &mut s.redis_form_nodes,
        ));
    }
    if settings.tls {
        section = section
            .child(input(
                "native-redis-ca",
                "CA certificate (optional PEM path)",
                &state.redis_form_ca,
                |s| &mut s.redis_form_ca,
            ))
            .child(input(
                "native-redis-client-cert",
                "Client certificate (optional PEM path)",
                &state.redis_form_cert,
                |s| &mut s.redis_form_cert,
            ))
            .child(input(
                "native-redis-client-key",
                "Client private key (optional PEM path)",
                &state.redis_form_key,
                |s| &mut s.redis_form_key,
            ));
    }
    let status = if state.redis_probe_pending {
        Some("Testing Redis…".to_string())
    } else {
        state
            .redis_probe_result
            .as_ref()
            .map(|result| match result {
                Ok(()) => "Redis connection succeeded for the tested settings.".into(),
                Err(error) => error.clone(),
            })
    };
    if let Some(status) = status {
        section = section.child(text(status).key("native-redis-probe-status").wrap());
    }
    section
}
