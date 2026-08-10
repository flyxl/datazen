//! MCP connection allowlist — empty means all saved connections are exposed.

/// Returns true when `config_id` may be used by MCP tools/resources.
///
/// An empty `allowed` list means unrestricted (all connections).
pub fn is_connection_allowed(config_id: &str, allowed: &[String]) -> bool {
    allowed.is_empty() || allowed.iter().any(|id| id == config_id)
}

/// Returns `Ok(())` when the connection is allowlisted (or allowlist is empty).
pub fn ensure_connection_allowed(config_id: &str, allowed: &[String]) -> Result<(), String> {
    if is_connection_allowed(config_id, allowed) {
        Ok(())
    } else {
        Err(format!(
            "Connection '{config_id}' is not in the MCP connection allowlist"
        ))
    }
}

/// Keep only connections whose id is allowlisted.
pub fn filter_connection_ids<'a>(
    ids: impl IntoIterator<Item = &'a str>,
    allowed: &'a [String],
) -> Vec<&'a str> {
    ids.into_iter()
        .filter(|id| is_connection_allowed(id, allowed))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_allowlist_allows_all() {
        assert!(is_connection_allowed("a", &[]));
        assert!(is_connection_allowed("b", &[]));
        assert!(ensure_connection_allowed("a", &[]).is_ok());
    }

    #[test]
    fn non_empty_allowlist_filters() {
        let allowed = vec!["prod".into(), "staging".into()];
        assert!(is_connection_allowed("prod", &allowed));
        assert!(!is_connection_allowed("dev", &allowed));
        assert!(ensure_connection_allowed("dev", &allowed).is_err());
        assert!(ensure_connection_allowed("staging", &allowed).is_ok());
    }

    #[test]
    fn filter_connection_ids_respects_allowlist() {
        let allowed = vec!["a".into()];
        let kept = filter_connection_ids(["a", "b", "c"], &allowed);
        assert_eq!(kept, vec!["a"]);
        let all = filter_connection_ids(["a", "b"], &[]);
        assert_eq!(all, vec!["a", "b"]);
    }
}
