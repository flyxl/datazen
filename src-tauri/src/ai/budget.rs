//! Token budget constants for AI / MCP schema context injection.
//!
//! Budgets are used by `SchemaContextBuilder` and `SchemaContextPipeline` to
//! limit the amount of schema DDL injected into prompt context.

/// Budget for pinned-table DDL in the schema pipeline.
pub const PINNED_DDL: usize = 4000;
/// Budget when falling back to a broader schema DDL scan.
pub const FALLBACK_DDL: usize = 4000;
/// Budget for diagnose-error schema context.
pub const DIAGNOSE: usize = 3000;
/// Budget for schema-documentation selective context.
pub const SCHEMA_DOC: usize = 8000;
/// Budget for MCP schema resource reads.
pub const MCP_RESOURCE: usize = 8000;

/// Maximum context budget cap (tokens). Even with large model windows, we
/// never inject more than this into schema context to leave room for the
/// actual prompt and conversation.
pub const MAX_CONTEXT_BUDGET: usize = 16_000;

/// Minimum context budget floor (tokens).
pub const MIN_CONTEXT_BUDGET: usize = 2_000;

/// Compute a dynamic schema-context budget based on the model's context window.
///
/// Formula: `min(model_window × 0.15, MAX_CONTEXT_BUDGET)`, clamped to
/// `[MIN_CONTEXT_BUDGET, MAX_CONTEXT_BUDGET]`.
///
/// If `model_window` is `None` or zero, falls back to `PINNED_DDL + FALLBACK_DDL`.
pub fn dynamic_budget(model_window: Option<u32>) -> usize {
    let window = model_window.unwrap_or(0) as usize;
    if window == 0 {
        return PINNED_DDL + FALLBACK_DDL;
    }
    let raw = (window as f64 * 0.15) as usize;
    raw.clamp(MIN_CONTEXT_BUDGET, MAX_CONTEXT_BUDGET)
}

/// Compute pinned and fallback budgets from a dynamic total budget.
///
/// Splits the total budget 60/40 between pinned (user-selected tables) and
/// fallback (auto-selected tables).
pub fn split_budget(total: usize) -> (usize, usize) {
    let pinned = (total as f64 * 0.6) as usize;
    let fallback = total - pinned;
    (pinned, fallback)
}

/// Token estimation for a string.
///
/// Uses a CJK-aware heuristic: CJK characters count as ~1.6 tokens each,
/// ASCII characters count as ~0.3 tokens each (approximately 3.3 chars per
/// token for English text).
pub fn estimate_tokens(text: &str) -> usize {
    let mut cjk_count = 0usize;
    let mut ascii_count = 0usize;

    for ch in text.chars() {
        if ch.is_ascii() {
            ascii_count += 1;
        } else if is_cjk(ch) {
            cjk_count += 1;
        } else {
            // Other unicode: treat as ~1 token per character
            ascii_count += 1;
        }
    }

    let cjk_tokens = (cjk_count as f64 * 1.6) as usize;
    let ascii_tokens = (ascii_count as f64 * 0.3) as usize;
    cjk_tokens + ascii_tokens
}

/// Check if a character is CJK (Chinese, Japanese, Korean).
fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Unified Ideographs Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{FF00}'..='\u{FFEF}' // Fullwidth Forms
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dynamic_budget_with_window() {
        // 8192 window → 8192 * 0.15 = 1228.8 → 1228 (clamped to MIN)
        assert_eq!(dynamic_budget(Some(8192)), MIN_CONTEXT_BUDGET);

        // 128k window → 128000 * 0.15 = 19200 → clamped to MAX
        assert_eq!(dynamic_budget(Some(128_000)), MAX_CONTEXT_BUDGET);

        // 32k window → 32000 * 0.15 = 4800 → within range
        assert_eq!(dynamic_budget(Some(32_000)), 4800);
    }

    #[test]
    fn dynamic_budget_zero_fallback() {
        assert_eq!(dynamic_budget(None), PINNED_DDL + FALLBACK_DDL);
        assert_eq!(dynamic_budget(Some(0)), PINNED_DDL + FALLBACK_DDL);
    }

    #[test]
    fn split_budget_ratio() {
        let (pinned, fallback) = split_budget(10_000);
        assert_eq!(pinned, 6000);
        assert_eq!(fallback, 4000);
        assert_eq!(pinned + fallback, 10_000);
    }

    #[test]
    fn estimate_tokens_english() {
        // "Hello World" = 11 chars ASCII → 11 * 0.3 = 3.3 → 3
        let tokens = estimate_tokens("Hello World");
        assert!(tokens >= 3 && tokens <= 4, "got {tokens}");
    }

    #[test]
    fn estimate_tokens_cjk() {
        // "你好世界" = 4 CJK → 4 * 1.6 = 6.4 → 6
        let tokens = estimate_tokens("你好世界");
        assert!(tokens >= 6 && tokens <= 7, "got {tokens}");
    }

    #[test]
    fn estimate_tokens_mixed() {
        let text = "Hello 你好";
        let tokens = estimate_tokens(text);
        // 5 ASCII * 0.3 + 2 CJK * 1.6 = 1.5 + 3.2 = 4.7 → ~4-5
        assert!(tokens >= 4 && tokens <= 6, "got {tokens}");
    }
}
