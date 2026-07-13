// ── Kiwi login (username/password → SSO → token) ─────────────────

#[tauri::command]
pub async fn kiwi_login(
    base_url: String,
    username: String,
    password: String,
) -> Result<serde_json::Value, String> {
    tracing::info!("[kiwi_login] base_url={base_url}, user={username}");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let token = crate::db::kiwi::sso_login(
        &client,
        base_url.trim_end_matches('/'),
        &username,
        &password,
    )
    .await
    .map_err(|e| {
        tracing::error!("[kiwi_login] failed: {e}");
        e.to_string()
    })?;

    tracing::info!("[kiwi_login] success, token_len={}", token.len());
    Ok(serde_json::json!({
        "token": token,
        "username": username,
    }))
}

/// Kiwi: list instances for a given token (called from frontend).
#[tauri::command]
pub async fn kiwi_list_instances(
    base_url: String,
    token: String,
    source_type: Option<u32>,
    user_name: Option<String>,
) -> Result<serde_json::Value, String> {
    let st = source_type.unwrap_or(4);
    let url = format!(
        "{}/gw/v1/dataquery/instances?source_type={}",
        base_url.trim_end_matches('/'),
        st
    );

    tracing::info!("[kiwi_list_instances] GET {url}");

    let client = reqwest::Client::builder()
        .danger_accept_invalid_certs(true)
        .build()
        .map_err(|e| format!("HTTP client: {e}"))?;

    let mut req = client
        .get(&url)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json, text/plain, */*")
        .header("X-Token", &token)
        .header("authorization", &token)
        .header("lang", "en");

    if let Some(ref uname) = user_name {
        if !uname.is_empty() {
            req = req.header("user_name", uname);
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| {
            tracing::error!("[kiwi_list_instances] request error: {e}");
            format!("Request failed: {e}")
        })?;

    let status = resp.status();
    let body_text = resp
        .text()
        .await
        .map_err(|e| format!("Read body: {e}"))?;

    tracing::info!("[kiwi_list_instances] status={status}, body_len={}", body_text.len());
    if body_text.len() < 500 {
        tracing::info!("[kiwi_list_instances] body: {body_text}");
    }

    let body: serde_json::Value = serde_json::from_str(&body_text)
        .map_err(|e| format!("Parse JSON: {e} — body: {}", &body_text[..body_text.len().min(200)]))?;

    Ok(body)
}
