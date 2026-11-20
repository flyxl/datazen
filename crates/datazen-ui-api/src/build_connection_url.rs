use crate::connection_types::{ConnectionConfig, SslMode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConnectionMode {
    Server,
    File,
    Url,
}

impl ConnectionMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::File => "file",
            Self::Url => "url",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionUrlDriverMeta {
    pub clipboard_schemes: Vec<String>,
    pub connection_mode: ConnectionMode,
    pub default_port: Option<u16>,
}

fn encode_uri_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let current = *byte;
        if current.is_ascii_alphanumeric()
            || matches!(current, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')')
        {
            out.push(current as char);
        } else {
            out.push('%');
            out.push_str(&format!("{current:02X}"));
        }
    }
    out
}

fn encode_form_component(value: &str) -> String {
    let mut out = String::new();
    for byte in value.as_bytes() {
        let current = *byte;
        match current {
            b' ' => out.push('+'),
            current if current.is_ascii_alphanumeric()
                || matches!(current, b'*' | b'-' | b'.' | b'_') =>
            {
                out.push(current as char)
            }
            current => {
                out.push('%');
                out.push_str(&format!("{current:02X}"));
            }
        }
    }
    out
}

pub fn build_connection_url(
    config: &ConnectionConfig,
    meta: Option<&ConnectionUrlDriverMeta>,
) -> Option<String> {
    let meta = meta?;
    let scheme = meta.clipboard_schemes.first()?;
    if scheme.is_empty() {
        return None;
    }
    if meta.connection_mode == ConnectionMode::File {
        let path = config.database.as_deref().unwrap_or("");
        if path.is_empty() {
            return None;
        }
        return Some(format!("{scheme}://{path}"));
    }

    let host = match config.host.as_deref() {
        Some(host) if !host.is_empty() => host,
        _ => "localhost",
    };
    let port = config.port.or(meta.default_port);

    let mut user_info = String::new();
    match config.username.as_deref().filter(|name| !name.is_empty()) {
        Some(username) => {
            user_info.push_str(&encode_uri_component(username));
            if let Some(password) = config.password.as_deref().filter(|value| !value.is_empty()) {
                user_info.push(':');
                user_info.push_str(&encode_uri_component(password));
            }
            user_info.push('@');
        }
        None => {
            if let Some(password) = config.password.as_deref().filter(|value| !value.is_empty()) {
                user_info.push(':');
                user_info.push_str(&encode_uri_component(password));
                user_info.push('@');
            }
        }
    }

    let host_port = match port {
        Some(port) if port != 0 && Some(port) != meta.default_port => format!("{host}:{port}"),
        _ => host.to_owned(),
    };
    let path = match config.database.as_deref().filter(|name| !name.is_empty()) {
        Some(database) => format!("/{}", encode_uri_component(database)),
        None => String::new(),
    };

    let mut query = String::new();
    if config.ssl_mode != SslMode::Disable {
        query.push_str("sslmode=");
        query.push_str(&encode_form_component(config.ssl_mode.as_str()));
    }
    let suffix = if query.is_empty() { String::new() } else { format!("?{query}") };

    Some(format!("{scheme}://{user_info}{host_port}{path}{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn config(value: Value) -> ConnectionConfig {
        serde_json::from_value(value).unwrap_or_else(|error| panic!("config decode failed: {error}"))
    }

    fn base() -> ConnectionConfig {
        config(json!({
            "id": "c1",
            "name": "test",
            "databaseType": "mysql",
            "sslMode": "disable"
        }))
    }

    fn meta() -> ConnectionUrlDriverMeta {
        ConnectionUrlDriverMeta {
            clipboard_schemes: vec!["mysql".to_owned(), "mysql2".to_owned()],
            connection_mode: ConnectionMode::Server,
            default_port: Some(3306),
        }
    }

    #[test]
    fn an_unknown_or_missing_driver_has_no_url() {
        assert_eq!(build_connection_url(&base(), None), None);
        let mut no_scheme = meta();
        no_scheme.clipboard_schemes.clear();
        assert_eq!(build_connection_url(&base(), Some(&no_scheme)), None);
        let mut empty_scheme = meta();
        empty_scheme.clipboard_schemes.insert(0, String::new());
        assert_eq!(build_connection_url(&base(), Some(&empty_scheme)), None);
    }

    #[test]
    fn the_first_clipboard_scheme_is_used() {
        assert_eq!(
            build_connection_url(&base(), Some(&meta())).as_deref(),
            Some("mysql://localhost")
        );
    }

    #[test]
    fn file_mode_uses_the_database_as_an_unencoded_path() {
        let mut file_meta = meta();
        file_meta.clipboard_schemes = vec!["sqlite".to_owned()];
        file_meta.connection_mode = ConnectionMode::File;
        let mut cfg = base();
        cfg.database = Some("/tmp/a b/db.sqlite?x=1".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&file_meta)).as_deref(),
            Some("sqlite:///tmp/a b/db.sqlite?x=1")
        );
        assert_eq!(build_connection_url(&base(), Some(&file_meta)), None);
        cfg.database = Some(String::new());
        assert_eq!(build_connection_url(&cfg, Some(&file_meta)), None);
    }

    #[test]
    fn a_missing_or_empty_host_defaults_to_localhost() {
        let mut cfg = base();
        cfg.host = Some(String::new());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://localhost")
        );
        cfg.host = Some("db.internal".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://db.internal")
        );
    }

    #[test]
    fn a_default_port_or_zero_is_omitted_from_the_authority() {
        let mut cfg = base();
        cfg.port = Some(3306);
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://localhost")
        );
        cfg.port = Some(0);
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://localhost")
        );
    }

    #[test]
    fn a_non_default_port_is_appended_to_the_host() {
        let mut cfg = base();
        cfg.port = Some(3307);
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://localhost:3307")
        );
        let mut no_default = meta();
        no_default.default_port = None;
        cfg.port = Some(5432);
        assert_eq!(
            build_connection_url(&cfg, Some(&no_default)).as_deref(),
            Some("mysql://localhost:5432")
        );
    }

    #[test]
    fn user_info_covers_username_only_password_only_and_both() {
        let mut cfg = base();
        cfg.username = Some("alice".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://alice@localhost")
        );
        cfg.username = None;
        cfg.password = Some("secret".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://:secret@localhost")
        );
        cfg.username = Some("alice".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://alice:secret@localhost")
        );
    }

    #[test]
    fn empty_credentials_are_treated_as_absent() {
        let mut cfg = base();
        cfg.username = Some(String::new());
        cfg.password = Some(String::new());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://localhost")
        );
    }

    #[test]
    fn uri_components_are_encoded_but_the_file_path_is_not() {
        let mut cfg = base();
        cfg.username = Some("a b@c".to_owned());
        cfg.password = Some("p&q/r".to_owned());
        cfg.database = Some("my db/x".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://a%20b%40c:p%26q%2Fr@localhost/my%20db%2Fx")
        );
        let mut file_meta = meta();
        file_meta.connection_mode = ConnectionMode::File;
        cfg.database = Some("my db/x".to_owned());
        assert_eq!(
            build_connection_url(&cfg, Some(&file_meta)).as_deref(),
            Some("mysql://my db/x")
        );
    }

    #[test]
    fn only_non_disable_ssl_modes_add_a_query_parameter() {
        let mut cfg = base();
        assert_eq!(
            build_connection_url(&cfg, Some(&meta())).as_deref(),
            Some("mysql://localhost")
        );
        for (mode, expected) in [
            (SslMode::Prefer, "mysql://localhost?sslmode=prefer"),
            (SslMode::Require, "mysql://localhost?sslmode=require"),
            (SslMode::VerifyCa, "mysql://localhost?sslmode=verifyCa"),
            (SslMode::VerifyFull, "mysql://localhost?sslmode=verifyFull"),
        ] {
            cfg.ssl_mode = mode;
            assert_eq!(build_connection_url(&cfg, Some(&meta())).as_deref(), Some(expected));
        }
    }

    #[test]
    fn url_mode_uses_the_same_server_shape() {
        let mut url_meta = meta();
        url_meta.clipboard_schemes = vec!["redis".to_owned()];
        url_meta.connection_mode = ConnectionMode::Url;
        let mut cfg = base();
        cfg.host = Some("cache".to_owned());
        cfg.port = Some(6380);
        assert_eq!(
            build_connection_url(&cfg, Some(&url_meta)).as_deref(),
            Some("redis://cache:6380")
        );
    }

    #[test]
    fn connection_mode_as_str_pins_the_react_literals() {
        assert_eq!(ConnectionMode::Server.as_str(), "server");
        assert_eq!(ConnectionMode::File.as_str(), "file");
        assert_eq!(ConnectionMode::Url.as_str(), "url");
    }
}
