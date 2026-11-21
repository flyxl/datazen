#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::RwLock;

pub const WAPP_ASSET_PATH_PREFIX: &str = "/wapps/";
pub const WAPP_ASSET_CSP: &str = "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'none'";

pub type WappAssetQueryMap = BTreeMap<String, String>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WappAssetTarget {
    pub wapp_id: String,
    pub path: String,
    pub query: WappAssetQueryMap,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WappAssetResolver {
    pub build_src: fn(&str, &str, &str) -> String,
    pub build_icon_src: fn(&str, &str) -> String,
}

fn strip_leading_dot_slash(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

fn is_uri_component_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')')
}

fn encode_version(version: &str) -> String {
    let mut out = String::new();
    for byte in version.as_bytes() {
        if is_uri_component_byte(*byte) {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

fn hex_value(unit: u16) -> Option<u8> {
    match unit {
        48..=57 => Some((unit - 48) as u8),
        65..=70 => Some((unit - 65 + 10) as u8),
        97..=102 => Some((unit - 97 + 10) as u8),
        _ => None,
    }
}

fn percent_decode(input: &str) -> String {
    if !input.contains('%') {
        return input.to_owned();
    }
    let units: Vec<u16> = input.encode_utf16().collect();
    let mut bytes = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        if units[i] == b'%' as u16 && i + 2 < units.len() {
            if let (Some(high), Some(low)) = (hex_value(units[i + 1]), hex_value(units[i + 2])) {
                bytes.push((high << 4) | low);
                i += 3;
                continue;
            }
        }
        bytes.push(units[i] as u8);
        i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

pub fn is_valid_wapp_id(id: &str) -> bool {
    let mut parts = id.split('.');
    let publisher = parts.next().unwrap_or("");
    let name = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return false;
    }
    let publisher_bytes = publisher.as_bytes();
    if publisher_bytes.is_empty()
        || publisher_bytes.len() > 31
        || !(publisher_bytes[0].is_ascii_lowercase() || publisher_bytes[0].is_ascii_digit())
    {
        return false;
    }
    if !publisher_bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
    {
        return false;
    }
    let name_bytes = name.as_bytes();
    if !(2..=32).contains(&name_bytes.len()) || !name_bytes[0].is_ascii_lowercase() {
        return false;
    }
    name_bytes[1..]
        .iter()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || *byte == b'-')
}

pub fn is_safe_wapp_asset_path(path: &str) -> bool {
    if path.is_empty() || path.contains('\\') || path.contains('\0') {
        return false;
    }
    path.split('/')
        .all(|component| !component.is_empty() && !component.starts_with('.'))
}

fn parse_query(raw: Option<&str>) -> WappAssetQueryMap {
    let mut query = WappAssetQueryMap::new();
    let Some(raw) = raw.filter(|raw| !raw.is_empty()) else {
        return query;
    };
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.find('=') {
            Some(index) => (&pair[..index], &pair[index + 1..]),
            None => (pair, ""),
        };
        let decoded_key = percent_decode(&key.replace('+', " "));
        let decoded_value = percent_decode(&value.replace('+', " "));
        query.insert(decoded_key, decoded_value);
    }
    query
}

fn has_url_scheme(input: &str) -> bool {
    let Some(index) = input.find("://") else {
        return false;
    };
    if index == 0 || !input.as_bytes()[0].is_ascii_alphabetic() {
        return false;
    }
    input.as_bytes()[..index]
        .iter()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'.' | b'-'))
}

fn without_origin(input: &str) -> &str {
    if !has_url_scheme(input) {
        return input;
    }
    let after_scheme = match input.find("://") {
        Some(index) => &input[index + 3..],
        None => return input,
    };
    match after_scheme.find('/') {
        Some(index) => &after_scheme[index..],
        None => "",
    }
}

pub fn parse_wapp_asset_target(url: &str) -> Result<WappAssetTarget, String> {
    let candidate = without_origin(url);
    let mut parts = candidate.split('?');
    let path_and_file = parts.next().unwrap_or("");
    let raw_query = parts.next();
    if !path_and_file.starts_with(WAPP_ASSET_PATH_PREFIX) {
        return Err(format!("unsupported wapp asset path: {path_and_file}"));
    }
    let rest = &path_and_file[WAPP_ASSET_PATH_PREFIX.len()..];
    let (wapp_id, raw_path) = match rest.find('/') {
        Some(index) => (&rest[..index], &rest[index + 1..]),
        None => (rest, ""),
    };
    if !is_valid_wapp_id(wapp_id) {
        return Err(format!("invalid wapp host: `{wapp_id}`"));
    }
    let path = percent_decode(raw_path);
    if !is_safe_wapp_asset_path(&path) {
        return Err(format!("unsafe path component in `{path}`"));
    }
    Ok(WappAssetTarget {
        wapp_id: wapp_id.to_owned(),
        path,
        query: parse_query(raw_query),
    })
}

fn desktop_build_src(target_id: &str, entry: &str, version: &str) -> String {
    format!(
        "datazen://{}/{}?v={}",
        target_id,
        strip_leading_dot_slash(entry),
        encode_version(version)
    )
}

fn desktop_build_icon_src(target_id: &str, icon: &str) -> String {
    format!("datazen://{target_id}/{}", strip_leading_dot_slash(icon))
}

fn web_build_src(target_id: &str, entry: &str, version: &str) -> String {
    format!(
        "{}{}/{}?v={}",
        WAPP_ASSET_PATH_PREFIX,
        target_id,
        strip_leading_dot_slash(entry),
        encode_version(version)
    )
}

fn web_build_icon_src(target_id: &str, icon: &str) -> String {
    format!(
        "{}{}/{}",
        WAPP_ASSET_PATH_PREFIX,
        target_id,
        strip_leading_dot_slash(icon)
    )
}

pub const DESKTOP_WAPP_ASSET_RESOLVER: WappAssetResolver = WappAssetResolver {
    build_src: desktop_build_src,
    build_icon_src: desktop_build_icon_src,
};

pub const WEB_WAPP_ASSET_RESOLVER: WappAssetResolver = WappAssetResolver {
    build_src: web_build_src,
    build_icon_src: web_build_icon_src,
};

static RESOLVER: RwLock<WappAssetResolver> = RwLock::new(DESKTOP_WAPP_ASSET_RESOLVER);

pub fn configure_wapp_asset_resolver(next: WappAssetResolver) {
    let mut resolver = RESOLVER
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *resolver = next;
}

pub fn reset_wapp_asset_resolver() {
    configure_wapp_asset_resolver(DESKTOP_WAPP_ASSET_RESOLVER);
}

pub fn build_wapp_asset_src(target_id: &str, entry: &str, version: &str) -> String {
    match RESOLVER.read() {
        Ok(resolver) => (resolver.build_src)(target_id, entry, version),
        Err(poisoned) => {
            let resolver = poisoned.into_inner();
            (resolver.build_src)(target_id, entry, version)
        }
    }
}

pub fn build_wapp_icon_src(target_id: &str, icon: &str) -> String {
    match RESOLVER.read() {
        Ok(resolver) => (resolver.build_icon_src)(target_id, icon),
        Err(poisoned) => {
            let resolver = poisoned.into_inner();
            (resolver.build_icon_src)(target_id, icon)
        }
    }
}
