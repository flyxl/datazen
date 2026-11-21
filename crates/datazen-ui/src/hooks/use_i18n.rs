//! Port of `packages/frontend/src/hooks/useI18n.ts` (T341).
//!
//! Host-side alias for the single @datazen/ui i18n hook. Kept as the
//! canonical import path for the 300+ host/driver consumer files.
//! There is deliberately no host-local implementation — lookup,
//! fallback and interpolation live only in `packages/ui/src/i18n.ts`;
//! on the Rust side that single home is [`crate::i18n`] (the
//! build-time zh-CN table, the key fallback). This module is the
//! alias face: the hook's `{ t, language }` answer plus the
//! `I18nParams` interpolation the React `t(key, params)` performs.
//!
//! The `import '../locales'` side effect (register the eager host
//! dictionaries) is the build script's `include!` — the table is
//! compiled in, never registered at runtime.
//!
//! Known deltas: the React hook re-renders on locale change
//! (`useSyncExternalStore`); the Rust table is a build-time constant
//! and the "hook" is a plain value — a locale change is a rebuild,
//! so there is no subscription to alias.

use crate::i18n;

/// `I18nParams`: the `{param}` tokens `t` interpolates.
pub(crate) type I18nParams<'a> = &'a [(&'a str, &'a str)];

/// `useI18n()`'s answer: the translate function and the active
/// language. The language is the compiled locale (zh-CN), the table's
/// own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct UseI18n {
    pub(crate) language: &'static str,
}

impl UseI18n {
    /// `useI18n()` — the single hook every host/driver consumer
    /// imports.
    pub(crate) fn new() -> Self {
        Self { language: "zh-CN" }
    }

    /// `t(key)` — the registered text, or the key itself when the
    /// table lacks it (an empty string is a real translation, not a
    /// miss).
    pub(crate) fn t<'k>(&self, key: &'k str) -> &'k str {
        i18n::t(key)
    }

    /// `t(key, params)` — the text with `{param}` tokens interpolated
    /// (`formatMessage`: an unknown token stays verbatim).
    pub(crate) fn t_with_params(&self, key: &str, params: I18nParams<'_>) -> String {
        format_message(i18n::t(key), params)
    }
}

impl Default for UseI18n {
    fn default() -> Self {
        Self::new()
    }
}

/// `formatMessage`: interpolate `{param}` tokens in a template; a
/// token without a value stays verbatim (the React `match` return).
pub(crate) fn format_message(template: &str, params: I18nParams<'_>) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        // `/\{(\w+)\}/`: word characters only, then the closing brace.
        let close = match after.find('}') {
            Some(close) => close,
            None => {
                // No closing brace at all: the rest is literal.
                out.push_str(&rest[open..]);
                return out;
            }
        };
        let name = &after[..close];
        if !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_') {
            match params.iter().find(|(k, _)| *k == name) {
                Some((_, value)) => out.push_str(value),
                // An unknown token renders verbatim (braces included).
                None => {
                    out.push('{');
                    out.push_str(name);
                    out.push('}');
                }
            }
        } else {
            // Not a `{word}` token: the whole span is literal.
            out.push('{');
            out.push_str(name);
            out.push('}');
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hook_face_is_the_single_table() {
        let hook = UseI18n::new();
        assert_eq!(hook.language, "zh-CN");
        assert_eq!(hook.t("common.save"), "保存");
        assert_eq!(hook.t("common.cancel"), "取消");
        // The key fallback: a missing entry shows up in the UI.
        assert_eq!(hook.t("no.such.key"), "no.such.key");
    }

    #[test]
    fn params_interpolate_and_unknown_tokens_stay_verbatim() {
        let hook = UseI18n::new();
        // A template with tokens (the key fallback interpolates too —
        // the React `formatMessage(key, params)` last-resort branch).
        assert_eq!(
            hook.t_with_params("no.such.{name}", &[("name", "value")]),
            "no.such.value"
        );
        assert_eq!(format_message("{a}-{b}", &[("a", "1"), ("b", "2")]), "1-2");
        // Unknown token: verbatim.
        assert_eq!(format_message("{a} {c}", &[("a", "1")]), "1 {c}");
        // Non-word names are not tokens.
        assert_eq!(format_message("{a b} {1a}", &[("a b", "x")]), "{a b} {1a}");
        // Empty name is not a token.
        assert_eq!(format_message("{}", &[]), "{}");
        // Unbalanced braces are literal.
        assert_eq!(format_message("a { b", &[]), "a { b");
        assert_eq!(format_message("} a", &[]), "} a");
        // A value may itself contain braces.
        assert_eq!(format_message("{a}", &[("a", "{x}")]), "{x}");
    }
}
