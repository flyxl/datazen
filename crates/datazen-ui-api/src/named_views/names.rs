use super::types::{NamedView, NamedViewScope};

pub fn normalize_view_name(name: &str) -> String {
    name.trim().split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn normalized_name_key(name: &str) -> String {
    normalize_view_name(name).to_lowercase()
}

pub fn view_names_equal(left: &str, right: &str) -> bool {
    normalized_name_key(left) == normalized_name_key(right)
}

fn tagged_part(value: Option<&str>) -> String {
    match value {
        None => "n:".to_owned(),
        Some(text) => format!("s:{text}"),
    }
}

pub fn named_view_scope_key(scope: &NamedViewScope) -> String {
    format!(
        "c:{}|d:{}|s:{}|t:{}",
        scope.connection_id,
        tagged_part(scope.database.as_deref()),
        tagged_part(scope.schema.as_deref()),
        scope.table
    )
}

fn contains_name(taken: &std::collections::HashSet<String>, name: &str) -> bool {
    let key = normalized_name_key(name);
    taken
        .iter()
        .any(|existing| normalized_name_key(existing) == key)
}

pub fn resolve_import_name(name: &str, taken: &std::collections::HashSet<String>) -> String {
    let base = normalize_view_name(name);
    if base.is_empty() || !contains_name(taken, &base) {
        return base;
    }
    for suffix in 2..100_000u32 {
        let candidate = format!("{base} ({suffix})");
        if !contains_name(taken, &candidate) {
            return candidate;
        }
    }
    format!("{base} (imported)")
}

pub fn views_for_scope(views: &[NamedView], scope: &NamedViewScope) -> Vec<NamedView> {
    let key = named_view_scope_key(scope);
    views
        .iter()
        .filter(|view| named_view_scope_key(&view.scope) == key)
        .cloned()
        .collect()
}

pub fn sort_named_views(views: &[NamedView]) -> Vec<NamedView> {
    let mut sorted = views.to_vec();
    sorted.sort_by(|left, right| {
        if left.is_default != right.is_default {
            return if left.is_default {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        let by_name = normalized_name_key(&left.name).cmp(&normalized_name_key(&right.name));
        if by_name != std::cmp::Ordering::Equal {
            return by_name;
        }
        let by_raw = left.name.cmp(&right.name);
        if by_raw != std::cmp::Ordering::Equal {
            return by_raw;
        }
        left.id.cmp(&right.id)
    });
    sorted
}
