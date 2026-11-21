pub const THEME_PACK_STYLE_ID: &str = "datazen-theme-pack";
pub const WAPP_THEME_PACK_PREFIX: &str = "wapp:";
pub const THEME_PACK_CHANGED_EVENT: &str = "datazen:theme-pack-changed";
pub const ICON_EXTENSIONS: [&str; 3] = [".svg", ".webp", ".png"];
pub const MIME_BY_EXT: [(&str, &str); 5] = [
    (".svg", "image/svg+xml"),
    (".webp", "image/webp"),
    (".png", "image/png"),
    (".woff2", "font/woff2"),
    (".woff", "font/woff"),
];
pub const DEFAULT_MIME_TYPE: &str = "application/octet-stream";

pub fn encode_wapp_theme_pack_id(wapp_id: &str, theme_id: &str) -> String {
    format!("{WAPP_THEME_PACK_PREFIX}{wapp_id}:{theme_id}")
}

pub fn parse_wapp_theme_pack_id(pack_id: Option<&str>) -> Option<(String, String)> {
    let pack_id = pack_id?;
    let rest = pack_id.strip_prefix(WAPP_THEME_PACK_PREFIX)?;
    let separator = rest.find(':')?;
    if separator == 0 || separator == rest.len() - 1 {
        return None;
    }
    Some((
        rest[..separator].to_owned(),
        rest[separator + 1..].to_owned(),
    ))
}

pub fn mime_for_path(path: &str) -> &'static str {
    let extension = match path.rfind('.') {
        Some(index) => &path[index..],
        None => "",
    };
    let extension = extension.to_lowercase();
    for (candidate, mime) in MIME_BY_EXT.iter() {
        if *candidate == extension {
            return mime;
        }
    }
    DEFAULT_MIME_TYPE
}

pub fn join_relative_path(base_dir: &str, reference: &str) -> String {
    let prefix = if reference.starts_with('/') || base_dir.is_empty() {
        String::new()
    } else {
        format!("{base_dir}/")
    };
    let joined = format!("{prefix}{reference}");
    let mut stack: Vec<&str> = Vec::new();
    for segment in joined.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            stack.pop();
            continue;
        }
        stack.push(segment);
    }
    stack.join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_pinned() {
        assert_eq!(THEME_PACK_STYLE_ID, "datazen-theme-pack");
        assert_eq!(WAPP_THEME_PACK_PREFIX, "wapp:");
        assert_eq!(THEME_PACK_CHANGED_EVENT, "datazen:theme-pack-changed");
        assert_eq!(ICON_EXTENSIONS, [".svg", ".webp", ".png"]);
        assert_eq!(MIME_BY_EXT.len(), 5);
    }

    #[test]
    fn pack_ids_round_trip_and_reject_malformed_forms() {
        let id = encode_wapp_theme_pack_id("pub.name", "dark");
        assert_eq!(id, "wapp:pub.name:dark");
        assert_eq!(
            parse_wapp_theme_pack_id(Some(&id)),
            Some(("pub.name".to_owned(), "dark".to_owned()))
        );
        assert_eq!(parse_wapp_theme_pack_id(None), None);
        assert_eq!(parse_wapp_theme_pack_id(Some("")), None);
        assert_eq!(parse_wapp_theme_pack_id(Some("dark")), None);
        assert_eq!(parse_wapp_theme_pack_id(Some("wapp::dark")), None);
        assert_eq!(parse_wapp_theme_pack_id(Some("wapp:name:")), None);
        assert_eq!(
            parse_wapp_theme_pack_id(Some("wapp:name:dark:extra")),
            Some(("name".to_owned(), "dark:extra".to_owned()))
        );
    }

    #[test]
    fn mime_types_are_case_insensitive_and_default_to_octet_stream() {
        assert_eq!(mime_for_path("a/font.WOFF2"), "font/woff2");
        assert_eq!(mime_for_path("icons/logo.svg"), "image/svg+xml");
        assert_eq!(mime_for_path("no-extension"), "application/octet-stream");
        assert_eq!(mime_for_path("a.b/unknown.xyz"), "application/octet-stream");
    }

    #[test]
    fn relative_paths_join_and_pop_without_escaping_the_root() {
        assert_eq!(
            join_relative_path("css", "fonts/a.woff2"),
            "css/fonts/a.woff2"
        );
        assert_eq!(join_relative_path("css", "./a.png"), "css/a.png");
        assert_eq!(join_relative_path("css", "../a.png"), "a.png");
        assert_eq!(join_relative_path("", "a.png"), "a.png");
        assert_eq!(join_relative_path("css", "/a.png"), "a.png");
        assert_eq!(join_relative_path("css", "a/./b/../c.svg"), "css/a/c.svg");
    }
}
