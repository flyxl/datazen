//! Port of `packages/frontend/src/windows/workflow/yamlMode.ts` (T265) — the
//! lightweight YAML syntax highlighting for the workflow YAML editor.
//!
//! React builds the highlighting as a CodeMirror 6 `StreamLanguage` whose
//! token names map straight to `@lezer/highlight` tags, then colors the
//! tags through a `HighlightStyle` over the app's `--cm-*` tokens — so the
//! coloring follows the active theme with no JS reconfigure. The port
//! folds both halves into one scan: [`highlight_yaml`] walks the document
//! with the stream parser's exact token boundaries and emits the Rune
//! `Span`s directly, taking the per-theme colors from
//! [`crate::ui::palette::yaml_palette`] (the `--cm-*` twin).
//!
//! Scope is the React file's own pragmatic cut, kept verbatim: comments,
//! mapping keys, quoted/plain scalars, numbers, booleans/null, flow/list
//! punctuation, anchors, and `${param}` interpolation.
//!
//! The stream state is one flag — "a mapping `key:` just ended on this
//! line → trailing text is a value" — reset at each line start (`sol()`);
//! the port restarts the flag per line the same way.

use rune::{Color, Span};

use crate::ui::palette::yaml_palette;

/// The React atom-literal set: `true|false|null|yes|no|on|off|~`, matched
/// case-insensitively (`/i` in the React regex).
fn is_atom_literal(word: &str) -> bool {
    matches!(
        word.to_ascii_lowercase().as_str(),
        "true" | "false" | "null" | "yes" | "no" | "on" | "off" | "~"
    )
}

/// The React number shape: `/^[+-]?\d+(\.\d+)?([eE][+-]?\d+)?$/` — the full
/// scalar text (sign included) must match, not just a prefix.
fn is_number_literal(word: &str) -> bool {
    let rest = word.strip_prefix(['+', '-']).unwrap_or(word);
    let mut mantissa_end = 0usize;
    let bytes = rest.as_bytes();
    while mantissa_end < bytes.len() && bytes[mantissa_end].is_ascii_digit() {
        mantissa_end += 1;
    }
    if mantissa_end == 0 {
        return false;
    }
    let mut at = mantissa_end;
    if at < bytes.len() && bytes[at] == b'.' {
        at += 1;
        let mut frac = 0usize;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
            frac += 1;
        }
        if frac == 0 {
            return false;
        }
    }
    if at < bytes.len() && (bytes[at] == b'e' || bytes[at] == b'E') {
        at += 1;
        if at < bytes.len() && (bytes[at] == b'+' || bytes[at] == b'-') {
            at += 1;
        }
        let mut exponent = 0usize;
        while at < bytes.len() && bytes[at].is_ascii_digit() {
            at += 1;
            exponent += 1;
        }
        if exponent == 0 {
            return false;
        }
    }
    at == bytes.len()
}

/// The plain-scalar character class the React stream eats after a key or a
/// scalar start: `[A-Za-z0-9_./-]`.
#[inline]
fn is_plain_word(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'/' | b'-')
}

/// Highlight a workflow YAML document into `Span`s — the port of the React
/// `StreamLanguage` walk plus its `HighlightStyle` coloring, folded into
/// one pass. `foreground` is the text ink the caller's theme already holds
/// (React's `--cm-foreground`, the `variableName` tag's color source).
pub(crate) fn highlight_yaml(src: &str, foreground: Color, dark: bool) -> Vec<Span> {
    let palette = yaml_palette(dark, foreground);
    let mut spans = Vec::new();
    let mut line_start = 0usize;
    for line in src.split('\n') {
        // sol(): values are per-line in simple mappings — the in-value
        // state a `key:` set on this line dies with the line.
        let mut in_value = false;
        let bytes = line.as_bytes();
        let mut i = 0usize;
        while i < bytes.len() {
            // eatWhile(/[ \t]/) — the leading whitespace the React stream
            // consumes without a token.
            if bytes[i] == b' ' || bytes[i] == b'\t' {
                i += 1;
                continue;
            }
            let ch = bytes[i];
            // comment: skipToEnd
            if ch == b'#' {
                spans.push(Span::new(line_start + i, line_start + line.len())
                    .color(palette.comment)
                    .italic());
                i = line.len();
                continue;
            }
            // block scalar introducer (| or >): one operator byte, then the
            // rest of the line belongs to a literal/folded scalar's value.
            if ch == b'|' || ch == b'>' {
                spans.push(Span::new(line_start + i, line_start + i + 1).color(palette.punctuation));
                in_value = true;
                i += 1;
                continue;
            }
            // flow structure / list markers: one punctuation byte.
            if matches!(ch, b'[' | b']' | b'{' | b'}' | b',' | b'-') {
                spans.push(Span::new(line_start + i, line_start + i + 1).color(palette.punctuation));
                in_value = false;
                i += 1;
                continue;
            }
            // anchor / alias: the marker plus its name, keyword-tagged.
            if ch == b'&' || ch == b'*' {
                let start = i;
                i += 1;
                while i < bytes.len()
                    && (bytes[i].is_ascii_alphanumeric() || matches!(bytes[i], b'_' | b'-'))
                {
                    i += 1;
                }
                spans.push(Span::new(line_start + start, line_start + i).color(palette.punctuation));
                continue;
            }
            // quoted string (with the possible quoted key after it).
            if ch == b'"' || ch == b'\'' {
                let quote = ch;
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    i += 1;
                    if c == quote {
                        break;
                    }
                    // "..." keeps its backslash escapes; '...' is literal.
                    if quote == b'"' && c == b'\\' && i < bytes.len() {
                        i += 1;
                    }
                }
                if i < bytes.len() && bytes[i] == b':' {
                    i += 1;
                    in_value = true;
                    spans.push(
                        Span::new(line_start + start, line_start + i).color(palette.keyword),
                    );
                } else {
                    spans.push(
                        Span::new(line_start + start, line_start + i).color(palette.string),
                    );
                }
                continue;
            }
            // ${param} interpolation: '$' eats through the closing '}' (or
            // the line's end) — keyword-tagged, punctuation-colored.
            if ch == b'$' {
                let start = i;
                i += 1;
                while i < bytes.len() {
                    let c = bytes[i];
                    i += 1;
                    if c == b'}' {
                        break;
                    }
                }
                spans.push(Span::new(line_start + start, line_start + i).color(palette.punctuation));
                continue;
            }
            // plain identifier: key or scalar.
            if is_plain_word(ch) {
                let start = i;
                while i < bytes.len() && is_plain_word(bytes[i]) {
                    i += 1;
                }
                if i < bytes.len() && bytes[i] == b':' {
                    i += 1;
                    in_value = true;
                    spans.push(
                        Span::new(line_start + start, line_start + i).color(palette.keyword),
                    );
                    continue;
                }
                // trailing plain scalar on this line (stop at inline
                // comment).
                while i < bytes.len() && bytes[i] != b'#' {
                    i += 1;
                }
                let text = line[start..i].trim();
                if text.is_empty() {
                    continue;
                }
                let tag_color = if is_atom_literal(text) {
                    palette.atom
                } else if is_number_literal(text) {
                    palette.number
                } else if in_value {
                    palette.string
                } else {
                    palette.foreground
                };
                spans.push(Span::new(line_start + start, line_start + i).color(tag_color));
                continue;
            }
            // anything else: one byte, no token, state reset.
            i += 1;
            in_value = false;
        }
        line_start += line.len() + 1;
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The React file has no test suite of its own (its behaviour shows
    /// through the editor); these pins hold the token walk the port
    /// promised: the exact tag→color routing the React HighlightStyle
    /// declares, driven by a document that exercises every branch.
    fn colors(src: &str) -> Vec<(usize, usize, rune::Color)> {
        let dark = true;
        let foreground = rune::Color::WHITE;
        highlight_yaml(src, foreground, dark)
            .into_iter()
            .map(|s| (s.start as usize, s.end as usize, s.color.unwrap_or(foreground)))
            .collect()
    }

    #[test]
    fn comments_keys_values_and_flow_punctuation_route_to_their_tags() {
        let src = "name: audit\nsteps:\n  - compare structures\n";
        let palette = yaml_palette(true, rune::Color::WHITE);
        let spans = colors(src);
        // `name:` (with its colon) is the propertyName tag → the keyword hue.
        assert!(
            spans.iter().any(|(from, to, c)| *from == 0 && *to == 5 && *c == palette.keyword),
            "the mapping key plus its colon is keyword-colored: {spans:?}"
        );
        // `audit` is a value on that line → the string hue.
        assert!(
            spans.iter().any(|(from, to, c)| *from == 6 && *to == 11 && *c == palette.string),
            "the trailing scalar after a key is string-colored: {spans:?}"
        );
        // `steps:` is a key again on its own line.
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 12 && *to == 18 && *c == palette.keyword),
            "the second key is keyword-colored: {spans:?}"
        );
        // the list marker `-` is one punctuation byte.
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 21 && *to == 22 && *c == palette.punctuation),
            "the list marker is punctuation-colored: {spans:?}"
        );
        // `compare structures` is not a value (the marker reset the state)
        // → the foreground ink, and the whole two-word scalar is one span.
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 23 && *to == 41 && *c == palette.foreground),
            "a bare scalar after a list marker is foreground-colored: {spans:?}"
        );
    }

    #[test]
    fn comment_lines_atom_and_number_scalars_route_to_their_tags() {
        let src = "# hint\nenabled: true\ncount: 42\nratio: +3.5e-1\n";
        let palette = yaml_palette(true, rune::Color::WHITE);
        let spans = colors(src);
        assert!(
            spans.iter().any(|(from, to, c)| *from == 0 && *to == 6 && *c == palette.comment),
            "the comment is comment-colored: {spans:?}"
        );
        assert!(
            spans.iter().any(|(from, to, c)| *from == 16 && *to == 20 && *c == palette.atom),
            "the true atom literal is atom-colored: {spans:?}"
        );
        assert!(
            spans.iter().any(|(from, to, c)| *from == 28 && *to == 30 && *c == palette.number),
            "the number scalar is number-colored: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 39 && *to == 45 && *c == palette.number),
            "the signed exponent literal is number-colored (the sign byte rides no span — React's null token): {spans:?}"
        );
    }

    #[test]
    fn param_interpolation_and_anchors_route_to_the_punctuation_tag() {
        // The scalar branch is greedy (React's `eatWhile(/[^#\n]/)`), so the
        // punctuation tag shows only when the '$' starts the token — the
        // value-position reference below; the mid-scalar case has its own
        // pin further down.
        let src = "sql: ${id}\nanchor: &base\nto: *base\n";
        let palette = yaml_palette(true, rune::Color::WHITE);
        let spans = colors(src);
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 5 && *to == 10 && *c == palette.punctuation),
            "the ${{id}} interpolation (closer included) is punctuation-colored: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 19 && *to == 24 && *c == palette.punctuation),
            "the &base anchor name rides with its marker: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 29 && *to == 34 && *c == palette.punctuation),
            "the *base alias rides with its marker: {spans:?}"
        );
    }

    #[test]
    fn a_mid_scalar_param_ref_is_swallowed_by_the_greedy_scalar() {
        // React's plain-scalar branch eats through the line, so the SQL
        // value's embedded ${id} rides inside the scalar's span (a plain
        // stream lexer's known cut, kept verbatim — the pair check in the
        // lint pass is the real guard for those).
        let src = "sql: SELECT ${id} FROM t\n";
        let palette = yaml_palette(true, rune::Color::WHITE);
        let spans = colors(src);
        assert!(
            spans.iter().any(|(from, to, c)| *from == 5 && *to == 24 && *c == palette.string),
            "the whole trailing SQL scalar (the ${{id}} included) is one string-colored span: {spans:?}"
        );
        assert!(
            !spans.iter().any(|(_, _, c)| *c == palette.punctuation),
            "no punctuation-tagged span survives mid-scalar: {spans:?}"
        );
    }

    #[test]
    fn quoted_strings_keys_and_block_scalars_route_to_their_tags() {
        let src = "label: \"plain quoted\"\n'quoted key': v\nlong: |\n  literal block\n";
        let palette = yaml_palette(true, rune::Color::WHITE);
        let spans = colors(src);
        assert!(
            spans.iter().any(|(from, to, c)| *from == 7 && *to == 21 && *c == palette.string),
            "the double-quoted string is string-colored: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 22 && *to == 35 && *c == palette.keyword),
            "the quoted key plus its colon is keyword-colored: {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 44 && *to == 45 && *c == palette.punctuation),
            "the block-scalar introducer is punctuation-colored: {spans:?}"
        );
        // The block body is a BARE scalar: React's `sol()` resets inValue at
        // every line start, so the introducer's in-value state dies with its
        // own line and the body's text is the foreground ink — the stream
        // parser's own per-line law, kept verbatim (the "rest belongs to a
        // scalar" note is the introducer line's semantics only).
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 48 && *to == 61 && *c == palette.foreground),
            "the literal block's body is foreground-colored (the per-line reset): {spans:?}"
        );
    }

    #[test]
    fn inline_comment_truncates_a_trailing_scalar() {
        // The plain-scalar run eats to the inline comment, and the comment
        // itself is comment-colored.
        let src = "mode: draft # open note\n";
        let palette = yaml_palette(true, rune::Color::WHITE);
        let spans = colors(src);
        assert!(
            spans.iter().any(|(from, to, c)| *from == 6 && *to == 12 && *c == palette.string),
            "the value scalar stops before the '#' (its trailing space rides the span; trim only classifies): {spans:?}"
        );
        assert!(
            spans
                .iter()
                .any(|(from, to, c)| *from == 12 && *to == 23 && *c == palette.comment),
            "the inline comment is comment-colored: {spans:?}"
        );
    }
}
