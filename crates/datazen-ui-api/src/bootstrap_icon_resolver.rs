use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DbIcon {
    pub label: String,
    pub bg_class: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IconResolverSeed {
    pub pack_icons: BTreeMap<String, String>,
    pub driver_icons: BTreeMap<String, String>,
    pub lucide_by_id: BTreeMap<String, String>,
    pub db_icons: BTreeMap<String, DbIcon>,
}

pub fn default_icon_resolver_seed(
    driver_icons: BTreeMap<String, String>,
    lucide_by_id: BTreeMap<String, String>,
    db_icons: BTreeMap<String, DbIcon>,
) -> IconResolverSeed {
    IconResolverSeed {
        pack_icons: BTreeMap::new(),
        driver_icons,
        lucide_by_id,
        db_icons,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedIcon {
    Url(String),
    Placeholder { label: String, bg_class: String },
    Lucide(String),
}

pub fn resolve_icon(seed: &IconResolverSeed, semantic_id: &str) -> ResolvedIcon {
    if let Some(url) = seed.pack_icons.get(semantic_id) {
        return ResolvedIcon::Url(url.clone());
    }
    if let Some(db_type) = semantic_id.strip_prefix("db.") {
        let driver_key = format!("db.{db_type}");
        if let Some(url) = seed.driver_icons.get(&driver_key) {
            return ResolvedIcon::Url(url.clone());
        }
        let icon = seed.db_icons.get(db_type).cloned().unwrap_or_else(|| DbIcon {
            label: "DB".to_owned(),
            bg_class: "bg-gray-500".to_owned(),
        });
        return ResolvedIcon::Placeholder {
            label: icon.label,
            bg_class: icon.bg_class,
        };
    }
    if let Some(name) = seed.lucide_by_id.get(semantic_id) {
        return ResolvedIcon::Lucide(name.clone());
    }
    ResolvedIcon::Placeholder {
        label: "?".to_owned(),
        bg_class: "bg-slate-600".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed() -> IconResolverSeed {
        let mut driver_icons = BTreeMap::new();
        driver_icons.insert("db.mysql".to_owned(), "/icons/mysql.svg".to_owned());
        let mut lucide_by_id = BTreeMap::new();
        lucide_by_id.insert("nav.settings".to_owned(), "Settings".to_owned());
        let mut db_icons = BTreeMap::new();
        db_icons.insert(
            "postgresql".to_owned(),
            DbIcon { label: "PG".to_owned(), bg_class: "bg-blue-500".to_owned() },
        );
        default_icon_resolver_seed(driver_icons, lucide_by_id, db_icons)
    }

    #[test]
    fn the_seed_starts_with_an_empty_pack_and_forwards_the_builtin_maps() {
        let seed = seed();
        assert!(seed.pack_icons.is_empty());
        assert_eq!(seed.driver_icons.get("db.mysql").map(String::as_str), Some("/icons/mysql.svg"));
        assert_eq!(seed.lucide_by_id.get("nav.settings").map(String::as_str), Some("Settings"));
        assert_eq!(seed.db_icons.get("postgresql").map(|icon| icon.label.as_str()), Some("PG"));
    }

    #[test]
    fn a_pack_icon_wins_over_driver_icons_and_lucide() {
        let mut seed = seed();
        seed.pack_icons.insert("db.mysql".to_owned(), "pack://mysql".to_owned());
        seed.pack_icons.insert("nav.settings".to_owned(), "pack://settings".to_owned());
        assert_eq!(
            resolve_icon(&seed, "db.mysql"),
            ResolvedIcon::Url("pack://mysql".to_owned())
        );
        assert_eq!(
            resolve_icon(&seed, "nav.settings"),
            ResolvedIcon::Url("pack://settings".to_owned())
        );
    }

    #[test]
    fn db_ids_resolve_to_the_builtin_driver_icon() {
        assert_eq!(
            resolve_icon(&seed(), "db.mysql"),
            ResolvedIcon::Url("/icons/mysql.svg".to_owned())
        );
    }

    #[test]
    fn db_ids_without_a_driver_icon_use_the_db_placeholder() {
        assert_eq!(
            resolve_icon(&seed(), "db.postgresql"),
            ResolvedIcon::Placeholder { label: "PG".to_owned(), bg_class: "bg-blue-500".to_owned() }
        );
    }

    #[test]
    fn an_unknown_db_type_falls_back_to_db_and_gray() {
        for id in ["db.unknown", "db.", "db.pg"] {
            assert_eq!(
                resolve_icon(&seed(), id),
                ResolvedIcon::Placeholder {
                    label: "DB".to_owned(),
                    bg_class: "bg-gray-500".to_owned()
                },
                "{id}"
            );
        }
    }

    #[test]
    fn semantic_ids_resolve_to_lucide_names() {
        assert_eq!(
            resolve_icon(&seed(), "nav.settings"),
            ResolvedIcon::Lucide("Settings".to_owned())
        );
    }

    #[test]
    fn unknown_ids_resolve_to_the_ui_placeholder() {
        for id in ["", "nav.unknown", "mysql", "dbx.mysql"] {
            assert_eq!(
                resolve_icon(&seed(), id),
                ResolvedIcon::Placeholder {
                    label: "?".to_owned(),
                    bg_class: "bg-slate-600".to_owned()
                },
                "{id}"
            );
        }
    }

    #[test]
    fn a_non_db_id_is_never_looked_up_in_the_driver_map() {
        let mut seed = seed();
        seed.driver_icons.insert("nav.settings".to_owned(), "wrong.svg".to_owned());
        assert_eq!(
            resolve_icon(&seed, "nav.settings"),
            ResolvedIcon::Lucide("Settings".to_owned())
        );
    }

    #[test]
    fn the_seed_owns_its_maps_independently_of_the_caller() {
        let mut driver_icons = BTreeMap::new();
        driver_icons.insert("db.mysql".to_owned(), "one.svg".to_owned());
        let seed = default_icon_resolver_seed(driver_icons.clone(), BTreeMap::new(), BTreeMap::new());
        driver_icons.insert("db.mysql".to_owned(), "two.svg".to_owned());
        assert_eq!(
            resolve_icon(&seed, "db.mysql"),
            ResolvedIcon::Url("one.svg".to_owned())
        );
    }
}
