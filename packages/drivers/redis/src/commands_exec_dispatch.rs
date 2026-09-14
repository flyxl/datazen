match command {
        "scan_keys" => {
            let pattern = opt_str(&input, "pattern").unwrap_or("*");
            let cursor = input.get("cursor").and_then(JsonValue::as_u64).unwrap_or(0);
            let count = input
                .get("count")
                .and_then(JsonValue::as_u64)
                .unwrap_or(100) as u32;
            let key_type = opt_str(&input, "keyType").or_else(|| opt_str(&input, "key_type"));
            let with_memory = input
                .get("withMemory")
                .or_else(|| input.get("with_memory"))
                .and_then(JsonValue::as_bool)
                .unwrap_or(false);
            let (next, keys, db_size) = driver
                .scan_keys_with_info(handle, db, pattern, cursor, count, key_type, with_memory)
                .await?;
            json_ok(serde_json::json!({ "cursor": next, "keys": keys, "dbSize": db_size }))
        }
        "get_key" => json_ok(
            driver
                .get_key_detail(handle, db, req_str(&input, "key")?)
                .await?,
        ),
        "set_string" => {
            let keep_ttl = input
                .get("keepTtl")
                .or_else(|| input.get("keep_ttl"))
                .and_then(JsonValue::as_bool)
                .unwrap_or(false);
            driver
                .plugin_set_string(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "value")?,
                    keep_ttl,
                )
                .await?;
            Ok(ok())
        }
        "hash_set" => {
            driver
                .plugin_hash_set(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "field")?,
                    req_str(&input, "value")?,
                )
                .await?;
            Ok(ok())
        }
        "hash_del" => {
            let fields = string_vec(&input, "fields")?;
            driver
                .plugin_hash_del(id, db, req_str(&input, "key")?, &fields)
                .await?;
            Ok(ok())
        }
        "list_push" => {
            let side = req_str(&input, "side")?;
            let values = string_vec(&input, "values")?;
            driver
                .plugin_list_push(id, db, req_str(&input, "key")?, side, &values)
                .await?;
            Ok(ok())
        }
        "list_set" => {
            driver
                .plugin_list_set(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_i64(&input, "index")?,
                    req_str(&input, "value")?,
                )
                .await?;
            Ok(ok())
        }
        "list_pop" => json_ok(
            driver
                .plugin_list_pop(id, db, req_str(&input, "key")?, req_str(&input, "side")?)
                .await?,
        ),
        "set_add" => {
            let members = string_vec(&input, "members")?;
            driver
                .plugin_set_add(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "set_remove" => {
            let members = string_vec(&input, "members")?;
            driver
                .plugin_set_remove(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "zset_add" => {
            let members: Vec<ZsetMember> = serde_json::from_value(
                input.get("members").cloned().unwrap_or(JsonValue::Array(vec![])),
            )
            .map_err(|e| DriverError::InvalidConfig(e.to_string()))?;
            driver
                .plugin_zset_add(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "zset_remove" => {
            let members = string_vec(&input, "members")?;
            driver
                .plugin_zset_remove(id, db, req_str(&input, "key")?, &members)
                .await?;
            Ok(ok())
        }
        "delete_keys" => {
            let keys = string_vec(&input, "keys")?;
            json_ok(driver.plugin_delete_keys(id, db, &keys).await?)
        }
        "rename" => {
            let new_key = input
                .get("newKey")
                .or_else(|| input.get("new_key"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'newKey'".into())
                })?;
            driver
                .plugin_rename_key(id, db, req_str(&input, "key")?, new_key)
                .await?;
            Ok(ok())
        }
        "set_ttl" => {
            let key = req_str(&input, "key")?;
            let expire_at = input
                .get("expireAt")
                .or_else(|| input.get("expire_at"))
                .and_then(JsonValue::as_i64);
            if let Some(ts) = expire_at {
                driver.plugin_set_expire_at(id, db, key, ts).await?;
            } else {
                let ttl = input
                    .get("ttlSeconds")
                    .or_else(|| input.get("ttl_seconds"))
                    .and_then(JsonValue::as_i64)
                    .ok_or_else(|| {
                        DriverError::InvalidConfig(
                            "command input requires 'ttlSeconds' or 'expireAt'".into(),
                        )
                    })?;
                driver.plugin_set_ttl(id, db, key, ttl).await?;
            }
            Ok(ok())
        }
        "batch_delete_pattern" => json_ok(
            driver
                .plugin_batch_delete_pattern(id, db, req_str(&input, "pattern")?)
                .await?,
        ),
        "batch_set_ttl" => {
            let keys = string_vec(&input, "keys")?;
            let ttl = input
                .get("ttlSeconds")
                .or_else(|| input.get("ttl_seconds"))
                .and_then(JsonValue::as_i64)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'ttlSeconds'".into())
                })?;
            json_ok(driver.plugin_batch_set_ttl(id, db, &keys, ttl).await?)
        }
        "batch_rename_prefix" => {
            let old_prefix = input
                .get("oldPrefix")
                .or_else(|| input.get("old_prefix"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'oldPrefix'".into())
                })?;
            let new_prefix = input
                .get("newPrefix")
                .or_else(|| input.get("new_prefix"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'newPrefix'".into())
                })?;
            let keys = input
                .get("keys")
                .and_then(JsonValue::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(JsonValue::as_str)
                        .map(str::to_string)
                        .collect::<Vec<_>>()
                });
            json_ok(
                driver
                    .plugin_batch_rename_prefix(id, db, old_prefix, new_prefix, keys)
                    .await?,
            )
        }
        "flush_db" => {
            crate::ops::ensure_flush_allowed(
                req_bool(&input, "allowFlush").or_else(|_| req_bool(&input, "allow_flush"))?,
            )
            .map_err(DriverError::InvalidConfig)?;
            driver.plugin_flush_db(id, db).await?;
            Ok(ok())
        }
        "flush_all" => {
            crate::ops::ensure_flush_allowed(
                req_bool(&input, "allowFlush").or_else(|_| req_bool(&input, "allow_flush"))?,
            )
            .map_err(DriverError::InvalidConfig)?;
            driver.plugin_flush_all(id).await?;
            Ok(ok())
        }
        "count_matching" => json_ok(
            driver
                .plugin_count_matching(id, db, req_str(&input, "pattern")?)
                .await?,
        ),
        "cluster_nodes" => json_ok(driver.plugin_cluster_nodes(id).await?),
        "info" => json_ok(
            driver
                .plugin_info(
                    id,
                    opt_str(&input, "section").map(str::to_string),
                    opt_str(&input, "nodeAddr")
                        .or_else(|| opt_str(&input, "node_addr"))
                        .map(str::to_string),
                )
                .await?,
        ),
        "memory_sample" => json_ok(
            driver
                .plugin_memory_sample(
                    id,
                    db,
                    input
                        .get("limit")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                )
                .await?,
        ),
        "slowlog_get" => json_ok(
            driver
                .plugin_slowlog_get(
                    id,
                    input.get("count").and_then(JsonValue::as_u64).unwrap_or(16) as u32,
                )
                .await?,
        ),
        "slowlog_reset" => {
            crate::ops_observe::ensure_slowlog_reset_confirmed(req_bool(&input, "confirm")?)
                .map_err(DriverError::InvalidConfig)?;
            driver.plugin_slowlog_reset(id).await?;
            Ok(ok())
        }
        "modules_list" => json_ok(driver.plugin_modules_list(id).await?),
        "exec" => json_ok(
            driver
                .plugin_exec(
                    id,
                    db,
                    req_str(&input, "commands")?,
                    opt_str(&input, "nodeAddr")
                        .or_else(|| opt_str(&input, "node_addr"))
                        .map(str::to_string),
                )
                .await?,
        ),
        "pubsub_publish" => json_ok(
            driver
                .plugin_pubsub_publish(id, req_str(&input, "channel")?, req_str(&input, "message")?)
                .await?,
        ),
        "pubsub_subscribe" => {
            let channels = opt_string_vec(&input, "channels");
            let patterns = opt_string_vec(&input, "patterns");
            json_ok(
                crate::ops_pubsub::start_subscription(
                    driver,
                    handle.pool_id.clone(),
                    channels,
                    patterns,
                )
                .await
                .map_err(DriverError::QueryFailed)?,
            )
        }
        "pubsub_unsubscribe" => {
            let sub_id = input
                .get("subscriptionId")
                .or_else(|| input.get("subscription_id"))
                .and_then(JsonValue::as_str)
                .ok_or_else(|| {
                    DriverError::InvalidConfig("command input requires 'subscriptionId'".into())
                })?;
            crate::ops_pubsub::unsubscribe(sub_id)
                .await
                .map_err(DriverError::QueryFailed)?;
            Ok(ok())
        }
        "json_get" => json_ok(
            driver
                .plugin_json_get(
                    id,
                    db,
                    req_str(&input, "key")?,
                    opt_str(&input, "path").unwrap_or("$"),
                )
                .await?,
        ),
        "json_set" => {
            driver
                .plugin_json_set(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "path")?,
                    req_str(&input, "value")?,
                )
                .await?;
            Ok(ok())
        }
        "json_del" => json_ok(
            driver
                .plugin_json_del(id, db, req_str(&input, "key")?, req_str(&input, "path")?)
                .await?,
        ),
        "xrange" => json_ok(
            driver
                .plugin_xrange(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "start")?,
                    req_str(&input, "end")?,
                    input
                        .get("count")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                )
                .await?,
        ),
        "xadd" => {
            let fields: std::collections::HashMap<String, String> =
                serde_json::from_value(input.get("fields").cloned().unwrap_or(JsonValue::Null))
                    .map_err(|e| DriverError::InvalidConfig(e.to_string()))?;
            json_ok(
                driver
                    .plugin_xadd(
                        id,
                        db,
                        req_str(&input, "key")?,
                        &fields,
                        opt_str(&input, "id").map(str::to_string),
                    )
                    .await?,
            )
        }
        "xgroup_create" => {
            driver
                .plugin_xgroup_create(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "group")?,
                    opt_str(&input, "startId")
                        .or_else(|| opt_str(&input, "start_id"))
                        .map(str::to_string),
                )
                .await?;
            Ok(ok())
        }
        "xgroup_destroy" => {
            driver
                .plugin_xgroup_destroy(id, db, req_str(&input, "key")?, req_str(&input, "group")?)
                .await?;
            Ok(ok())
        }
        "xinfo_groups" => json_ok(
            driver
                .plugin_xinfo_groups(id, db, req_str(&input, "key")?)
                .await?,
        ),
        "xpending" => json_ok(
            driver
                .plugin_xpending(
                    id,
                    db,
                    req_str(&input, "key")?,
                    req_str(&input, "group")?,
                    opt_str(&input, "start").map(str::to_string),
                    opt_str(&input, "end").map(str::to_string),
                    input
                        .get("count")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                    opt_str(&input, "consumer").map(str::to_string),
                )
                .await?,
        ),
        "xack" => {
            let ids = string_vec(&input, "ids")?;
            json_ok(
                driver
                    .plugin_xack(
                        id,
                        db,
                        req_str(&input, "key")?,
                        req_str(&input, "group")?,
                        &ids,
                    )
                    .await?,
            )
        }
        "stream_overview" => json_ok(
            driver
                .plugin_stream_overview(
                    id,
                    db,
                    input
                        .get("limit")
                        .and_then(JsonValue::as_u64)
                        .map(|v| v as u32),
                )
                .await?,
        ),
        "dump_keys" => {
            let keys = string_vec(&input, "keys")?;
            json_ok(driver.plugin_dump_keys(id, db, &keys).await?)
        }
        "restore_keys" => {
            let entries: Vec<RestoreKeyEntry> =
                serde_json::from_value(input.get("entries").cloned().unwrap_or(JsonValue::Null))
                    .map_err(|e| DriverError::InvalidConfig(e.to_string()))?;
            json_ok(
                driver
                    .plugin_restore_keys(
                        id,
                        db,
                        entries,
                        input
                            .get("replace")
                            .and_then(JsonValue::as_bool)
                            .unwrap_or(false),
                    )
                    .await?,
            )
        }
        other => Err(DriverError::Unsupported(format!(
            "unsupported driver command: {other}"
        ))),
    }
