#[cfg(test)]
mod tests;

use std::sync::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowKind {
    Main,
    Onboarding,
    DataSync,
    DataTransfer,
    SchemaDiff,
    Backup,
}

impl WindowKind {
    pub fn as_str(self) -> &'static str {
        match self {
            WindowKind::Main => "main",
            WindowKind::Onboarding => "onboarding",
            WindowKind::DataSync => "data-sync",
            WindowKind::DataTransfer => "data-transfer",
            WindowKind::SchemaDiff => "schema-diff",
            WindowKind::Backup => "backup",
        }
    }
}

pub const LEGACY_MAIN_ALIASES: [&str; 6] = [
    "connection",
    "workflow",
    "dashboard",
    "settings",
    "docs",
    "new-connection",
];

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn decode_component(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[i + 1]), hex_value(bytes[i + 2]))
                {
                    out.push((high << 4) | low);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn first_param(name: &str, search: &str) -> Option<String> {
    let raw = search.strip_prefix('?').unwrap_or(search);
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.find('=') {
            Some(index) => (&pair[..index], &pair[index + 1..]),
            None => (pair, ""),
        };
        if decode_component(key) == name {
            return Some(decode_component(value));
        }
    }
    None
}

pub fn get_url_param(name: &str, search: &str) -> Option<String> {
    first_param(name, search)
}

pub fn resolve_window_kind(search: &str) -> WindowKind {
    let value = first_param("window", search);
    match value.as_deref() {
        Some("onboarding") => WindowKind::Onboarding,
        Some("data-sync") => WindowKind::DataSync,
        Some("data-transfer") => WindowKind::DataTransfer,
        Some("schema-diff") => WindowKind::SchemaDiff,
        Some("backup") => WindowKind::Backup,
        Some(legacy) if LEGACY_MAIN_ALIASES.contains(&legacy) => WindowKind::Main,
        _ => WindowKind::Main,
    }
}

static CACHED_KIND: RwLock<Option<WindowKind>> = RwLock::new(None);

pub fn get_window_kind(search: &str) -> WindowKind {
    let current = CACHED_KIND
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(kind) = *current {
        return kind;
    }
    drop(current);
    let kind = resolve_window_kind(search);
    let mut cached = CACHED_KIND
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(kind) = *cached {
        return kind;
    }
    *cached = Some(kind);
    kind
}

pub fn reset_window_kind_cache() {
    let mut cached = CACHED_KIND
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    *cached = None;
}
