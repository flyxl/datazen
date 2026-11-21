pub const DESTRUCTIVE_CONFIRM_TOKEN: &str = "DEPLOY";

pub fn can_confirm_destructive_deploy(typed: &str, has_destructive: bool) -> bool {
    if !has_destructive {
        return true;
    }
    typed.trim() == DESTRUCTIVE_CONFIRM_TOKEN
}

pub struct CanRunDeployOptions<'a> {
    pub has_destructive: bool,
    pub confirm_text: &'a str,
    pub require_rollback: bool,
    pub rollback_complete: bool,
    pub statement_count: usize,
}

pub fn can_run_deploy(opts: &CanRunDeployOptions<'_>) -> bool {
    if opts.statement_count == 0 {
        return false;
    }
    if opts.require_rollback && !opts.rollback_complete {
        return false;
    }
    can_confirm_destructive_deploy(opts.confirm_text, opts.has_destructive)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(confirm: &str) -> CanRunDeployOptions<'_> {
        CanRunDeployOptions {
            has_destructive: false,
            confirm_text: confirm,
            require_rollback: false,
            rollback_complete: false,
            statement_count: 3,
        }
    }

    #[test]
    fn the_confirm_token_is_exact() {
        assert_eq!(DESTRUCTIVE_CONFIRM_TOKEN, "DEPLOY");
    }

    #[test]
    fn non_destructive_deploys_need_no_confirmation_text() {
        assert!(can_confirm_destructive_deploy("", false));
        assert!(can_confirm_destructive_deploy("anything", false));
    }

    #[test]
    fn destructive_deploys_need_the_trimmed_exact_token() {
        assert!(can_confirm_destructive_deploy("DEPLOY", true));
        assert!(can_confirm_destructive_deploy("  DEPLOY  ", true));
        assert!(!can_confirm_destructive_deploy("deploy", true));
        assert!(!can_confirm_destructive_deploy("Deploy", true));
        assert!(!can_confirm_destructive_deploy("", true));
        assert!(!can_confirm_destructive_deploy("DEPLOY NOW", true));
    }

    #[test]
    fn an_empty_statement_list_never_runs() {
        let mut input = opts("");
        input.statement_count = 0;
        assert!(!can_run_deploy(&input));
        input.confirm_text = "DEPLOY";
        input.rollback_complete = true;
        assert!(!can_run_deploy(&input));
    }

    #[test]
    fn a_required_rollback_must_be_complete() {
        let mut input = opts("");
        input.require_rollback = true;
        input.rollback_complete = false;
        assert!(!can_run_deploy(&input));
        input.rollback_complete = true;
        assert!(can_run_deploy(&input));
    }

    #[test]
    fn all_gates_together() {
        let mut input = opts("  DEPLOY  ");
        input.has_destructive = true;
        input.require_rollback = true;
        input.rollback_complete = true;
        assert!(can_run_deploy(&input));
        input.rollback_complete = false;
        assert!(!can_run_deploy(&input));
        input.rollback_complete = true;
        input.confirm_text = "wrong";
        assert!(!can_run_deploy(&input));
    }

    #[test]
    fn an_optional_rollback_does_not_gate_the_run() {
        let input = opts("");
        assert!(can_run_deploy(&input));
    }
}
