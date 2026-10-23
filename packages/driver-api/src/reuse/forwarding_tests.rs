//! Machine enforcement of "a reuse wrapper forwards every driver rule".
//!
//! [`ReuseDriver`] presents an existing driver under a different id
//! (`questdb`, `cloudberry`, `doris`, `starrocks`, `manticore`, `ob_oracle`).
//! Its whole job is identity preservation: any method it does not forward
//! silently falls back to the [`DatabaseDriver`] default, which is written for
//! a *generic* driver, not for this wire family.
//!
//! That failure mode is not hypothetical. Sixteen methods were missing from the
//! wrapper: `has_multi_database`, `qualify_sql_target`, `get_schemas`,
//! `default_host`, `default_port`, `migration_renderer`,
//! `migration_capabilities`, `dialect_notes`, `query_stream_with_params`,
//! `query_stream_with_execution_at`, `render_transfer_identity_sequence_sync_sql`,
//! `validate_schema_migration`, `validate_schema_migration_plan`,
//! `build_update_statement`, `build_delete_statement` and `quote_ident` — while
//! the sibling methods next to them *were* forwarded. The wrapper forwarded the
//! per-database pool operations (`open_databases`, `close_database`) but not
//! `has_multi_database`, so a PostgreSQL alias told the host it had one database
//! while holding pools for several.
//!
//! # What is enforced
//!
//! Every method declared in `trait DatabaseDriver` must appear in
//! `impl DatabaseDriver for ReuseDriver`. Forwarded or deliberately overridden
//! both count — the rule is "the wrapper made a decision", never "the wrapper
//! inherited a default for a driver-specific rule".
//!
//! # Why source text instead of a macro
//!
//! Same reasoning as `resource_no_default_bodies_tests.rs`: no `syn`/`trybuild`
//! dependency for a one-trait invariant, and `include_str!` keeps the check in
//! the ordinary `cargo test` gate. The extractor has positive and negative
//! controls below, so the guard cannot pass by finding nothing.

/// The canonical contract, read as text.
const TRAITS_RS: &str = include_str!("../traits.rs");

/// The wrapper whose forwarding table is under guard. The table is one file —
/// one impl block — and it is read as a whole.
const REUSE_DRIVER_IMPL_RS: &str = include_str!("driver_impl.rs");

const TRAIT_ANCHOR: &str = "pub trait DatabaseDriver";
const IMPL_ANCHOR: &str = "impl DatabaseDriver for ReuseDriver";

/// The gate: no trait method may be left to the trait default.
#[test]
fn reuse_driver_decides_on_every_database_driver_method() {
    let traits_stripped = strip_comments_and_strings(TRAITS_RS);
    let trait_body = block_after(&traits_stripped, TRAIT_ANCHOR)
        .expect("guard ran vacuously: `pub trait DatabaseDriver` block not found");

    let declared = method_names(trait_body);

    // The forwarding table is one impl block in one file, so it is read as
    // one. A method that is forwarded nowhere shows up as undecided.
    let reuse_stripped = strip_comments_and_strings(REUSE_DRIVER_IMPL_RS);
    let impl_body = block_after(&reuse_stripped, IMPL_ANCHOR)
        .expect("guard ran vacuously: `impl DatabaseDriver for ReuseDriver` block not found");
    let decided = method_names(impl_body);

    assert!(
        declared.len() > 50,
        "guard ran vacuously: only {} trait methods found — the scanner is broken",
        declared.len()
    );

    let missing: Vec<&str> = declared
        .iter()
        .copied()
        .filter(|name| !decided.iter().any(|d| d == name))
        .collect();

    assert!(
        missing.is_empty(),
        "ReuseDriver inherits the trait default for {} method(s) instead of forwarding or \
         overriding them: {missing:?}.\n\
         Every method of `DatabaseDriver` must be decided by the wrapper, because a default body \
         is written for a generic driver and silently drops the wrapped driver's rule \
         (e.g. `has_multi_database` returning false while `open_databases` is forwarded).",
        missing.len()
    );
}

/// Positive control: a method present on the trait but absent from the impl is
/// reported. Proves the gate is not passing because both lists are empty.
#[test]
fn the_guard_reports_a_missing_forward() {
    let trait_src = "pub trait DatabaseDriver: Send + Sync {\n    fn driver_type(&self) -> u8;\n    fn quote_ident(&self, name: &str) -> String;\n}\n";
    let impl_src =
        "impl DatabaseDriver for ReuseDriver {\n    fn driver_type(&self) -> u8 { 1 }\n}\n";
    let declared = method_names(block_after(trait_src, TRAIT_ANCHOR).unwrap());
    let decided = method_names(block_after(impl_src, IMPL_ANCHOR).unwrap());
    assert_eq!(declared, vec!["driver_type", "quote_ident"]);
    assert_eq!(decided, vec!["driver_type"]);
    assert_eq!(declared.iter().filter(|n| !decided.contains(n)).count(), 1);
}

/// Negative control: comments, doc comments and string literals cannot inject
/// fake `fn` tokens or desynchronise the brace scanner.
#[test]
fn comments_and_strings_do_not_feed_the_scanner() {
    let src = "pub trait DatabaseDriver: Send + Sync {\n    \
               /// Use `fn inner_rule()` in your impl.\n    \
               // fn commented_out(&self);\n    \
               fn real(&self, s: &str) -> String; // trailing { } text\n}\n";
    let stripped = strip_comments_and_strings(src);
    let body = block_after(&stripped, TRAIT_ANCHOR).unwrap();
    assert_eq!(method_names(body), vec!["real"]);
}

/// Returns the text between the first `{` after `anchor` and its matching `}`.
fn block_after<'a>(text: &'a str, anchor: &str) -> Option<&'a str> {
    let from = text.find(anchor)?;
    let open = from + text[from..].find('{')?;
    let mut depth = 0usize;
    for (offset, ch) in text[open..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[open + 1..open + offset]);
                }
            }
            _ => {}
        }
    }
    None
}

/// `fn`-prefixed identifiers in a stripped block, in declaration order.
fn method_names(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let mut names = Vec::new();
    let mut i = 0;
    while i + 1 < bytes.len() {
        if &bytes[i..i + 2] == b"fn" {
            let before_ok = i == 0 || !is_ident_byte(bytes[i - 1]);
            let mut j = i + 2;
            let after_ok = j < bytes.len() && !is_ident_byte(bytes[j]);
            if before_ok && after_ok {
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                let start = j;
                while j < bytes.len() && is_ident_byte(bytes[j]) {
                    j += 1;
                }
                if start < j {
                    names.push(&body[start..j]);
                }
                i = j;
                continue;
            }
        }
        i += 1;
    }
    names
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Replace comments and string/char literal contents with spaces, keeping line
/// structure and braces outside literals intact.
fn strip_comments_and_strings(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let bytes = src.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    out.push(' ');
                    i += 1;
                }
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                let mut depth = 1;
                out.push_str("  ");
                i += 2;
                while i < bytes.len() && depth > 0 {
                    if i + 1 < bytes.len() && bytes[i] == b'/' && bytes[i + 1] == b'*' {
                        depth += 1;
                        out.push_str("  ");
                        i += 2;
                    } else if i + 1 < bytes.len() && bytes[i] == b'*' && bytes[i + 1] == b'/' {
                        depth -= 1;
                        out.push_str("  ");
                        i += 2;
                    } else {
                        out.push(if bytes[i] == b'\n' { '\n' } else { ' ' });
                        i += 1;
                    }
                }
            }
            b'"' => {
                out.push('"');
                i += 1;
                while i < bytes.len() {
                    let ch = bytes[i];
                    if ch == b'\\' && i + 1 < bytes.len() {
                        out.push_str("  ");
                        i += 2;
                        continue;
                    }
                    i += 1;
                    if ch == b'"' {
                        break;
                    }
                    out.push(if ch == b'\n' { '\n' } else { ' ' });
                }
                out.push('"');
            }
            b'\'' => {
                // A lifetime (`'a`) must survive; only char literals are blanked.
                let is_lifetime = i + 1 < bytes.len()
                    && (bytes[i + 1].is_ascii_alphabetic() || bytes[i + 1] == b'_')
                    && (i + 2 >= bytes.len() || bytes[i + 2] != b'\'');
                if is_lifetime {
                    out.push('\'');
                    i += 1;
                    continue;
                }
                out.push('\'');
                i += 1;
                while i < bytes.len() {
                    let ch = bytes[i];
                    if ch == b'\\' && i + 1 < bytes.len() {
                        out.push_str("  ");
                        i += 2;
                        continue;
                    }
                    i += 1;
                    if ch == b'\'' {
                        break;
                    }
                    out.push(' ');
                }
                out.push('\'');
            }
            _ => {
                let ch = bytes[i] as char;
                out.push(ch);
                i += 1;
            }
        }
    }
    out
}
