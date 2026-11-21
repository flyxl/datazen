#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImportPasswordPolicy {
    Required,
    Optional,
}

impl ImportPasswordPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            ImportPasswordPolicy::Required => "required",
            ImportPasswordPolicy::Optional => "optional",
        }
    }
}

fn basename(path: &str) -> &str {
    path.split(|ch| ch == '/' || ch == '\\').last().unwrap_or(path)
}

pub fn import_file_password_policy(path: &str) -> ImportPasswordPolicy {
    let base = basename(path);
    let extension = match base.rfind('.') {
        Some(dot) => base[dot + 1..].to_lowercase(),
        None => String::new(),
    };
    if matches!(
        extension.as_str(),
        "datazenconnection" | "datazenconnections" | "tableplusconnection"
    ) {
        ImportPasswordPolicy::Required
    } else {
        ImportPasswordPolicy::Optional
    }
}

pub fn import_file_display_name(path: &str) -> String {
    basename(path).to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_connection_extensions_require_a_password() {
        for path in [
            "conn.datazenconnection",
            "conn.DATAZENCONNECTION",
            "/tmp/conn.datazenconnections",
            "C:\\exports\\conn.tableplusconnection",
            "a.datazenconnection",
        ] {
            assert_eq!(import_file_password_policy(path), ImportPasswordPolicy::Required, "{path}");
        }
    }

    #[test]
    fn other_extensions_are_optional() {
        for path in [
            "conn.json",
            "conn.datazenconnection.bak",
            "datazenconnection",
            "",
            "/tmp/noext",
            "C:\\dir\\conn.sql",
            "conn.datazenconnection ",
        ] {
            assert_eq!(import_file_password_policy(path), ImportPasswordPolicy::Optional, "{path}");
        }
    }

    #[test]
    fn display_name_uses_either_separator_and_keeps_the_whole_leaf() {
        assert_eq!(import_file_display_name("/a/b/conn.json"), "conn.json");
        assert_eq!(import_file_display_name("C:\\a\\b\\conn.json"), "conn.json");
        assert_eq!(import_file_display_name("conn.json"), "conn.json");
        assert_eq!(import_file_display_name(""), "");
        assert_eq!(import_file_display_name("/a/b/"), "");
    }

    #[test]
    fn policy_and_display_name_never_emit_credentials_or_full_paths() {
        let path = "/secret/user:password@host/db/conn.datazenconnection";
        assert_eq!(import_file_password_policy(path), ImportPasswordPolicy::Required);
        assert_eq!(import_file_display_name(path), "conn.datazenconnection");
        assert!(!import_file_display_name(path).contains("password"));
        assert_eq!(import_file_password_policy(path).as_str(), "required");
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::import_file_password_policy;
        let _ = crate::import_file_display_name;
        let _: Option<crate::ImportPasswordPolicy> = None;
    }
}
