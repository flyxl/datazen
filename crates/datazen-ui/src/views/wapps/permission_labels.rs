//! Port of `packages/frontend/src/windows/wapps/permissionLabels.ts`
//! (T484) — the human-readable explanations shown as permission
//! badge tooltips.
//!
//! React attaches the explanation as `title`; the Native badge attaches
//! it as Rune help metadata while retaining the visible permission token.

/// `PERMISSION_LABELS[permission]` — `None` for anything but the
/// closed five (the call sites fall back to the permission string
/// itself, React's `?? perm`).
pub fn wapp_permission_label(permission: &str) -> Option<&'static str> {
    match permission {
        "context:connections" => Some(
            "Read the connection list (names and types only, never credentials)",
        ),
        "session:stateful" => Some(
            "Open dedicated database sessions after explicit approval; retain session state until closed or expired",
        ),
        "command:invoke" => Some("Run database commands through the host Driver Command API"),
        "storage:local" => Some("Keep a small private key-value store on this machine"),
        "ui:notify" => Some("Show notifications via the host (rate limited)"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_five_permissions_carry_their_explanations() {
        assert_eq!(
            wapp_permission_label("context:connections"),
            Some("Read the connection list (names and types only, never credentials)")
        );
        assert_eq!(
            wapp_permission_label("session:stateful"),
            Some(
                "Open dedicated database sessions after explicit approval; retain session \
                 state until closed or expired"
            )
        );
        assert_eq!(
            wapp_permission_label("command:invoke"),
            Some("Run database commands through the host Driver Command API")
        );
        assert_eq!(
            wapp_permission_label("storage:local"),
            Some("Keep a small private key-value store on this machine")
        );
        assert_eq!(
            wapp_permission_label("ui:notify"),
            Some("Show notifications via the host (rate limited)")
        );
    }

    #[test]
    fn anything_else_falls_back_to_the_permission_string() {
        // React's `PERMISSION_LABELS[perm] ?? perm`.
        assert_eq!(wapp_permission_label("workspace:admin"), None);
        assert_eq!(wapp_permission_label(""), None);
        assert_eq!(wapp_permission_label("context:connection"), None, "the key is exact");
        // The fallback the call sites apply.
        let perm = "workspace:admin";
        let label = wapp_permission_label(perm).unwrap_or(perm);
        assert_eq!(label, "workspace:admin");
    }
}
