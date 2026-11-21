use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const CHANNEL_NAME: &str = "datazen-bus";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CrossWindowMessage {
    pub event: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrossWindowRail {
    Unavailable,
}

impl CrossWindowRail {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unavailable => "unavailable",
        }
    }
}

pub fn cross_window_rail() -> CrossWindowRail {
    CrossWindowRail::Unavailable
}

pub fn make_message(event: &str, payload: Option<Value>) -> CrossWindowMessage {
    CrossWindowMessage {
        event: event.to_owned(),
        payload,
    }
}

pub fn matches_event(data: Option<&Value>, event: &str) -> bool {
    data.and_then(Value::as_object)
        .and_then(|object| object.get("event"))
        .and_then(Value::as_str)
        == Some(event)
}

pub fn payload_of(data: &Value) -> Option<&Value> {
    data.as_object()?.get("payload")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_channel_name_is_the_react_literal() {
        assert_eq!(CHANNEL_NAME, "datazen-bus");
    }

    #[test]
    fn a_message_carries_event_and_payload() {
        let message = make_message("orders:changed", Some(json!({"id": 7})));
        assert_eq!(message.event, "orders:changed");
        assert_eq!(message.payload, Some(json!({"id": 7})));
    }

    #[test]
    fn a_missing_payload_is_omitted_from_the_wire() {
        let message = make_message("ping", None);
        assert_eq!(
            serde_json::to_value(&message).ok(),
            Some(json!({"event": "ping"}))
        );
    }

    #[test]
    fn a_message_round_trips_through_json() {
        let message = make_message("sync", Some(json!([1, "two", null, true])));
        let encoded = serde_json::to_string(&message).unwrap_or_default();
        let decoded: CrossWindowMessage =
            serde_json::from_str(&encoded).unwrap_or_else(|_| make_message("decode-failed", None));
        assert_eq!(decoded, message);
    }

    #[test]
    fn the_wire_keys_are_event_and_payload() {
        let value = serde_json::to_value(make_message("a", Some(json!(1)))).ok();
        assert_eq!(value, Some(json!({"event": "a", "payload": 1})));
    }

    #[test]
    fn an_event_filter_matches_only_exact_string_events() {
        let data = json!({"event": "mcp:approval-requested", "payload": {"id": 1}});
        assert!(matches_event(Some(&data), "mcp:approval-requested"));
        assert!(!matches_event(Some(&data), "mcp:approval-requested "));
        assert!(!matches_event(Some(&data), "mcp:approval"));
        assert!(!matches_event(Some(&data), "approval-requested"));
        assert!(!matches_event(Some(&data), "MCP:APPROVAL-REQUESTED"));
        assert!(!matches_event(Some(&data), "other"));
    }

    #[test]
    fn malformed_or_absent_data_never_matches_an_event() {
        assert!(!matches_event(None, "x"));
        assert!(!matches_event(Some(&Value::Null), "x"));
        assert!(!matches_event(Some(&json!("x")), "x"));
        assert!(!matches_event(Some(&json!([])), "x"));
        assert!(!matches_event(Some(&json!({})), "x"));
        assert!(!matches_event(Some(&json!({"event": 7})), "x"));
        assert!(!matches_event(Some(&json!({"event": null})), "x"));
    }

    #[test]
    fn a_payload_is_read_only_from_an_object_envelope() {
        assert_eq!(payload_of(&json!({"event": "x", "payload": 3})), Some(&json!(3)));
        assert_eq!(
            payload_of(&json!({"event": "x", "payload": null})),
            Some(&Value::Null)
        );
        assert_eq!(payload_of(&json!({"event": "x"})), None);
        assert_eq!(payload_of(&Value::Null), None);
        assert_eq!(payload_of(&json!([])), None);
    }

    #[test]
    fn the_cross_window_rail_is_reported_as_unavailable() {
        assert_eq!(cross_window_rail(), CrossWindowRail::Unavailable);
        assert_eq!(cross_window_rail().as_str(), "unavailable");
    }

    #[test]
    fn message_construction_is_pure_and_independent() {
        let first = make_message("a", Some(json!(1)));
        let second = make_message("a", Some(json!(2)));
        assert_eq!(first.event, second.event);
        assert_ne!(first.payload, second.payload);
        assert_eq!(first, make_message("a", Some(json!(1))));
    }
}
