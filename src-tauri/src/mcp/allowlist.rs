//! MCP connection allowlist — empty means deny-all (secure default).

/// Returns true when `connection_id` may be used by MCP tools/resources.
///
/// An empty `allowed` list denies every connection until explicitly allowlisted.
pub fn is_connection_allowed(connection_id: &str, allowed: &[String]) -> bool {
    !allowed.is_empty() && allowed.iter().any(|id| id == connection_id)
}

/// Returns `Ok(())` when the connection is allowlisted.
pub fn ensure_connection_allowed(connection_id: &str, allowed: &[String]) -> Result<(), String> {
    if is_connection_allowed(connection_id, allowed) {
        Ok(())
    } else if allowed.is_empty() {
        Err(format!(
            "Connection '{connection_id}' is blocked: MCP connection allowlist is empty (deny-all default). Add connections in Settings → MCP."
        ))
    } else {
        Err(format!(
            "Connection '{connection_id}' is not in the MCP connection allowlist"
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
    fn empty_allowlist_denies_all() {
        assert!(!is_connection_allowed("a", &[]));
        assert!(!is_connection_allowed("b", &[]));
        assert!(ensure_connection_allowed("a", &[]).is_err());
        let msg = ensure_connection_allowed("a", &[]).unwrap_err();
        assert!(msg.contains("deny-all"));
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
        let none = filter_connection_ids(["a", "b"], &[]);
        assert!(none.is_empty());
    }
}
