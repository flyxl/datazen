#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DataExportCapability {
    None,
    LoadedOnly,
    FullTable,
}

impl DataExportCapability {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::LoadedOnly => "loaded_only",
            Self::FullTable => "full_table",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExportDriverMeta {
    pub export_scope: Option<DataExportCapability>,
}

fn scope_rank(capability: DataExportCapability) -> u8 {
    match capability {
        DataExportCapability::None => 0,
        DataExportCapability::LoadedOnly => 1,
        DataExportCapability::FullTable => 2,
    }
}

pub fn driver_export_scope(meta: Option<&ExportDriverMeta>) -> DataExportCapability {
    meta.and_then(|entry| entry.export_scope)
        .unwrap_or(DataExportCapability::FullTable)
}

pub fn resolve_export_scope(meta: Option<&ExportDriverMeta>) -> DataExportCapability {
    driver_export_scope(meta)
}

pub fn supports_full_table_export(capability: DataExportCapability) -> bool {
    capability == DataExportCapability::FullTable
}

pub fn supports_any_export(capability: DataExportCapability) -> bool {
    capability != DataExportCapability::None
}

pub fn min_scope(a: DataExportCapability, b: DataExportCapability) -> DataExportCapability {
    if scope_rank(a) <= scope_rank(b) {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use DataExportCapability::{FullTable, LoadedOnly, None as NoScope};

    #[test]
    fn wire_literals_are_pinned() {
        assert_eq!(NoScope.as_str(), "none");
        assert_eq!(LoadedOnly.as_str(), "loaded_only");
        assert_eq!(FullTable.as_str(), "full_table");
    }

    #[test]
    fn absent_metadata_defaults_to_full_table() {
        assert_eq!(driver_export_scope(None), FullTable);
        assert_eq!(
            driver_export_scope(Some(&ExportDriverMeta::default())),
            FullTable
        );
        for capability in [NoScope, LoadedOnly, FullTable] {
            let meta = ExportDriverMeta {
                export_scope: Some(capability),
            };
            assert_eq!(driver_export_scope(Some(&meta)), capability);
        }
    }

    #[test]
    fn resolve_matches_driver_for_every_input() {
        assert_eq!(resolve_export_scope(None), driver_export_scope(None));
        for capability in [NoScope, LoadedOnly, FullTable] {
            let meta = ExportDriverMeta {
                export_scope: Some(capability),
            };
            assert_eq!(
                resolve_export_scope(Some(&meta)),
                driver_export_scope(Some(&meta))
            );
        }
    }

    #[test]
    fn full_table_and_any_export_predicates() {
        assert!(!supports_full_table_export(NoScope));
        assert!(!supports_full_table_export(LoadedOnly));
        assert!(supports_full_table_export(FullTable));
        assert!(!supports_any_export(NoScope));
        assert!(supports_any_export(LoadedOnly));
        assert!(supports_any_export(FullTable));
    }

    #[test]
    fn rank_order_and_tie_behavior() {
        assert_eq!(scope_rank(NoScope), 0);
        assert_eq!(scope_rank(LoadedOnly), 1);
        assert_eq!(scope_rank(FullTable), 2);
        let all = [NoScope, LoadedOnly, FullTable];
        for a in all {
            for b in all {
                let expected = if scope_rank(a) <= scope_rank(b) { a } else { b };
                assert_eq!(min_scope(a, b), expected);
            }
        }
        assert_eq!(min_scope(LoadedOnly, NoScope), NoScope);
        assert_eq!(min_scope(NoScope, FullTable), NoScope);
        assert_eq!(min_scope(FullTable, LoadedOnly), LoadedOnly);
    }
}
