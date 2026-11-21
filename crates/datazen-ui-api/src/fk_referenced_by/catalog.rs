use std::collections::HashMap;

use super::types::{
    CatalogScope, InboundCatalogFetcher, InboundForeignKeyCatalog, PROBE_FAILED_MESSAGE,
};

#[derive(Clone, Debug, PartialEq)]
enum CatalogState {
    Loading,
    Ready(InboundForeignKeyCatalog),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct InboundCatalogCache {
    entries: HashMap<String, (CatalogScope, CatalogState)>,
}

pub fn catalog_key(scope: &CatalogScope) -> String {
    serde_json::json!([
        scope.db_session_id.clone(),
        scope.database.clone(),
        scope.schema.clone(),
        scope.table.clone(),
    ])
    .to_string()
}

impl InboundCatalogCache {
    pub fn get_ready(&self, scope: &CatalogScope) -> Option<&InboundForeignKeyCatalog> {
        match self.entries.get(&catalog_key(scope)) {
            Some((_, CatalogState::Ready(catalog))) => Some(catalog),
            _ => None,
        }
    }

    pub fn is_loading(&self, scope: &CatalogScope) -> bool {
        matches!(
            self.entries.get(&catalog_key(scope)),
            Some((_, CatalogState::Loading))
        )
    }

    pub fn mark_loading(&mut self, scope: &CatalogScope) {
        self.entries
            .insert(catalog_key(scope), (scope.clone(), CatalogState::Loading));
    }

    pub fn mark_ready(&mut self, scope: &CatalogScope, catalog: InboundForeignKeyCatalog) {
        self.entries.insert(
            catalog_key(scope),
            (scope.clone(), CatalogState::Ready(catalog)),
        );
    }

    pub fn remove(&mut self, scope: &CatalogScope) {
        self.entries.remove(&catalog_key(scope));
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn invalidate(&mut self, db_session_id: &str, table_name: Option<&str>) {
        self.entries.retain(|_, (scope, _)| {
            !(scope.db_session_id == db_session_id
                && table_name.is_none_or(|table| table == scope.table))
        });
    }
}

pub fn inbound_catalog(
    scope: &CatalogScope,
    cache: &mut InboundCatalogCache,
    fetcher: &mut InboundCatalogFetcher,
) -> Result<InboundForeignKeyCatalog, String> {
    if let Some(catalog) = cache.get_ready(scope) {
        return Ok(catalog.clone());
    }
    if cache.is_loading(scope) {
        return Err(PROBE_FAILED_MESSAGE.to_owned());
    }
    cache.mark_loading(scope);
    match fetcher(scope) {
        Ok(catalog) => {
            cache.mark_ready(scope, catalog.clone());
            Ok(catalog)
        }
        Err(cause) => {
            cache.remove(scope);
            Err(cause)
        }
    }
}
