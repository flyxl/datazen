use std::sync::{OnceLock, RwLock};

use crate::{DbIcon, IconResolverSeed, ResolvedIcon, resolve_icon};

pub const ICON_KIND_LUCIDE: &str = "lucide";
pub const ICON_KIND_URL: &str = "url";
pub const ICON_KIND_PLACEHOLDER: &str = "placeholder";

static ACTIVE_RESOLVER: OnceLock<RwLock<IconResolverSeed>> = OnceLock::new();

fn active() -> &'static RwLock<IconResolverSeed> {
    ACTIVE_RESOLVER.get_or_init(|| RwLock::new(IconResolverSeed::default()))
}

pub fn ui_placeholder() -> ResolvedIcon {
    ResolvedIcon::Placeholder {
        label: "?".to_owned(),
        bg_class: "bg-slate-600".to_owned(),
    }
}

pub fn default_placeholder_for_db(db_type: &str) -> DbIcon {
    DbIcon {
        label: db_type.chars().take(2).collect(),
        bg_class: "bg-slate-600".to_owned(),
    }
}

pub fn set_active_icon_resolver(seed: IconResolverSeed) {
    if let Ok(mut guard) = active().write() {
        *guard = seed;
    }
}

pub fn get_active_icon_resolver() -> IconResolverSeed {
    active().read().map(|guard| guard.clone()).unwrap_or_default()
}

pub fn resolve_with_active(semantic_id: &str) -> ResolvedIcon {
    match active().read() {
        Ok(guard) => resolve_icon(&guard, semantic_id),
        Err(_) => ui_placeholder(),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use crate::host_lucide_seed;

    use super::*;

    #[test]
    fn icon_kinds_are_pinned() {
        assert_eq!(ICON_KIND_LUCIDE, "lucide");
        assert_eq!(ICON_KIND_URL, "url");
        assert_eq!(ICON_KIND_PLACEHOLDER, "placeholder");
    }

    #[test]
    fn placeholder_shapes_follow_the_react_defaults() {
        assert_eq!(
            ui_placeholder(),
            ResolvedIcon::Placeholder {
                label: "?".to_owned(),
                bg_class: "bg-slate-600".to_owned(),
            }
        );
        assert_eq!(
            default_placeholder_for_db("postgresql"),
            DbIcon {
                label: "po".to_owned(),
                bg_class: "bg-slate-600".to_owned(),
            }
        );
        assert_eq!(default_placeholder_for_db("x").label, "x");
    }

    #[test]
    fn host_seed_resolves_pack_driver_lucide_and_placeholder() {
        let mut seed = host_lucide_seed();
        assert_eq!(resolve_icon(&seed, "nav.settings"), ResolvedIcon::Lucide("Settings".to_owned()));
        assert_eq!(
            resolve_icon(&seed, "missing.id"),
            ResolvedIcon::Placeholder {
                label: "?".to_owned(),
                bg_class: "bg-slate-600".to_owned(),
            }
        );

        seed.pack_icons.insert("nav.settings".to_owned(), "https://pack/x.svg".to_owned());
        assert_eq!(
            resolve_icon(&seed, "nav.settings"),
            ResolvedIcon::Url("https://pack/x.svg".to_owned())
        );

        seed.driver_icons.insert("db.mysql".to_owned(), "https://driver/my.svg".to_owned());
        assert_eq!(
            resolve_icon(&seed, "db.mysql"),
            ResolvedIcon::Url("https://driver/my.svg".to_owned())
        );

        seed.db_icons.insert(
            "redis".to_owned(),
            DbIcon { label: "Re".to_owned(), bg_class: "bg-red".to_owned() },
        );
        assert_eq!(
            resolve_icon(&seed, "db.redis"),
            ResolvedIcon::Placeholder {
                label: "Re".to_owned(),
                bg_class: "bg-red".to_owned(),
            }
        );

        seed.driver_icons = BTreeMap::new();
        seed.db_icons = BTreeMap::new();
        assert_eq!(
            resolve_icon(&seed, "db.mysql"),
            ResolvedIcon::Placeholder {
                label: "DB".to_owned(),
                bg_class: "bg-gray-500".to_owned(),
            }
        );
    }

    #[test]
    fn active_resolver_is_settable_and_defaults_empty() {
        let original = get_active_icon_resolver();
        let mut seed = IconResolverSeed::default();
        seed.lucide_by_id.insert("nav.settings".to_owned(), "Settings".to_owned());
        set_active_icon_resolver(seed);
        assert_eq!(resolve_with_active("nav.settings"), ResolvedIcon::Lucide("Settings".to_owned()));
        assert_eq!(get_active_icon_resolver().lucide_by_id.len(), 1);
        set_active_icon_resolver(original);
    }

    #[test]
    fn crate_root_surface_is_reexported() {
        let _ = crate::set_active_icon_resolver;
        let _ = crate::get_active_icon_resolver;
        let _ = crate::resolve_with_active;
        let _ = crate::ui_placeholder;
        let _ = crate::default_placeholder_for_db;
        assert_eq!(crate::ICON_KIND_LUCIDE, "lucide");
    }
}
