use super::*;
use crate::cache::schema_cache::CachedColumns;
use std::{collections::HashMap, time::Instant};

/// Group only exact scopes. The legacy bare-name batch payload is safe within
/// one schema, never across schemas with same-named relations.
pub(super) async fn read(
    state: &AppState,
    session: &str,
    driver: &Arc<dyn DatabaseDriver>,
    handle: &ConnectionHandle,
    relations: Vec<RelationRef>,
) -> Result<ReadColumnsOutput, CommandError> {
    if relations.len() > 256 {
        return Err(CommandError::Validation(
            "At most 256 relations per metadata request".into(),
        ));
    }
    for relation in &relations {
        validate_relation(driver.as_ref(), relation)?;
    }
    let mut seen = HashSet::new();
    let relations: Vec<_> = relations
        .into_iter()
        .filter(|r| seen.insert(r.clone()))
        .collect();
    let mut values = HashMap::new();
    let mut groups: HashMap<(String, Option<String>), Vec<RelationRef>> = HashMap::new();
    for relation in &relations {
        if let Some(cached) = state
            .schema_cache
            .try_get_cached_columns(
                session,
                &relation.database,
                relation.schema.as_deref(),
                &relation.name,
            )
            .await
        {
            values.insert(relation.clone(), Ok(cached));
        } else {
            groups
                .entry((relation.database.clone(), relation.schema.clone()))
                .or_default()
                .push(relation.clone());
        }
    }
    for ((database, schema), missing) in groups {
        let generation = state.schema_cache.generation();
        let mut batch = if missing.len() > 1 {
            match driver
                .get_all_columns(handle, &database, schema.as_deref())
                .await
            {
                Ok(batch) => batch,
                Err(DriverError::Unsupported(_) | DriverError::NotSupported(_)) => HashMap::new(),
                Err(error) => {
                    let message = crate::log_redact::redact_secrets_for_log(&error.to_string());
                    for relation in missing {
                        values.insert(
                            relation,
                            Err(MetadataReadError {
                                code: MetadataReadErrorCode::ReadFailed,
                                message: message.clone(),
                            }),
                        );
                    }
                    continue;
                }
            }
        } else {
            HashMap::new()
        };
        for relation in missing {
            let columns = if let Some((columns, primary_keys)) = batch.remove(&relation.name) {
                let cached = CachedColumns {
                    columns,
                    primary_keys,
                    table_name: relation.name.clone(),
                    cached_at: Instant::now(),
                };
                state
                    .schema_cache
                    .store_columns_if_current(
                        session,
                        &database,
                        schema.as_deref(),
                        &relation.name,
                        cached.clone(),
                        generation,
                    )
                    .await;
                Ok(cached)
            } else {
                state
                    .schema_cache
                    .get_columns(
                        session,
                        &database,
                        schema.as_deref(),
                        &relation.name,
                        driver,
                        handle,
                    )
                    .await
                    .map_err(|error| MetadataReadError {
                        code: match &error {
                            DriverError::Unsupported(_) | DriverError::NotSupported(_) => {
                                MetadataReadErrorCode::Unsupported
                            }
                            _ => MetadataReadErrorCode::ReadFailed,
                        },
                        message: crate::log_redact::redact_secrets_for_log(&error.to_string()),
                    })
            };
            values.insert(relation, columns);
        }
    }
    let mut results = Vec::with_capacity(relations.len());
    for relation in relations {
        let value = values.remove(&relation).ok_or_else(|| {
            CommandError::Internal("Metadata batch omitted a requested relation".into())
        })?;
        results.push(match value {
            Ok(columns) => ColumnsReadResult::Ok {
                value: RelationColumns {
                    relation,
                    columns: columns.columns,
                    primary_keys: columns.primary_keys,
                },
            },
            Err(error) => ColumnsReadResult::Error { relation, error },
        });
    }
    Ok(ReadColumnsOutput { results })
}
