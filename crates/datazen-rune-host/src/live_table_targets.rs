//! Resolve the current Rune shell's bare table keys without guessing a schema.
//!
//! The UI still sends table names rather than structured relation identities.
//! Preserve the namespace supplied by the live driver's catalog, and refuse
//! an ambiguous name rather than reading/writing whichever table is first.
use std::collections::{hash_map::Entry, HashMap};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct TableTarget {
    pub database: String,
    pub schema: Option<String>,
}

#[derive(Default)]
pub(super) struct LiveTableTargets {
    by_name: HashMap<String, Option<TableTarget>>,
}

impl LiveTableTargets {
    pub fn remember(&mut self, database: &str, table: &datazen_driver_api::TableInfo) {
        if table.name.is_empty() {
            return;
        }
        let target = TableTarget {
            database: database.to_owned(),
            schema: table.schema.clone(),
        };
        match self.by_name.entry(table.name.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(Some(target));
            }
            Entry::Occupied(mut entry) => {
                if entry.get().as_ref() != Some(&target) {
                    entry.insert(None);
                }
            }
        }
    }

    pub fn resolve(&self, table: &str) -> Result<TableTarget, String> {
        match self.by_name.get(table) {
            Some(Some(target)) => Ok(target.clone()),
            Some(None) => Err("This table name is ambiguous across namespaces.".into()),
            None => Err(
                "The live catalog does not contain this table. Refresh the catalog and retry."
                    .into(),
            ),
        }
    }
}
