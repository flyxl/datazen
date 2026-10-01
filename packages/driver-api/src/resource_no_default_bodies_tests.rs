//! Machine enforcement of the "no default bodies on `ResourceProvider`" rule.
//!
//! The rule is documented in `resource.rs`, but documentation is not a
//! constraint: before this module existed, `impl ResourceProvider for X {}`
//! compiled and handed a driver a full set of no-op operations, which is
//! exactly the "a missing new capability succeeds as a silent no-op" outcome
//! that `platform-development-plan.md` §6 P2 forbids at the exit gate.
//!
//! # Scope, stated precisely
//!
//! * **Enforced here:** every method in `trait ResourceProvider` is a
//!   declaration (`;`), never a definition (`{ ... }`). That single property is
//!   what makes an empty `impl` block a compile error, which is the whole
//!   point of the rule.
//! * **Not enforced here:** the contents of a provider's *own* impl. A driver
//!   that hand-writes its own no-op body is a review question, and is caught
//!   at runtime by the fail-closed accessor in `factory.rs`.
//!
//! # Why this reads the source instead of using a macro
//!
//! `trybuild` / `syn` are not in this crate's dependency tree and adding them
//! for a one-trait invariant is not worth the build cost. `include_str!` plus
//! the small extractor below runs inside the ordinary `cargo test` gate with
//! no new dependency. The extractor is itself covered by the last three tests,
//! so the guard cannot pass by silently failing to find the trait.
//!
//! # No test fixture is exempt
//!
//! Both in-crate `ResourceProvider` implementations —
//! `resource_adapter.rs::LegacyResourceAdapter` and the integration fixture
//! `tests/support/mod.rs::FakeProvider` — already spell out all 14 methods, so
//! none of them leans on a default body and none needed changing. That is
//! re-checked whenever the method-count test below runs: if a default body ever
//! appears, the fixtures break at compile time *and* this gate goes red.

/// The file under guard, as source text.
const RESOURCE_RS: &str = include_str!("resource.rs");

/// Trait whose methods must all be declarations.
const TRAIT_NAME: &str = "ResourceProvider";

/// Method counts asserted against the trait doc comment in `resource.rs`.
const EXPECTED_METHODS: usize = 14;
const EXPECTED_ASYNC_METHODS: usize = 11;

/// The core gate: not one `{` may survive inside the trait block.
#[test]
fn every_resource_provider_method_is_a_declaration() {
    let stripped = strip_comments(RESOURCE_RS);
    let (body_start, body) = trait_block(&stripped, TRAIT_NAME);

    assert!(
        !body.is_empty(),
        "guard ran vacuously: could not locate `trait {TRAIT_NAME} {{ ... }}` in resource.rs"
    );

    if let Some(offset) = body.find(|c| c == '{' || c == '}') {
        let at = body_start + offset;
        panic!(
            "ResourceProvider declares a method body (default implementation) at resource.rs:{} — \
             every method must be a declaration ending in `;`, otherwise \
             `impl ResourceProvider for X {{}}` compiles and a driver inherits no-ops.\n  {}",
            line_of(RESOURCE_RS, at),
            line_text(RESOURCE_RS, at).trim()
        );
    }
}

/// Pins the numbers the `resource.rs` doc comment quotes, so the comment and
/// the trait cannot drift apart without turning CI red.
#[test]
fn documented_method_counts_match_the_trait() {
    let stripped = strip_comments(RESOURCE_RS);
    let (_, body) = trait_block(&stripped, TRAIT_NAME);
    assert!(!body.is_empty(), "trait block not found");

    let methods = method_signatures(body);
    assert_eq!(
        methods.len(),
        EXPECTED_METHODS,
        "resource.rs doc comment and this guard disagree on the total method count: {:?}",
        names_of(&methods)
    );
    assert_eq!(
        methods.iter().filter(|m| m.is_async).count(),
        EXPECTED_ASYNC_METHODS,
        "resource.rs doc comment and this guard disagree on the async method count: {:?}",
        names_of(&methods)
    );

    let mut seen: Vec<&str> = Vec::new();
    for method in &methods {
        assert!(
            !method.name.is_empty(),
            "found a `fn` keyword with no name in the trait block"
        );
        assert!(
            !seen.contains(&method.name.as_str()),
            "`{}` is declared twice on the trait",
            method.name
        );
        seen.push(&method.name);
    }
}

/// Positive control: a default body really is reported, so the gate above is
/// not passing because the extractor sees nothing.
#[test]
fn the_guard_flags_a_default_body() {
    let snippet = "pub trait ResourceProvider: Send + Sync {\n    \
                   async fn close_resource(&self) -> Result<(), ()> { Ok(()) }\n}\n";
    let stripped = strip_comments(snippet);
    let (_, body) = trait_block(&stripped, TRAIT_NAME);
    assert!(
        body.find(|c| c == '{' || c == '}').is_some(),
        "a default body must be reported"
    );
}

/// Negative control: braces inside doc comments are not a method body. Without
/// this, adding a `{}` example to any doc comment would fail the build.
#[test]
fn braces_inside_doc_comments_are_not_mistaken_for_bodies() {
    let snippet = "pub trait ResourceProvider: Send + Sync {\n    \
                   /// Returns `{}` once the resource is idle.\n    \
                   async fn close_resource(&self) -> Result<(), ()>;\n}\n";
    let stripped = strip_comments(snippet);
    let (_, body) = trait_block(&stripped, TRAIT_NAME);
    assert_eq!(body.find(|c| c == '{' || c == '}'), None);
}

/// Negative control: comments, strings and lifetimes do not desynchronise the
/// scanner, so the gate cannot be silenced by editing prose.
#[test]
fn comments_strings_and_lifetimes_do_not_desynchronise_the_scanner() {
    let snippet = "pub trait ResourceProvider: Send + Sync {\n    \
                   // a line comment with } and { and a /* start\n    \
                   /// doc: the provider's own \"a // b\" text and a /* nested start\n    \
                   fn provider_id<'a>(&'a self) -> &'a str;\n}\n";
    let stripped = strip_comments(snippet);
    let (_, body) = trait_block(&stripped, TRAIT_NAME);
    assert_eq!(body.find(|c| c == '{' || c == '}'), None);
    assert_eq!(method_signatures(body).len(), 1);
    assert_eq!(method_signatures(body)[0].name, "provider_id");
}

// ---------------------------------------------------------------------------
// Extractor
// ---------------------------------------------------------------------------

struct MethodSignature {
    name: String,
    is_async: bool,
}

fn names_of(methods: &[MethodSignature]) -> Vec<&str> {
    methods.iter().map(|m| m.name.as_str()).collect()
}

fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Blank out comments while preserving every byte offset and every newline, so
/// diagnostics can still point at the real line in the real file.
///
/// Only `//` and `/* */` matter here, and neither can appear inside a char
/// literal or a lifetime, so char/lifetime tracking is deliberately omitted.
/// Double-quoted strings are tracked because `"http://..."` legitimately
/// contains a comment opener.
fn strip_comments(source: &str) -> String {
    let bytes = source.as_bytes();
    let mut out = vec![b' '; bytes.len()];
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' {
            out[index] = b'\n';
        }
    }

    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'/' if bytes.get(i + 1) == Some(&b'/') => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'/' if bytes.get(i + 1) == Some(&b'*') => {
                let mut depth = 0usize;
                while i < bytes.len() {
                    match (bytes[i], bytes.get(i + 1).copied()) {
                        (b'/', Some(b'*')) => {
                            depth += 1;
                            i += 2;
                        }
                        (b'*', Some(b'/')) => {
                            depth -= 1;
                            i += 2;
                            if depth == 0 {
                                break;
                            }
                        }
                        _ => i += 1,
                    }
                }
            }
            b'"' => i = copy_string(bytes, i, &mut out),
            _ => {
                if let Some(end) = raw_string_end(bytes, i) {
                    for slot in out.iter_mut().take(end).skip(i) {
                        *slot = b' ';
                    }
                    i = end;
                } else {
                    out[i] = bytes[i];
                    i += 1;
                }
            }
        }
    }

    String::from_utf8(out).expect("blanking comments preserves UTF-8")
}

/// Copy a `"…"` literal through its closing quote; return the next index.
fn copy_string(bytes: &[u8], open: usize, out: &mut [u8]) -> usize {
    out[open] = b'"';
    let mut i = open + 1;
    while i < bytes.len() {
        out[i] = bytes[i];
        if bytes[i] == b'\\' {
            if let Some(next) = bytes.get(i + 1) {
                out[i + 1] = *next;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }
        if bytes[i] == b'"' {
            return i + 1;
        }
        if bytes[i] == b'\n' {
            return i;
        }
        i += 1;
    }
    i
}

/// Index just past a raw string starting at `start`, or `None`.
///
/// Recognises `r"…"`, `br"…"` and their `#`-delimited forms.
fn raw_string_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut cursor = start;
    if bytes.get(cursor) == Some(&b'b') {
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'r') {
        return None;
    }
    if start > 0 && is_ident_byte(bytes[start - 1]) {
        return None;
    }
    cursor += 1;
    let mut hashes = 0usize;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }
    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }
    cursor += 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes[cursor + 1..cursor + 1 + hashes]
                .iter()
                .all(|b| *b == b'#')
        {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }
    Some(bytes.len())
}

/// `(body_start_offset, text_between_the_trait_braces)`.
///
/// Returns an empty body when the trait cannot be located, which every caller
/// treats as a failure rather than as "nothing found, nothing to check".
fn trait_block<'a>(stripped: &'a str, name: &str) -> (usize, &'a str) {
    let needle = format!("trait {name}");
    let mut search_from = 0usize;
    while let Some(found) = stripped[search_from..].find(&needle) {
        let header = search_from + found;
        let after_name = header + needle.len();
        // Reject `trait ResourceProviderExtra`.
        if bytes_get(stripped, after_name).is_some_and(is_ident_byte) {
            search_from = after_name;
            continue;
        }
        let Some(open) = stripped[header..].find('{').map(|o| header + o) else {
            return (0, "");
        };
        let mut depth = 0usize;
        for (offset, byte) in stripped.as_bytes()[open..].iter().enumerate() {
            match byte {
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return (open + 1, &stripped[open + 1..open + offset]);
                    }
                }
                _ => {}
            }
        }
        return (0, "");
    }
    (0, "")
}

fn bytes_get(source: &str, index: usize) -> Option<u8> {
    source.as_bytes().get(index).copied()
}

/// Every `fn` declaration in the block, in source order.
fn method_signatures(body: &str) -> Vec<MethodSignature> {
    let bytes = body.as_bytes();
    let mut found = Vec::new();
    let mut i = 0usize;
    while i + 1 < bytes.len() {
        let is_fn_keyword = bytes[i] == b'f'
            && bytes[i + 1] == b'n'
            && (i == 0 || !is_ident_byte(bytes[i - 1]))
            && !bytes.get(i + 2).is_some_and(|b| is_ident_byte(*b));
        if !is_fn_keyword {
            i += 1;
            continue;
        }
        let is_async = preceding_word(body, i).as_deref() == Some("async");
        let mut cursor = i + 2;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let name_start = cursor;
        while cursor < bytes.len() && is_ident_byte(bytes[cursor]) {
            cursor += 1;
        }
        found.push(MethodSignature {
            name: body[name_start..cursor].to_string(),
            is_async,
        });
        i = cursor;
    }
    found
}

fn preceding_word(source: &str, before: usize) -> Option<String> {
    let bytes = source.as_bytes();
    let mut i = before;
    while i > 0 && bytes[i - 1].is_ascii_whitespace() {
        i -= 1;
    }
    let start = i;
    while i > 0 && is_ident_byte(bytes[i - 1]) {
        i -= 1;
    }
    (i < start).then(|| source[i..start].to_string())
}

fn line_of(source: &str, offset: usize) -> usize {
    source[..offset.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

fn line_text<'a>(source: &'a str, offset: usize) -> &'a str {
    let offset = offset.min(source.len());
    let start = source[..offset].rfind('\n').map_or(0, |i| i + 1);
    let end = source[offset..]
        .find('\n')
        .map_or(source.len(), |i| offset + i);
    &source[start..end]
}
