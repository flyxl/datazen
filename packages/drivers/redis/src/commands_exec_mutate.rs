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
