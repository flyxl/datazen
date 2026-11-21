fn contains_with_whitespace(text: &str, left: &str, right: &str) -> bool {
    let mut start = 0usize;
    while let Some(offset) = text[start..].find(left) {
        let mut cursor = start + offset + left.len();
        while cursor < text.len() && text.as_bytes()[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if text[cursor..].starts_with(right) {
            return true;
        }
        start += offset + 1;
    }
    false
}

pub fn is_pipeline_budget_interception(message: Option<&str>) -> bool {
    let Some(message) = message else {
        return false;
    };
    let lower = message.to_lowercase();
    contains_with_whitespace(&lower, "pipeline", "budget")
        || contains_with_whitespace(&lower, "budget", "exceed")
        || contains_with_whitespace(&lower, "8", "mib")
        || lower.contains("8388608")
}

pub fn is_plan_consumed_rejection(message: Option<&str>) -> bool {
    let Some(message) = message else {
        return false;
    };
    let lower = message.to_lowercase();
    contains_with_whitespace(&lower, "already", "consumed")
        || lower.contains("planalreadyconsumed")
        || lower.contains("re-review the migration to mint a new plan")
}

pub fn is_backend_scope_rejection(message: Option<&str>) -> bool {
    let Some(message) = message else {
        return false;
    };
    let lower = message.to_lowercase();
    contains_with_whitespace(&lower, "backend", "scope")
        || lower.contains("local-desktop-backend")
        || lower.contains("scope")
}
