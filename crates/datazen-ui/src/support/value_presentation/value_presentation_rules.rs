//! Port of `packages/frontend/src/lib/valuePresentation/valuePresentationRules.ts`.
//!
//! The global rules model of the timestamp presentation (issue-32 T08,
//! design §5, PRD §3.1/§4): a rule is name + enabled + match conditions
//! (all present conditions AND together; at least one required) + a full
//! `TimestampOptions`. Rules sit between column overrides and the global
//! default; the first matching rule freezes the column's interpretation.
//!
//! # What maps where
//!
//! - **The restricted-regex validator ports verbatim.** The hand-written
//!   recursive-descent parser (quantifier/alternation/nullable flags per
//!   node, rejecting backreferences, lookaround, `\p`/`\u`/`\x` escapes,
//!   nested quantifiers, quantified alternation, quantified-nullable
//!   groups, unclosed groups/classes and trailing backslashes) is the same
//!   algorithm over bytes instead of UTF-16 units — error *offsets* are
//!   byte offsets (the React tests never pin offset text).
//! - **`new RegExp(source, flags).test(value)` → the `regex` crate.**
//!   Matching stays an unanchored substring search; the `i` flag maps to
//!   `case_insensitive(true)`. The crate's engine is non-backtracking
//!   (linear time), which is strictly *safer* than the JS engine the
//!   restricted subset was designed to fence in — the subset is still
//!   enforced verbatim because the persisted `source` must stay
//!   portable between the two runtimes.
//! - **Two escape spellings are normalized before compiling** (compile
//!   adapter, [`normalize_pattern_for_engine`]): JS `\0` (NUL) becomes
//!   `\x00`, and `\b` inside a character class (JS backspace) becomes
//!   `\x08` — the `regex` crate has no octal escapes and no in-class
//!   `\b`. Both rewrites are position-aware (escape pairs consumed
//!   together, leading-`]` literal handling) and semantics-preserving.
//!   `[\B]` stays a compile error in both engines, matching JS.
//! - **Trim semantics**: `.trim()` is Unicode-aware in both; the
//!   "contains whitespace" check uses `char::is_whitespace` (JS `\s` also
//!   matches U+FEFF — a documented one-codepoint delta, unreachable from
//!   the identifier inputs the editor produces).
//! - **`Object.freeze` is elided** (ownership); canonical rules serialize
//!   to React's exact JSON (the `match` key keeps its name via a serde
//!   rename).
//!
//! # Known deltas (type-level)
//!
//! Non-object / non-string runtime inputs are representable here as
//! `serde_json::Value` variants, so every garbage shape the React tests
//! feed still produces the same pathed problems.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::support::value_presentation::timestamp_config::{
    TimestampOptions, TimestampOptionsProblem, TimestampOptionsValidation,
    validate_timestamp_options,
};

/// Rule count cap (design §5: count and input length both capped).
pub(crate) const MAX_VALUE_PRESENTATION_RULES: usize = 20;
/// Per-pattern (exact value or regex source) length cap in characters.
pub(crate) const MAX_PATTERN_LENGTH: usize = 128;
/// Rule-name length cap in characters.
pub(crate) const MAX_RULE_NAME_LENGTH: usize = 64;
/// Identifier-class exact value length cap.
pub(crate) const MAX_MATCH_VALUE_LENGTH: usize = 128;
/// Upper bound of `{n,m}` repetition counts.
const MAX_REPETITION_COUNT: u64 = 100;

// ---------------------------------------------------------------------------
// Model
// ---------------------------------------------------------------------------

/// Name pattern: an exact value (whole-string equality) or a restricted
/// regex. Only the source and flags persist — never a compiled object.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(crate) enum ValuePresentationNamePattern {
    Exact {
        value: String,
    },
    Regex {
        source: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        flags: Option<String>,
    },
}

/// A rule's match conditions: every present field ANDs; at least one
/// required. Table-level conditions demand the source field to exist.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ValuePresentationRuleMatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) connection_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) database: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) namespace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) table: Option<ValuePresentationNamePattern>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) column: Option<ValuePresentationNamePattern>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) database_type_name: Option<ValuePresentationNamePattern>,
}

/// One global rule: name, enabled switch, match conditions, options.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ValuePresentationRule {
    pub(crate) name: String,
    pub(crate) enabled: bool,
    #[serde(rename = "match")]
    pub(crate) r#match: ValuePresentationRuleMatch,
    pub(crate) options: TimestampOptions,
}

/// The column context a rule matches against; missing table-level fields
/// can never satisfy table-level conditions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValuePresentationRuleContext<'a> {
    pub(crate) connection_id: Option<&'a str>,
    pub(crate) database: Option<&'a str>,
    pub(crate) namespace: Option<&'a str>,
    pub(crate) table: Option<&'a str>,
    pub(crate) column: &'a str,
    pub(crate) database_type_name: Option<&'a str>,
}

/// Pathed validation problem (same shape as T02's).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ValuePresentationRuleProblem {
    pub(crate) path: String,
    pub(crate) message: String,
}

impl ValuePresentationRuleProblem {
    fn new(path: &str, message: &str) -> Self {
        Self { path: path.to_string(), message: message.to_string() }
    }
}

/// `validate_value_presentation_rule`'s result.
#[derive(Debug, PartialEq)]
pub(crate) enum ValuePresentationRuleValidation {
    Ok(Box<ValuePresentationRule>),
    Err(Vec<ValuePresentationRuleProblem>),
}

// ---------------------------------------------------------------------------
// Identifier text reading
// ---------------------------------------------------------------------------

/// Identifier-class text: trimmed, non-empty, no inner whitespace, capped.
fn read_identifier_value(
    value: Option<&serde_json::Value>,
    path: &str,
    problems: &mut Vec<ValuePresentationRuleProblem>,
) -> Option<String> {
    let text = match value {
        Some(serde_json::Value::String(text)) => text,
        _ => {
            problems.push(ValuePresentationRuleProblem::new(path, "expected a string"));
            return None;
        }
    };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        problems.push(ValuePresentationRuleProblem::new(path, "must not be empty"));
        return None;
    }
    if trimmed.chars().any(char::is_whitespace) {
        problems.push(ValuePresentationRuleProblem::new(path, "must not contain whitespace"));
        return None;
    }
    if trimmed.chars().count() > MAX_MATCH_VALUE_LENGTH {
        problems.push(ValuePresentationRuleProblem::new(
            path,
            &format!("must be at most {MAX_MATCH_VALUE_LENGTH} characters"),
        ));
        return None;
    }
    Some(trimmed.to_string())
}

// ---------------------------------------------------------------------------
// Restricted regex: hand-validated expression subset
// ---------------------------------------------------------------------------

/// Per-node analysis flags the parser threads through.
#[derive(Clone, Copy, Debug)]
struct PatternNodeFlags {
    has_quantifier: bool,
    has_alternation: bool,
    nullable: bool,
}

const EMPTY_FLAGS: PatternNodeFlags =
    PatternNodeFlags { has_quantifier: false, has_alternation: false, nullable: true };

fn merge_sequence_flags(terms: &[PatternNodeFlags]) -> PatternNodeFlags {
    PatternNodeFlags {
        has_quantifier: terms.iter().any(|t| t.has_quantifier),
        has_alternation: terms.iter().any(|t| t.has_alternation),
        nullable: terms.iter().all(|t| t.nullable),
    }
}

/// The restricted-pattern parser (React `RestrictedPatternParser`): a
/// cursor walker that accepts only the complexity-bounded subset and
/// analyses quantifier / alternation / nullability as it goes.
struct RestrictedPatternParser<'a> {
    source: &'a [u8],
    pos: usize,
}

impl<'a> RestrictedPatternParser<'a> {
    fn new(source: &'a str) -> Self {
        Self { source: source.as_bytes(), pos: 0 }
    }

    fn parse(&mut self) -> Result<PatternNodeFlags, String> {
        let flags = self.parse_alternation(false)?;
        if self.pos != self.source.len() {
            return Err(format!(
                "unexpected character {:?} at offset {}",
                self.source.get(self.pos).map(|b| *b as char),
                self.pos
            ));
        }
        Ok(flags)
    }

    /// alternation: branch ('|' branch)*; `in_group` selects the ')' end.
    fn parse_alternation(&mut self, in_group: bool) -> Result<PatternNodeFlags, String> {
        let mut branches = vec![self.parse_sequence(in_group)?];
        while self.peek() == Some(b'|') {
            self.pos += 1;
            branches.push(self.parse_sequence(in_group)?);
        }
        if in_group && self.peek() != Some(b')') {
            return Err("unclosed group".to_string());
        }
        Ok(PatternNodeFlags {
            has_quantifier: branches.iter().any(|b| b.has_quantifier),
            has_alternation: branches.len() > 1 || branches.iter().any(|b| b.has_alternation),
            nullable: branches.iter().any(|b| b.nullable),
        })
    }

    /// sequence: term*, stopping at '|' / ')' / end.
    fn parse_sequence(&mut self, in_group: bool) -> Result<PatternNodeFlags, String> {
        let mut terms: Vec<PatternNodeFlags> = Vec::new();
        loop {
            match self.peek() {
                None => break,
                Some(b'|') => break,
                Some(b')') if in_group => break,
                Some(b')') => return Err("unmatched closing parenthesis".to_string()),
                _ => terms.push(self.parse_term()?),
            }
        }
        if terms.is_empty() {
            Ok(EMPTY_FLAGS)
        } else {
            Ok(merge_sequence_flags(&terms))
        }
    }

    /// term: atom + optional quantifier.
    fn parse_term(&mut self) -> Result<PatternNodeFlags, String> {
        let atom = self.parse_atom()?;
        let Some(min) = self.parse_quantifier()? else {
            return Ok(atom);
        };
        // Quantified quantifiers / alternation groups / nullable groups are
        // all rejected (see the module header's subset description).
        if atom.has_quantifier {
            return Err("nested quantifier is not allowed".to_string());
        }
        if atom.has_alternation {
            return Err("a quantified group must not contain alternation".to_string());
        }
        if atom.nullable {
            return Err("a quantified group must not match the empty string".to_string());
        }
        Ok(PatternNodeFlags {
            has_quantifier: true,
            has_alternation: atom.has_alternation,
            nullable: atom.nullable || min == 0,
        })
    }

    /// Quantifier `*` `+` `?` `{n}` `{n,}` `{n,m}` with lazy `?` suffix.
    /// Returns None when there is no quantifier. A `{` that is not a valid
    /// repetition is left for the atom parser (literal).
    fn parse_quantifier(&mut self) -> Result<Option<u64>, String> {
        match self.peek() {
            Some(b'*') | Some(b'+') | Some(b'?') => {
                let ch = self.peek();
                self.pos += 1;
                self.consume_lazy_marker();
                Ok(Some(match ch {
                    Some(b'+') => 1,
                    _ => 0,
                }))
            }
            Some(b'{') => {
                let Some((min, max, width)) = self.peek_repetition() else {
                    return Ok(None);
                };
                if min > MAX_REPETITION_COUNT
                    || max.is_some_and(|max| max > MAX_REPETITION_COUNT)
                {
                    return Err(format!(
                        "repetition count must be at most {MAX_REPETITION_COUNT}"
                    ));
                }
                if max.is_some_and(|max| min > max) {
                    return Err("repetition minimum must not exceed its maximum".to_string());
                }
                self.pos += width;
                self.consume_lazy_marker();
                Ok(Some(min))
            }
            _ => Ok(None),
        }
    }

    /// `^\{(\d+)(,(\d*))?\}` at the cursor → (min, max, consumed width).
    fn peek_repetition(&self) -> Option<(u64, Option<u64>, usize)> {
        let bytes = &self.source[self.pos..];
        let mut i = 0;
        if bytes.first() != Some(&b'{') {
            return None;
        }
        i += 1;
        let min_digits_start = i;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i == min_digits_start {
            return None; // `{,m}` is not a repetition — a literal for the atom parser
        }
        let min: u64 = std::str::from_utf8(&bytes[min_digits_start..i])
            .ok()?
            .parse()
            .ok()?;
        let mut max: Option<u64> = None;
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
            let max_start = i;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
            if i > max_start {
                max = Some(
                    std::str::from_utf8(&bytes[max_start..i]).ok()?.parse().ok()?,
                );
            }
        }
        if i < bytes.len() && bytes[i] == b'}' {
            i += 1;
            Some((min, max, i))
        } else {
            None
        }
    }

    /// The lazy marker: exactly one `?` after a quantifier.
    fn consume_lazy_marker(&mut self) {
        if self.peek() == Some(b'?') {
            self.pos += 1;
        }
    }

    /// atom: group / class / anchor / escape / literal.
    fn parse_atom(&mut self) -> Result<PatternNodeFlags, String> {
        let non_nullable = PatternNodeFlags {
            has_quantifier: false,
            has_alternation: false,
            nullable: false,
        };
        match self.peek() {
            None => Ok(EMPTY_FLAGS),
            Some(b'(') => {
                self.pos += 1;
                if self.peek() == Some(b'?') {
                    // Only the non-capturing `(?:` form; lookaround and every
                    // other `(?…` construct is rejected.
                    if self.source.get(self.pos + 1) == Some(&b':') {
                        self.pos += 2;
                    } else {
                        return Err(
                            "lookaround and other (?...) groups are not allowed".to_string()
                        );
                    }
                }
                let inner = self.parse_alternation(true)?;
                self.pos += 1; // consume ')'
                Ok(inner)
            }
            Some(b'[') => {
                self.parse_character_class()?;
                Ok(non_nullable)
            }
            Some(b'^') | Some(b'$') => {
                self.pos += 1;
                Ok(PatternNodeFlags {
                    has_quantifier: false,
                    has_alternation: false,
                    nullable: true,
                })
            }
            Some(b'\\') => {
                self.parse_escape()?;
                Ok(non_nullable)
            }
            Some(b'{') => {
                // A `{n,m}` text at term position is a quantifier without a
                // preceding atom — JS refuses to compile it, so is this
                // parser. Loose `{…}` forms are literals.
                if let Some((_, _, width)) = self.peek_repetition() {
                    let _ = width;
                    return Err("repetition without a preceding atom".to_string());
                }
                self.consume_loose_brace();
                Ok(non_nullable)
            }
            // Quantifier characters at atom position are "nothing to
            // repeat" — a SyntaxError in the JS engine (whose recompile
            // is what rejected `a**` / `a+*` in React); the `regex` crate
            // would accept them as nested repetition, so the walker
            // enforces the JS verdict itself.
            Some(b'*') | Some(b'+') | Some(b'?') => {
                Err("repetition without a preceding atom".to_string())
            }
            Some(_) => {
                self.pos += 1;
                Ok(non_nullable)
            }
        }
    }

    /// A `{` that is not a strict repetition: consume `{…}` when it closes
    /// loosely (`{,3}`, `{a,}`), otherwise the single `{` literal.
    fn consume_loose_brace(&mut self) {
        let bytes = &self.source[self.pos..];
        debug_assert_eq!(bytes.first(), Some(&b'{'));
        let mut i = 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
        if i < bytes.len() && bytes[i] == b',' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                i += 1;
            }
        }
        if i < bytes.len() && bytes[i] == b'}' {
            self.pos += i + 1;
        } else {
            self.pos += 1;
        }
    }

    /// character class: `[` … `]` with leading `]` literal, `^` negation,
    /// escapes and ranges.
    fn parse_character_class(&mut self) -> Result<(), String> {
        self.pos += 1; // consume '['
        if self.peek() == Some(b'^') {
            self.pos += 1;
        }
        // A ']' right after '[' (or '[^') is a literal, not the close.
        if self.peek() == Some(b']') {
            self.pos += 1;
        }
        loop {
            match self.peek() {
                None => return Err("unclosed character class".to_string()),
                Some(b']') => {
                    self.pos += 1;
                    return Ok(());
                }
                Some(b'\\') => self.parse_escape()?,
                Some(_) => self.pos += 1,
            }
        }
    }

    /// escape: whitelist only; backreferences and uncommon escapes reject.
    fn parse_escape(&mut self) -> Result<(), String> {
        self.pos += 1; // consume '\'
        let Some(esc) = self.peek() else {
            return Err("trailing backslash".to_string());
        };
        if esc.is_ascii_digit() {
            return Err("backreferences are not allowed".to_string());
        }
        if esc == b'k' {
            return Err("named backreferences are not allowed".to_string());
        }
        if esc == b'p' || esc == b'u' || esc == b'x' {
            return Err(format!("unsupported escape \"\\{}\"", esc as char));
        }
        if matches!(
            esc,
            b'd' | b'D' | b'w' | b'W' | b's' | b'S' | b'b' | b'B' | b't' | b'r' | b'f'
                | b'v' | b'0'
        ) {
            self.pos += 1;
            return Ok(());
        }
        // Other punctuation escapes (\. \\ \- \/) are literals; letter
        // escapes reject.
        if esc.is_ascii_alphabetic() {
            return Err(format!("unsupported escape \"\\{}\"", esc as char));
        }
        self.pos += 1;
        Ok(())
    }

    fn peek(&self) -> Option<u8> {
        self.source.get(self.pos).copied()
    }
}

/// Compile adapter: JS-vs-`regex`-crate spelling differences.
/// - `\0` (NUL) → `\x00` anywhere; `\b` inside a character class (JS
///   backspace) → `\x08`. Escape pairs are consumed together.
/// - A `{` that is NOT a strict `{n}`/`{n,}`/`{n,m}` repetition (the loose
///   `{,3}`/`{a,}` forms the atom parser consumes as literals) is escaped
///   to `\{`: the JS engine treats it as a literal brace, the `regex`
///   crate would error on it. Strict repetitions pass through unchanged
///   (the parser guarantees a preceding atom). Braces inside character
///   classes are always literal and pass through.
fn normalize_pattern_for_engine(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = String::with_capacity(source.len() + 8);
    let mut in_class = false;
    let mut class_just_opened = false;
    let mut i = 0;
    while i < bytes.len() {
        let byte = bytes[i];
        match byte {
            b'\\' if i + 1 < bytes.len() => {
                let esc = bytes[i + 1];
                if esc == b'0' {
                    out.push_str("\\x00");
                } else if in_class && esc == b'b' {
                    out.push_str("\\x08");
                } else {
                    out.push(byte as char);
                    out.push(esc as char);
                }
                i += 2;
                class_just_opened = false;
            }
            b'[' => {
                in_class = true;
                class_just_opened = true;
                out.push('[');
                i += 1;
            }
            b']' => {
                // A ']' that opens a class is a literal member.
                if class_just_opened {
                    class_just_opened = false;
                } else {
                    in_class = false;
                }
                out.push(']');
                i += 1;
            }
            b'{' if !in_class => {
                if is_strict_repetition(&bytes[i..]) {
                    // A real quantifier — copy the whole `{...}` group.
                    let mut end = i;
                    while end < bytes.len() && bytes[end] != b'}' {
                        end += 1;
                    }
                    let group = &source[i..=end.min(bytes.len() - 1)];
                    out.push_str(group);
                    i = end + 1;
                } else {
                    // Loose brace literal — escape it.
                    out.push_str("\\{");
                    i += 1;
                }
                class_just_opened = false;
            }
            _ => {
                // Non-ASCII bytes pass through as whole UTF-8 chars.
                let ch = source[i..].chars().next().expect("utf-8 boundary");
                out.push(ch);
                i += ch.len_utf8();
                class_just_opened = false;
            }
        }
    }
    out
}

/// `^\{\d+(,\d*)?\}` — the strict repetition shape (shared with the
/// parser's `peek_repetition`).
fn is_strict_repetition(bytes: &[u8]) -> bool {
    let mut i = 1;
    let digits_start = i;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digits_start {
        return false;
    }
    if i < bytes.len() && bytes[i] == b',' {
        i += 1;
        while i < bytes.len() && bytes[i].is_ascii_digit() {
            i += 1;
        }
    }
    i < bytes.len() && bytes[i] == b'}'
}

/// Validate that a regex source + flags belong to the restricted subset
/// (never panics). Returns Err(reason) on rejection.
pub(crate) fn validate_restricted_pattern(
    source: &str,
    flags: Option<&str>,
) -> Result<(), String> {
    match flags {
        None | Some("") | Some("i") => {}
        Some(_) => return Err("only the \"i\" flag is allowed".to_string()),
    }
    if source.is_empty() {
        return Err("pattern must not be empty".to_string());
    }
    if source.chars().count() > MAX_PATTERN_LENGTH {
        return Err(format!("pattern must be at most {MAX_PATTERN_LENGTH} characters"));
    }
    RestrictedPatternParser::new(source)
        .parse()
        .map_err(|reason| reason)?;
    // A parsed expression always compiles; the defensive recompile still
    // rejects anything the walker might have let through.
    if compile_pattern(source, flags).is_none() {
        return Err("pattern cannot be compiled".to_string());
    }
    Ok(())
}

/// Compile a (validated) pattern for matching. Unanchored substring
/// semantics like `RegExp.test`; `i` flag → case-insensitive.
fn compile_pattern(source: &str, flags: Option<&str>) -> Option<regex::Regex> {
    let normalized = normalize_pattern_for_engine(source);
    regex::RegexBuilder::new(&normalized)
        .case_insensitive(flags == Some("i"))
        .build()
        .ok()
}

// ---------------------------------------------------------------------------
// Name-pattern matching (plan-build time only, never per row)
// ---------------------------------------------------------------------------

/// Match a name pattern: exact values compare whole; regexes are substring
/// matches. An uncompilable pattern matches nothing (defensive — validated
/// patterns always compile).
pub(crate) fn match_name_pattern(pattern: &ValuePresentationNamePattern, value: &str) -> bool {
    match pattern {
        ValuePresentationNamePattern::Exact { value: exact } => exact == value,
        ValuePresentationNamePattern::Regex { source, flags } => {
            compile_pattern(source, flags.as_deref())
                .map(|regex| regex.is_match(value))
                .unwrap_or(false)
        }
    }
}

/// Human-readable pattern description (rule-list summaries): the exact
/// value verbatim, regexes as `/source/flags`.
pub(crate) fn describe_name_pattern(pattern: &ValuePresentationNamePattern) -> String {
    match pattern {
        ValuePresentationNamePattern::Exact { value } => value.clone(),
        ValuePresentationNamePattern::Regex { source, flags } => {
            format!("/{}/{}", source, flags.as_deref().unwrap_or(""))
        }
    }
}

// ---------------------------------------------------------------------------
// Rule validation
// ---------------------------------------------------------------------------

fn read_name_pattern(
    value: Option<&serde_json::Value>,
    path: &str,
    problems: &mut Vec<ValuePresentationRuleProblem>,
) -> Option<ValuePresentationNamePattern> {
    let object = match value {
        Some(serde_json::Value::Object(map)) => map,
        _ => {
            problems.push(ValuePresentationRuleProblem::new(path, "expected an object"));
            return None;
        }
    };
    let before = problems.len();
    let mut pattern: Option<ValuePresentationNamePattern> = None;
    match object.get("kind").and_then(serde_json::Value::as_str) {
        Some("exact") => {
            if let Some(exact) =
                read_identifier_value(object.get("value"), &format!("{path}.value"), problems)
            {
                pattern = Some(ValuePresentationNamePattern::Exact { value: exact });
            }
            for key in object.keys() {
                if key != "kind" && key != "value" {
                    problems.push(ValuePresentationRuleProblem::new(
                        &format!("{path}.{key}"),
                        "unknown field",
                    ));
                }
            }
        }
        Some("regex") => {
            match object.get("source") {
                Some(serde_json::Value::String(source)) => {
                    let source = source.trim();
                    // flags: absent → None; string → Some; anything else →
                    // a type problem (and no pattern).
                    let flags: Option<Option<String>> = match object.get("flags") {
                        None => Some(None),
                        Some(serde_json::Value::String(flags)) => Some(Some(flags.clone())),
                        Some(_) => {
                            problems.push(ValuePresentationRuleProblem::new(
                                &format!("{path}.flags"),
                                "expected a string",
                            ));
                            None
                        }
                    };
                    if let Some(flags) = flags {
                        match validate_restricted_pattern(source, flags.as_deref()) {
                            Err(reason) => problems.push(ValuePresentationRuleProblem::new(
                                &format!("{path}.source"),
                                &reason,
                            )),
                            Ok(()) => {
                                pattern = Some(match flags.as_deref() {
                                    None | Some("") => ValuePresentationNamePattern::Regex {
                                        source: source.to_string(),
                                        flags: None,
                                    },
                                    _ => ValuePresentationNamePattern::Regex {
                                        source: source.to_string(),
                                        flags,
                                    },
                                });
                            }
                        }
                    }
                }
                _ => problems.push(ValuePresentationRuleProblem::new(
                    &format!("{path}.source"),
                    "expected a string",
                )),
            }
            for key in object.keys() {
                if key != "kind" && key != "source" && key != "flags" {
                    problems.push(ValuePresentationRuleProblem::new(
                        &format!("{path}.{key}"),
                        "unknown field",
                    ));
                }
            }
        }
        _ => problems.push(ValuePresentationRuleProblem::new(
            &format!("{path}.kind"),
            "expected \"exact\" or \"regex\"",
        )),
    }
    if problems.len() > before {
        None
    } else {
        pattern
    }
}

fn read_rule_match(
    value: Option<&serde_json::Value>,
    problems: &mut Vec<ValuePresentationRuleProblem>,
) -> Option<ValuePresentationRuleMatch> {
    let object = match value {
        Some(serde_json::Value::Object(map)) => map,
        _ => {
            problems.push(ValuePresentationRuleProblem::new("match", "expected an object"));
            return None;
        }
    };
    let mut r#match = ValuePresentationRuleMatch::default();
    let mut condition_count = 0usize;
    if let Some(connection) = object.get("connectionId") {
        condition_count += 1;
        if let Some(v) =
            read_identifier_value(Some(connection), "match.connectionId", problems)
        {
            r#match.connection_id = Some(v);
        }
    }
    if let Some(database) = object.get("database") {
        condition_count += 1;
        if let Some(v) = read_identifier_value(Some(database), "match.database", problems) {
            r#match.database = Some(v);
        }
    }
    if let Some(namespace) = object.get("namespace") {
        condition_count += 1;
        if let Some(v) = read_identifier_value(Some(namespace), "match.namespace", problems) {
            r#match.namespace = Some(v);
        }
    }
    if let Some(table) = object.get("table") {
        condition_count += 1;
        if let Some(v) = read_name_pattern(Some(table), "match.table", problems) {
            r#match.table = Some(v);
        }
    }
    if let Some(column) = object.get("column") {
        condition_count += 1;
        if let Some(v) = read_name_pattern(Some(column), "match.column", problems) {
            r#match.column = Some(v);
        }
    }
    if let Some(type_name) = object.get("databaseTypeName") {
        condition_count += 1;
        if let Some(v) =
            read_name_pattern(Some(type_name), "match.databaseTypeName", problems)
        {
            r#match.database_type_name = Some(v);
        }
    }
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "connectionId" | "database" | "namespace" | "table" | "column" | "databaseTypeName"
        ) {
            problems.push(ValuePresentationRuleProblem::new(
                &format!("match.{key}"),
                "unknown field",
            ));
        }
    }
    if condition_count == 0 {
        problems.push(ValuePresentationRuleProblem::new(
            "match",
            "at least one match condition is required",
        ));
    }
    if !problems.is_empty() {
        return None;
    }
    Some(r#match)
}

/// Complete structural validation of unknown JSON input: a rebuilt
/// canonical rule on success, every pathed problem on failure. Illegal
/// rules must not save (PRD §3.1).
pub(crate) fn validate_value_presentation_rule(
    input: &serde_json::Value,
) -> ValuePresentationRuleValidation {
    let Some(object) = input.as_object() else {
        return ValuePresentationRuleValidation::Err(vec![
            ValuePresentationRuleProblem::new("", "expected an object"),
        ]);
    };
    let mut problems: Vec<ValuePresentationRuleProblem> = Vec::new();

    let mut name: Option<String> = None;
    match object.get("name") {
        Some(serde_json::Value::String(name_value)) => {
            let trimmed = name_value.trim();
            if trimmed.is_empty() {
                problems.push(ValuePresentationRuleProblem::new("name", "must not be empty"));
            } else if trimmed.chars().any(char::is_whitespace) {
                problems
                    .push(ValuePresentationRuleProblem::new("name", "must not contain whitespace"));
            } else if trimmed.chars().count() > MAX_RULE_NAME_LENGTH {
                problems.push(ValuePresentationRuleProblem::new(
                    "name",
                    &format!("must be at most {MAX_RULE_NAME_LENGTH} characters"),
                ));
            } else {
                name = Some(trimmed.to_string());
            }
        }
        _ => problems.push(ValuePresentationRuleProblem::new("name", "expected a string")),
    }

    let mut enabled: Option<bool> = None;
    match object.get("enabled") {
        Some(serde_json::Value::Bool(value)) => enabled = Some(*value),
        _ => problems.push(ValuePresentationRuleProblem::new("enabled", "expected a boolean")),
    }

    let r#match = read_rule_match(object.get("match"), &mut problems);

    let mut options: Option<TimestampOptions> = None;
    match validate_timestamp_options(object.get("options").unwrap_or(&serde_json::Value::Null)) {
        TimestampOptionsValidation::Ok(validated) => options = Some(validated),
        TimestampOptionsValidation::Err(option_problems) => {
            for TimestampOptionsProblem { path, message } in option_problems {
                problems.push(ValuePresentationRuleProblem::new(
                    &format!("options.{path}"),
                    &message,
                ));
            }
        }
    }

    for key in object.keys() {
        if !matches!(key.as_str(), "name" | "enabled" | "match" | "options") {
            problems.push(ValuePresentationRuleProblem::new(key, "unknown field"));
        }
    }

    if !problems.is_empty() || name.is_none() || enabled.is_none() || r#match.is_none() {
        return ValuePresentationRuleValidation::Err(problems);
    }

    ValuePresentationRuleValidation::Ok(Box::new(ValuePresentationRule {
        name: name.expect("checked above"),
        enabled: enabled.expect("checked above"),
        r#match: r#match.expect("checked above"),
        options: options.expect("checked above"),
    }))
}

/// Lenient rule-array parsing (storage boundary): non-arrays → empty;
/// invalid entries drop (the store keeps its original text); at most
/// MAX_VALUE_PRESENTATION_RULES valid entries survive.
pub(crate) fn validate_value_presentation_rules(
    input: &serde_json::Value,
) -> Vec<ValuePresentationRule> {
    let Some(entries) = input.as_array() else {
        return Vec::new();
    };
    let mut rules: Vec<ValuePresentationRule> = Vec::new();
    for entry in entries {
        if rules.len() >= MAX_VALUE_PRESENTATION_RULES {
            break;
        }
        if let ValuePresentationRuleValidation::Ok(rule) = validate_value_presentation_rule(entry)
        {
            rules.push(*rule);
        }
    }
    rules
}

/// Duplicate-name check for the editor layer (case-sensitive): returns the
/// first duplicated name, ignoring the row being edited.
pub(crate) fn find_duplicate_rule_name(
    rules: &[ValuePresentationRule],
    ignore_index: Option<usize>,
) -> Option<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    for (index, rule) in rules.iter().enumerate() {
        if Some(index) == ignore_index {
            continue;
        }
        if !seen.insert(rule.name.as_str()) {
            return Some(rule.name.clone());
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Matching: AND within a rule; table-level conditions need the source
// ---------------------------------------------------------------------------

/// Whether one enabled rule matches the column context (callers exclude
/// disabled rules first — see [`find_first_matching_rule`]).
pub(crate) fn match_value_presentation_rule(
    rule: &ValuePresentationRule,
    context: &ValuePresentationRuleContext<'_>,
) -> bool {
    let r#match = &rule.r#match;
    if let Some(connection) = &r#match.connection_id {
        if context.connection_id != Some(connection.as_str()) {
            return false;
        }
    }
    if let Some(database) = &r#match.database {
        if context.database != Some(database.as_str()) {
            return false;
        }
    }
    if let Some(namespace) = &r#match.namespace {
        if context.namespace != Some(namespace.as_str()) {
            return false;
        }
    }
    if let Some(table) = &r#match.table {
        match context.table {
            None => return false,
            Some(table_name) => {
                if !match_name_pattern(table, table_name) {
                    return false;
                }
            }
        }
    }
    if let Some(column) = &r#match.column {
        if !match_name_pattern(column, context.column) {
            return false;
        }
    }
    if let Some(type_name) = &r#match.database_type_name {
        match context.database_type_name {
            None => return false,
            Some(type_value) => {
                if !match_name_pattern(type_name, type_value) {
                    return false;
                }
            }
        }
    }
    true
}

/// The first matching rule in list order, skipping disabled rules; None
/// when nothing matches. The first match freezes the column's
/// interpretation — callers never fall through to lower-priority rules.
pub(crate) fn find_first_matching_rule<'a>(
    rules: &'a [ValuePresentationRule],
    context: &ValuePresentationRuleContext<'_>,
) -> Option<&'a ValuePresentationRule> {
    rules
        .iter()
        .filter(|rule| rule.enabled)
        .find(|rule| match_value_presentation_rule(rule, context))
}

/// Human-readable match summary for rule-list rows.
pub(crate) fn describe_rule_match(r#match: &ValuePresentationRuleMatch) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(connection) = &r#match.connection_id {
        parts.push(format!("connection={connection}"));
    }
    if let Some(database) = &r#match.database {
        parts.push(format!("database={database}"));
    }
    if let Some(namespace) = &r#match.namespace {
        parts.push(format!("schema={namespace}"));
    }
    if let Some(table) = &r#match.table {
        parts.push(format!("table={}", describe_name_pattern(table)));
    }
    if let Some(column) = &r#match.column {
        parts.push(format!("column={}", describe_name_pattern(column)));
    }
    if let Some(type_name) = &r#match.database_type_name {
        parts.push(format!("type={}", describe_name_pattern(type_name)));
    }
    parts.join(", ")
}
