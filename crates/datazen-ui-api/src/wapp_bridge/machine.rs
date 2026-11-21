use serde_json::Value;

use super::host::WappBridgeHost;
use super::protocol::{
    err_envelope, host_ready_envelope, ok_envelope, parse_request, response_type_of,
    theme_apply_envelope, to_public_connection, BridgeErrorCode, WappRequestEnvelope,
    WappResponseEnvelope, BRIDGE_CHANNEL, ERROR_MESSAGE_LIMIT_CHARS, SESSION_REQUEST_LIMIT,
};
use super::routes::{is_wapp_command_allowed, route_for};
use super::state::{
    timeout_disposition, AttachBridgeOptions, BridgeEffect, BridgeState, SessionRequestRecord,
    TimeoutDisposition,
};
use super::validation::{
    looks_like_not_found, parse_command_invoke, parse_i18n_key, parse_notify, parse_storage,
    BridgeApiError,
};
use crate::types::wapp::WappPermission;

pub struct WappBridge {
    pub options: AttachBridgeOptions,
    pub state: BridgeState,
    next_timer: u64,
}

impl WappBridge {
    pub fn new(options: AttachBridgeOptions, initial_nonce: String) -> Self {
        Self {
            options,
            state: BridgeState::new(initial_nonce),
            next_timer: 0,
        }
    }

    fn granted(&self, permission: WappPermission) -> bool {
        self.options.permissions.contains(&permission)
    }

    pub fn attach(&mut self) -> Vec<BridgeEffect> {
        vec![
            BridgeEffect::AddFrameLoadListener,
            BridgeEffect::AddMessageListener,
        ]
    }

    pub fn on_message(
        &mut self,
        host: &mut dyn WappBridgeHost,
        source_is_iframe: bool,
        data: &Value,
    ) -> Vec<BridgeEffect> {
        if self.state.detached || !source_is_iframe {
            return Vec::new();
        }
        let request = match parse_request(data) {
            Some(request) => request,
            None => return Vec::new(),
        };
        if request.kind == "wapp.ready" {
            let ch = if request.ch.is_empty() {
                BRIDGE_CHANNEL.to_owned()
            } else {
                request.ch.clone()
            };
            let envelope = host_ready_envelope(
                &ch,
                self.options.api_version,
                &self.state.instance_nonce,
                &self.options.locale,
                &host.theme_snapshot(),
            );
            return vec![BridgeEffect::PostMessage(envelope)];
        }
        if !BridgeState::request_id_valid(&request.req_id) {
            return Vec::new();
        }
        self.dispatch(host, &request)
    }

    fn dispatch(
        &mut self,
        host: &mut dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> Vec<BridgeEffect> {
        let mut effects = Vec::new();
        let required = match route_for(&request.kind) {
            Some(required) => required,
            None => {
                let response = err_envelope(
                    &request.ch,
                    request.req_id.as_deref(),
                    BridgeErrorCode::NotFound,
                    format!("unknown api \"{}\"", request.kind),
                );
                effects.push(self.respond(request, WappResponseEnvelope::Err(response)));
                return effects;
            }
        };
        if let Some(permission) = required {
            if !self.granted(permission) {
                let response = err_envelope(
                    &request.ch,
                    request.req_id.as_deref(),
                    BridgeErrorCode::Permission,
                    format!(
                        "\"{}\" needs permission \"{}\"",
                        request.kind,
                        permission.as_str()
                    ),
                );
                effects.push(self.respond(request, WappResponseEnvelope::Err(response)));
                return effects;
            }
        }
        if self.state.inflight >= self.options.max_inflight {
            let response = err_envelope(
                &request.ch,
                request.req_id.as_deref(),
                BridgeErrorCode::RateLimit,
                format!(
                    "more than {} unfinished requests",
                    self.options.max_inflight
                ),
            );
            effects.push(self.respond(request, WappResponseEnvelope::Err(response)));
            return effects;
        }

        self.state.inflight += 1;
        let generation = self.state.generation;
        let token = self.next_timer;
        self.next_timer = self.next_timer.saturating_add(1);
        self.state.timers.insert(token, generation);
        effects.push(BridgeEffect::SetTimer {
            token,
            generation,
            duration_ms: self.options.timeout_ms,
        });
        let response = self.handle(host, request);
        self.state.timers.remove(&token);
        effects.push(BridgeEffect::ClearTimer { token });
        self.state.inflight -= 1;
        if !self.state.detached && self.state.generation == generation {
            effects.push(self.respond(request, response));
        }
        effects
    }

    fn respond(
        &self,
        request: &WappRequestEnvelope,
        response: WappResponseEnvelope,
    ) -> BridgeEffect {
        let mut value = response.to_value();
        if let Some(object) = value.as_object_mut() {
            let ch = if request.ch.is_empty() {
                BRIDGE_CHANNEL
            } else {
                request.ch.as_str()
            };
            object.insert("ch".to_owned(), Value::String(ch.to_owned()));
            object.insert(
                "type".to_owned(),
                Value::String(response_type_of(&request.kind, response.is_ok())),
            );
            match &request.req_id {
                Some(id) => {
                    object.insert("reqId".to_owned(), Value::String(id.clone()));
                }
                None => {
                    object.remove("reqId");
                }
            }
        }
        BridgeEffect::PostMessage(value)
    }

    fn ok(&self, request: &WappRequestEnvelope, payload: Value) -> WappResponseEnvelope {
        WappResponseEnvelope::Ok(ok_envelope(
            &request.ch,
            request.req_id.as_deref(),
            Some(payload),
        ))
    }

    fn fail(&self, request: &WappRequestEnvelope, error: BridgeApiError) -> WappResponseEnvelope {
        WappResponseEnvelope::Err(err_envelope(
            &request.ch,
            request.req_id.as_deref(),
            error.code,
            error.message,
        ))
    }

    fn handle(
        &mut self,
        host: &mut dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        match request.kind.as_str() {
            "context.getConnections" => self.handle_get_connections(host, request),
            "context.getActiveConnection" => self.handle_get_active(host, request),
            "command.invoke" => self.handle_command_invoke(host, request),
            "session.open" | "session.use" | "session.close" | "session.receipt" => {
                self.handle_session(host, request)
            }
            "storage.get" | "storage.set" | "storage.remove" => self.handle_storage(host, request),
            "ui.notify" => self.handle_notify(host, request),
            "i18n.getString" => self.handle_i18n(host, request),
            _ => self.fail(
                request,
                BridgeApiError::not_found(format!("unknown api \"{}\"", request.kind)),
            ),
        }
    }

    fn handle_get_connections(
        &self,
        host: &dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        let connections = host
            .load_connections()
            .iter()
            .map(to_public_connection)
            .map(|summary| serde_json::to_value(summary).unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        self.ok(request, serde_json::json!({ "connections": connections }))
    }

    fn handle_get_active(
        &self,
        host: &dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        let connection = host.active_connection_id().and_then(|id| {
            host.load_connections()
                .into_iter()
                .find(|config| config.id == id)
                .map(|config| to_public_connection(&config))
        });
        let value = match connection {
            Some(summary) => serde_json::to_value(summary).unwrap_or(Value::Null),
            None => Value::Null,
        };
        self.ok(request, serde_json::json!({ "connection": value }))
    }

    fn handle_command_invoke(
        &self,
        host: &dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        let input = match parse_command_invoke(request.payload.as_ref()) {
            Ok(input) => input,
            Err(error) => return self.fail(request, error),
        };
        if !is_wapp_command_allowed(&input.command) {
            return self.fail(
                request,
                BridgeApiError::permission(format!(
                    "Command '{}' is not permitted for extensions",
                    input.command
                )),
            );
        }
        host.audit_log(
            &self.options.wapp_id,
            "command.invoke",
            &format!("{} via {}", input.command, input.connection_id),
        );
        match host.execute_driver_command(
            &self.options.wapp_id,
            &input.connection_id,
            &input.command,
            input.args,
        ) {
            Ok(result) => self.ok(request, serde_json::json!({ "result": result })),
            Err(message) => {
                if looks_like_not_found(&message) {
                    self.fail(request, BridgeApiError::not_found(message))
                } else {
                    self.fail(
                        request,
                        BridgeApiError::internal(truncate_message(&message)),
                    )
                }
            }
        }
    }

    fn handle_storage(
        &mut self,
        host: &mut dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        let input = match parse_storage(&request.kind, request.payload.as_ref()) {
            Ok(input) => input,
            Err(error) => return self.fail(request, error),
        };
        let wapp_id = self.options.wapp_id.as_str();
        match request.kind.as_str() {
            "storage.get" => {
                let value = host.storage_get(wapp_id, &input.key).unwrap_or(Value::Null);
                self.ok(request, serde_json::json!({ "value": value }))
            }
            "storage.set" => {
                host.storage_set(wapp_id, &input.key, input.value);
                self.ok(request, serde_json::json!({}))
            }
            _ => {
                host.storage_remove(wapp_id, &input.key);
                self.ok(request, serde_json::json!({}))
            }
        }
    }

    fn handle_notify(
        &mut self,
        host: &mut dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        let input = match parse_notify(request.payload.as_ref()) {
            Ok(input) => input,
            Err(error) => return self.fail(request, error),
        };
        let now = host.now_ms();
        if let Some(last) = self.state.last_notify_ms {
            if now - last < self.options.notify_cooldown_ms {
                return self.fail(
                    request,
                    BridgeApiError::rate_limit(format!(
                        "notifications limited to one per {}ms",
                        self.options.notify_cooldown_ms
                    )),
                );
            }
        }
        self.state.last_notify_ms = Some(now);
        match host.show_notification(&input.title, input.body.as_deref()) {
            Ok(()) => self.ok(request, serde_json::json!({})),
            Err(message) => self.fail(
                request,
                BridgeApiError::internal(truncate_message(&message)),
            ),
        }
    }

    fn handle_i18n(
        &self,
        host: &dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        let key = match parse_i18n_key(request.payload.as_ref()) {
            Ok(key) => key,
            Err(error) => return self.fail(request, error),
        };
        let value = host
            .resolve_string(&self.options.wapp_id, &key, &self.options.locale)
            .map(Value::String)
            .unwrap_or(Value::Null);
        self.ok(request, serde_json::json!({ "key": key, "value": value }))
    }

    fn handle_session(
        &mut self,
        host: &mut dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> WappResponseEnvelope {
        match self.session_request(host, request) {
            Ok(value) => self.ok(request, value),
            Err(error) => self.fail(request, error),
        }
    }

    fn session_request(
        &mut self,
        host: &mut dyn WappBridgeHost,
        request: &WappRequestEnvelope,
    ) -> Result<Value, BridgeApiError> {
        if request.instance_nonce.as_deref() != Some(self.state.instance_nonce.as_str()) {
            return Err(BridgeApiError::permission(
                "Wapp instance nonce mismatch; handshake again",
            ));
        }
        if !self.granted(WappPermission::CommandInvoke) {
            return Err(BridgeApiError::permission(
                "session requires command:invoke",
            ));
        }
        let req_id = request.req_id.as_deref().unwrap_or("");
        let digest = serde_json::to_value(serde_json::json!([request.kind, request.payload]))
            .map(|value| value.to_string())
            .unwrap_or_default();
        if let Some(previous) = self.state.session_requests.get(req_id) {
            if previous.digest != digest {
                return Err(BridgeApiError::bad_request(
                    "requestId already used with different input",
                ));
            }
            return Ok(previous.result.clone());
        }
        if self.state.session_requests.len() >= SESSION_REQUEST_LIMIT {
            return Err(BridgeApiError::rate_limit(
                "instance request limit reached; reload the Wapp",
            ));
        }

        let generation = self.state.generation;
        if self.state.instance_id.is_none() {
            let instance_id = host
                .session_register(&self.options.wapp_id)
                .map_err(BridgeApiError::internal)?;
            if self.state.detached || self.state.generation != generation {
                host.session_end(&instance_id);
                return Err(BridgeApiError::internal("Wapp instance ended"));
            }
            self.state.instance_id = Some(instance_id);
        }
        let instance_id = self
            .state
            .instance_id
            .clone()
            .ok_or_else(|| BridgeApiError::internal("Wapp instance ended"))?;
        let payload = request
            .payload
            .as_ref()
            .and_then(Value::as_object)
            .ok_or_else(|| BridgeApiError::bad_request("session requires object input"))?;
        let mut wire = payload.clone();
        let action = request.kind.strip_prefix("session.").unwrap_or("");
        wire.insert("action".to_owned(), Value::String(action.to_owned()));
        wire.insert("instanceId".to_owned(), Value::String(instance_id));
        let result = host
            .session_invoke(&Value::Object(wire))
            .map_err(|message| {
                if looks_like_not_found(&message) {
                    BridgeApiError::not_found(message)
                } else {
                    BridgeApiError::internal(truncate_message(&message))
                }
            })?;
        self.state.session_requests.insert(
            req_id.to_owned(),
            SessionRequestRecord {
                digest,
                result: result.clone(),
            },
        );
        Ok(result)
    }

    pub fn on_frame_load(&mut self, host: &mut dyn WappBridgeHost) -> Vec<BridgeEffect> {
        self.state.generation = self.state.generation.saturating_add(1);
        self.state.instance_nonce = host.new_nonce();
        if let Some(instance_id) = self.state.instance_id.take() {
            host.session_end(&instance_id);
        }
        self.state.session_requests.clear();
        self.clear_timers()
    }

    pub fn detach(&mut self, host: &mut dyn WappBridgeHost) -> Vec<BridgeEffect> {
        self.state.detached = true;
        let mut effects = vec![
            BridgeEffect::RemoveFrameLoadListener,
            BridgeEffect::RemoveMessageListener,
        ];
        if let Some(instance_id) = self.state.instance_id.take() {
            host.session_end(&instance_id);
        }
        self.state.session_requests.clear();
        effects.extend(self.clear_timers());
        effects
    }

    fn clear_timers(&mut self) -> Vec<BridgeEffect> {
        let effects = self
            .state
            .timers
            .keys()
            .map(|token| BridgeEffect::ClearTimer { token: *token })
            .collect::<Vec<_>>();
        self.state.timers.clear();
        effects
    }

    pub fn on_timeout(
        &mut self,
        request: &WappRequestEnvelope,
        generation: u64,
    ) -> Vec<BridgeEffect> {
        if timeout_disposition(
            false,
            self.state.detached,
            self.state.generation,
            generation,
        ) == TimeoutDisposition::Respond
        {
            let response = err_envelope(
                &request.ch,
                request.req_id.as_deref(),
                BridgeErrorCode::Timeout,
                format!("handler timed out after {}ms", self.options.timeout_ms),
            );
            vec![self.respond(request, WappResponseEnvelope::Err(response))]
        } else {
            Vec::new()
        }
    }

    pub fn push_theme_snapshot(&self, host: &dyn WappBridgeHost) -> BridgeEffect {
        BridgeEffect::PostMessage(theme_apply_envelope(&host.theme_snapshot()))
    }
}

fn truncate_message(message: &str) -> String {
    if message.chars().count() > ERROR_MESSAGE_LIMIT_CHARS {
        let mut text = message
            .chars()
            .take(ERROR_MESSAGE_LIMIT_CHARS)
            .collect::<String>();
        text.push('…');
        text
    } else {
        message.to_owned()
    }
}
