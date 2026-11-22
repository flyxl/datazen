use std::collections::BTreeMap;

use super::identity::is_verified_migration_pair;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionLike {
    pub id: String,
    pub name: String,
    pub database_type: String,
    pub read_only: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConnectionOption {
    pub value: String,
    pub label: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TargetOption {
    pub value: String,
    pub label: String,
    pub disabled: bool,
    pub title: Option<String>,
}

pub fn connection_label(connection: &ConnectionLike) -> String {
    format!("{} ({})", connection.name, connection.database_type)
}

pub fn build_conn_options(connections: &[ConnectionLike]) -> Vec<ConnectionOption> {
    connections
        .iter()
        .map(|connection| ConnectionOption {
            value: connection.id.clone(),
            label: connection_label(connection),
        })
        .collect()
}

pub fn build_target_options(
    connections: &[ConnectionLike],
    source_type: Option<&str>,
    target_support: &BTreeMap<String, bool>,
    unsupported_hint: &str,
    experimental_hint: &str,
) -> Vec<TargetOption> {
    connections
        .iter()
        .map(|connection| {
            let base = connection_label(connection);
            let has_support = source_type.is_some() && target_support.contains_key(&connection.id);
            let unsupported = has_support && !target_support[&connection.id];
            let experimental = source_type.is_some()
                && !unsupported
                && has_support
                && !is_verified_migration_pair(
                    source_type.unwrap_or(""),
                    &connection.database_type,
                );
            let title = if unsupported {
                Some(unsupported_hint.to_owned())
            } else if experimental {
                Some(experimental_hint.to_owned())
            } else {
                None
            };
            TargetOption {
                value: connection.id.clone(),
                label: match title.as_deref() {
                    Some(hint) => format!("{base} — {hint}"),
                    None => base,
                },
                disabled: unsupported,
                title,
            }
        })
        .collect()
}

pub fn is_experimental(
    source: Option<&ConnectionLike>,
    target: Option<&ConnectionLike>,
    pairing_supported: bool,
) -> bool {
    match (source, target) {
        (Some(source), Some(target)) => {
            pairing_supported
                && !is_verified_migration_pair(&source.database_type, &target.database_type)
        }
        _ => false,
    }
}

pub fn is_target_read_only(target: Option<&ConnectionLike>) -> bool {
    target.is_some_and(|connection| connection.read_only)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn(id: &str, kind: &str) -> ConnectionLike {
        ConnectionLike {
            id: id.to_owned(),
            name: id.to_owned(),
            database_type: kind.to_owned(),
            read_only: false,
        }
    }

    #[test]
    fn connection_options_use_the_name_and_type_label() {
        let options = build_conn_options(&[conn("c1", "postgresql")]);
        assert_eq!(options[0].value, "c1");
        assert_eq!(options[0].label, "c1 (postgresql)");
    }

    #[test]
    fn target_options_distinguish_unsupported_experimental_and_plain() {
        let connections = [
            conn("a", "postgresql"),
            conn("b", "sqlite"),
            conn("c", "mariadb"),
        ];
        let support = BTreeMap::from([
            ("a".to_owned(), true),
            ("b".to_owned(), false),
            ("c".to_owned(), true),
        ]);
        let options = build_target_options(
            &connections,
            Some("postgresql"),
            &support,
            "UNSUPPORTED",
            "EXPERIMENTAL",
        );
        assert_eq!(options[0].label, "a (postgresql)");
        assert_eq!(options[0].title, None);
        assert!(options[1].disabled);
        assert_eq!(options[1].label, "b (sqlite) — UNSUPPORTED");
        assert_eq!(options[1].title.as_deref(), Some("UNSUPPORTED"));
        assert!(!options[2].disabled);
        assert_eq!(options[2].label, "c (mariadb) — EXPERIMENTAL");

        let without_source =
            build_target_options(&connections, None, &support, "UNSUPPORTED", "EXPERIMENTAL");
        assert!(without_source
            .iter()
            .all(|option| option.title.is_none() && !option.disabled));
    }

    #[test]
    fn experimental_flag_and_read_only_are_derived() {
        let source = conn("a", "postgresql");
        let target = conn("b", "sqlite");
        assert!(is_experimental(Some(&source), Some(&target), true));
        assert!(!is_experimental(Some(&source), Some(&target), false));
        let verified = conn("c", "mysql");
        assert!(!is_experimental(Some(&source), Some(&verified), true));
        assert!(!is_experimental(None, Some(&target), true));

        let mut readonly = conn("r", "mysql");
        readonly.read_only = true;
        assert!(is_target_read_only(Some(&readonly)));
        assert!(!is_target_read_only(Some(&source)));
        assert!(!is_target_read_only(None));
    }
}
