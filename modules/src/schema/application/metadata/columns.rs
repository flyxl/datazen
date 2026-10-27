use super::*;
use crate::schema::CachedColumns;
use datazen_driver_api::{CatalogReader, LegacyCatalogReader};
use std::{collections::HashMap, time::Instant};

/// Group only exact scopes. The legacy bare-name batch payload is safe within
/// one schema, never across schemas with same-named relations.
pub(super) async fn read(
    schema_cache: &SchemaCache,
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
    let catalog = LegacyCatalogReader::new(driver.as_ref());
    let mut seen = HashSet::new();
    let relations: Vec<_> = relations
        .into_iter()
        .filter(|r| seen.insert(r.clone()))
        .collect();
    let mut values = HashMap::new();
    let mut groups: HashMap<(String, Option<String>), Vec<RelationRef>> = HashMap::new();
    for relation in &relations {
        if let Some(cached) = schema_cache
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
        let generation = schema_cache.generation();
        let mut batch = if missing.len() > 1 {
            match catalog
                .get_all_columns(handle, &database, schema.as_deref())
                .await
            {
                Ok(batch) => batch,
                Err(DriverError::Unsupported(_) | DriverError::NotSupported(_)) => HashMap::new(),
                Err(error) => {
                    let message =
                        datazen_infra::redaction::redact_secrets_for_log(&error.to_string());
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
                schema_cache
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
                schema_cache
                    .get_columns(
                        session,
                        &database,
                        schema.as_deref(),
                        &relation.name,
                        &catalog,
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
                        message: datazen_infra::redaction::redact_secrets_for_log(
                            &error.to_string(),
                        ),
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

/// One bounded metadata lease, grouped batch reads and sequential fallback.
/// The gateway owns the handle; no legacy cache is read or written.
pub(super) async fn read_on_runtime_resource(
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
    let catalog = LegacyCatalogReader::new(driver.as_ref());
    let mut seen = HashSet::new();
    let relations: Vec<_> = relations
        .into_iter()
        .filter(|relation| seen.insert(relation.clone()))
        .collect();
    let mut groups: HashMap<(String, Option<String>), Vec<RelationRef>> = HashMap::new();
    for relation in &relations {
        groups
            .entry((relation.database.clone(), relation.schema.clone()))
            .or_default()
            .push(relation.clone());
    }
    let mut values = HashMap::new();
    for ((database, schema), requested) in groups {
        let mut batch = if requested.len() > 1 {
            match catalog
                .get_all_columns(handle, &database, schema.as_deref())
                .await
            {
                Ok(batch) => batch,
                Err(DriverError::Unsupported(_) | DriverError::NotSupported(_)) => HashMap::new(),
                Err(error) => return Err(CommandError::Driver(error)),
            }
        } else {
            HashMap::new()
        };
        for relation in requested {
            let read = if let Some(value) = batch.remove(&relation.name) {
                Ok(value)
            } else {
                catalog
                    .get_columns(
                        handle,
                        &relation.name,
                        &relation.database,
                        relation.schema.as_deref(),
                    )
                    .await
            };
            let value = match read {
                Ok((columns, primary_keys)) => ColumnsReadResult::Ok {
                    value: RelationColumns {
                        relation: relation.clone(),
                        columns,
                        primary_keys,
                    },
                },
                Err(error) => ColumnsReadResult::Error {
                    relation: relation.clone(),
                    error: MetadataReadError {
                        code: if matches!(
                            error,
                            DriverError::Unsupported(_) | DriverError::NotSupported(_)
                        ) {
                            MetadataReadErrorCode::Unsupported
                        } else {
                            MetadataReadErrorCode::ReadFailed
                        },
                        message: datazen_infra::redaction::redact_secrets_for_log(
                            &error.to_string(),
                        ),
                    },
                },
            };
            values.insert(relation, value);
        }
    }
    let results = relations
        .into_iter()
        .map(|relation| {
            values
                .remove(&relation)
                .ok_or_else(|| CommandError::Internal("Metadata batch omitted relation".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ReadColumnsOutput { results })
}
