use crate::connection_types::DriverCapabilities;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapabilityAnswer {
    Unknown,
    Supported,
    Unsupported,
}

impl CapabilityAnswer {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Supported => "true",
            Self::Unsupported => "false",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionCapabilitiesEntry {
    pub db_session_id: String,
    pub capabilities: Option<DriverCapabilities>,
}

pub fn has_schema_level(capabilities: Option<&DriverCapabilities>) -> CapabilityAnswer {
    let Some(capabilities) = capabilities else {
        return CapabilityAnswer::Unknown;
    };
    if capabilities.has_schema_level {
        CapabilityAnswer::Supported
    } else {
        CapabilityAnswer::Unsupported
    }
}

pub fn supports_offset(capabilities: Option<&DriverCapabilities>) -> CapabilityAnswer {
    let Some(capabilities) = capabilities else {
        return CapabilityAnswer::Unknown;
    };
    if capabilities.supports_offset {
        CapabilityAnswer::Supported
    } else {
        CapabilityAnswer::Unsupported
    }
}

pub fn capabilities_for_db_session(
    db_session_id: Option<&str>,
    entries: &[SessionCapabilitiesEntry],
) -> Option<DriverCapabilities> {
    let wanted = db_session_id?;
    if wanted.is_empty() {
        return None;
    }
    for entry in entries {
        if entry.db_session_id == wanted {
            return entry.capabilities;
        }
    }
    None
}

pub fn relation_schema_for(
    capabilities: Option<&DriverCapabilities>,
    schema: Option<&str>,
) -> Option<String> {
    if has_schema_level(capabilities) == CapabilityAnswer::Unsupported {
        return None;
    }
    schema
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(has_schema: bool, offset: bool) -> DriverCapabilities {
        DriverCapabilities {
            supports_cancel_query: true,
            supports_query_execution_cancel: true,
            supports_explain: true,
            supports_streaming_results: true,
            supports_offset: offset,
            has_schema_level: has_schema,
        }
    }

    fn entry(session: &str, capabilities: Option<DriverCapabilities>) -> SessionCapabilitiesEntry {
        SessionCapabilitiesEntry {
            db_session_id: session.to_owned(),
            capabilities,
        }
    }

    #[test]
    fn schema_level_is_strict_true_or_unknown() {
        assert_eq!(has_schema_level(None), CapabilityAnswer::Unknown);
        assert_eq!(
            has_schema_level(Some(&caps(true, true))),
            CapabilityAnswer::Supported
        );
        assert_eq!(
            has_schema_level(Some(&caps(false, true))),
            CapabilityAnswer::Unsupported
        );
    }

    #[test]
    fn offset_is_false_checked_or_unknown() {
        assert_eq!(supports_offset(None), CapabilityAnswer::Unknown);
        assert_eq!(
            supports_offset(Some(&caps(false, false))),
            CapabilityAnswer::Unsupported
        );
        assert_eq!(
            supports_offset(Some(&caps(false, true))),
            CapabilityAnswer::Supported
        );
    }

    #[test]
    fn capability_answer_literals_are_pinned() {
        assert_eq!(CapabilityAnswer::Unknown.as_str(), "unknown");
        assert_eq!(CapabilityAnswer::Supported.as_str(), "true");
        assert_eq!(CapabilityAnswer::Unsupported.as_str(), "false");
    }

    #[test]
    fn session_lookup_needs_a_nonempty_handle_and_a_stored_entry() {
        let entries = vec![entry("s1", Some(caps(true, true))), entry("s2", None)];
        assert_eq!(capabilities_for_db_session(None, &entries), None);
        assert_eq!(capabilities_for_db_session(Some(""), &entries), None);
        assert_eq!(capabilities_for_db_session(Some("missing"), &entries), None);
        assert_eq!(
            capabilities_for_db_session(Some("s1"), &entries),
            Some(caps(true, true))
        );
        assert_eq!(capabilities_for_db_session(Some("s2"), &entries), None);
    }

    #[test]
    fn session_lookup_returns_the_first_matching_entry() {
        let entries = vec![
            entry("dup", Some(caps(false, false))),
            entry("dup", Some(caps(true, true))),
        ];
        assert_eq!(
            capabilities_for_db_session(Some("dup"), &entries),
            Some(caps(false, false))
        );
        assert_eq!(capabilities_for_db_session(Some("dup"), &[]), None);
    }

    #[test]
    fn relation_schema_drops_only_for_known_schema_less_drivers() {
        let schema_less = caps(false, true);
        let schema_aware = caps(true, true);
        assert_eq!(
            relation_schema_for(Some(&schema_less), Some("public")),
            None
        );
        assert_eq!(
            relation_schema_for(Some(&schema_aware), Some("  public  ")),
            Some("public".to_owned())
        );
        assert_eq!(
            relation_schema_for(None, Some("  public  ")),
            Some("public".to_owned())
        );
        assert_eq!(relation_schema_for(Some(&schema_aware), None), None);
        assert_eq!(relation_schema_for(Some(&schema_aware), Some("   ")), None);
        assert_eq!(relation_schema_for(None, None), None);
        assert_eq!(relation_schema_for(Some(&schema_less), Some("   ")), None);
    }
}
