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
