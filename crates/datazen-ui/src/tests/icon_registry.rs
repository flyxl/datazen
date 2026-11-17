//! T0-04: chrome icons come from one lucide registry.
//!
//! The kit's 16-view `rune_kit::glyphs` belong to kit components; chrome draws lucide (24 view,
//! stroke outline) to match the React UI. That only holds while every chrome icon's SVG body
//! lives in `src/icons.rs`: an inline `Icon { … }` at a call site is a second definition, and two
//! registry constants with the same body are the same icon named twice. This guard reads the
//! source text (the registry is plain statics) and fails with the offending files and lines.
//!
//! Coverage: the scan walks `crates/datazen-ui/src/**/*.rs` and skips `src/icons.rs` and
//! `src/tests/`. It sees definitions written as Rust source — `Icon {` (rustfmt's shape) or
//! `Icon{` — including one inside a single-line function body. It does not see raw SVG strings
//! held outside an `Icon` literal, `include_str!`/`include_bytes!` payloads, type aliases for
//! `Icon`, or icons synthesized by a macro; those are outside the text-level contract this guard
//! enforces. `crates/datazen-rune-host/src/tray.rs` is a different crate and its `TrayIcon` is an
//! OS tray pixel buffer, not a chrome SVG — a deliberate exception.

use std::fs;
use std::path::{Path, PathBuf};

fn collect_rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).expect("read source directory");
    for entry in entries {
        let entry = entry.expect("read directory entry");
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Offsets in `line` where an `Icon {`/`Icon{` literal starts a definition, not a return type.
fn inline_icon_offsets(line: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    let mut from = 0;
    while let Some(rel) = line[from..].find("Icon") {
        let at = from + rel;
        let rest = &line[at + "Icon".len()..];
        if rest.trim_start().starts_with('{') && !is_type_position(line, at) {
            offsets.push(at);
        }
        from = at + "Icon".len();
    }
    offsets
}

/// True when the `Icon` at `at` is a return type (`-> Icon`, `-> &'static rune::Icon`, …) or the
/// head of an `impl Icon` block, i.e. a mention of the type rather than a definition.
fn is_type_position(line: &str, at: usize) -> bool {
    let prefix = line[..at].trim_end();
    if prefix.ends_with("impl") {
        return true;
    }
    let Some(arrow) = prefix.rfind("->") else {
        return false;
    };
    let ret = line[arrow + 2..at + "Icon".len()].trim();
    let ret = ret.strip_prefix('&').map(str::trim).unwrap_or(ret);
    let ret = match ret.strip_prefix('\'') {
        Some(after) => match after.split_once(char::is_whitespace) {
            Some((_, type_name)) => type_name.trim(),
            None => ret,
        },
        None => ret,
    };
    let ret = ret.strip_prefix("rune::").map(str::trim).unwrap_or(ret);
    ret == "Icon"
}

/// Lines under `dir` that define an icon inline, formatted `path:line: source`. `skip` paths are
/// left out (the registry itself and the test tree in production use).
fn inline_icon_violations(dir: &Path, skip: &[&Path]) -> Vec<String> {
    let mut files = Vec::new();
    collect_rs_files(dir, &mut files);
    let mut violations = Vec::new();
    for path in files {
        if skip.iter().any(|skipped| path.starts_with(skipped)) {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read rust source");
        for (idx, line) in content.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            if inline_icon_offsets(line).is_empty() {
                continue;
            }
            violations.push(format!("{}:{}: {}", path.display(), idx + 1, line.trim()));
        }
    }
    violations
}

/// One `ICON_*` static: its name, its declared view size and its SVG body.
struct RegistryIcon {
    name: String,
    view: f32,
    body: String,
    line: usize,
}

fn registry_icons(source: &str) -> Vec<RegistryIcon> {
    const PREFIX: &str = "pub(crate) static ICON_";
    let mut icons = Vec::new();
    let mut from = 0;
    while let Some(offset) = source[from..].find(PREFIX) {
        let start = from + offset;
        let line = source[..start].matches('\n').count() + 1;
        let name_start = start + "pub(crate) static ".len();
        let name_end = name_start
            + source[name_start..]
                .find(':')
                .expect("an ICON_* static names a type");
        let name = source[name_start..name_end].trim().to_owned();

        let next = source[start + PREFIX.len()..]
            .find(PREFIX)
            .map(|i| start + PREFIX.len() + i)
            .unwrap_or(source.len());
        let segment = &source[name_end..next];

        let view = segment
            .find("view: ")
            .and_then(|i| segment[i + "view: ".len()..].split(',').next())
            .and_then(|value| value.trim().parse::<f32>().ok())
            .unwrap_or(f32::NAN);

        let body = segment
            .find("body: \"")
            .map(|i| read_quoted(&segment[i + "body: \"".len()..]))
            .unwrap_or_default();

        icons.push(RegistryIcon {
            name,
            view,
            body,
            line,
        });
        from = next;
    }
    icons
}

/// The contents of a leading `"…"` literal, honouring `\"` and `\\`.
fn read_quoted(text: &str) -> String {
    let mut out = String::new();
    let mut escaped = false;
    for c in text.chars() {
        if escaped {
            out.push(c);
            escaped = false;
        } else if c == '\\' {
            escaped = true;
        } else if c == '"' {
            break;
        } else {
            out.push(c);
        }
    }
    out
}

#[test]
fn no_inline_icon_definitions_outside_the_registry() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let registry = src.join("icons.rs");
    let tests_dir = src.join("tests");
    let violations = inline_icon_violations(&src, &[&registry, &tests_dir]);

    assert!(
        violations.is_empty(),
        "a chrome icon body may only live in src/icons.rs; move these inline definitions into \
         the registry and reference the constant:\n{}",
        violations.join("\n")
    );
}

#[test]
fn a_single_line_function_body_is_still_scanned() {
    let dir = std::env::temp_dir().join(format!("datazen-icon-guard-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create the probe directory");
    let probe = dir.join("probe.rs");
    fs::write(
        &probe,
        "fn probe() -> Icon { Icon { name: \"probe\", view: 24.0, body: \"<path d=\\\"M0 0\\\"/>\" } }\n",
    )
    .expect("write the probe source");

    let violations = inline_icon_violations(&dir, &[]);
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(
        violations.len(),
        1,
        "an inline icon inside a single-line function body must be reported:\n{violations:#?}"
    );
    assert!(
        violations[0].contains("probe.rs:1:"),
        "the report names the probe file and line: {violations:#?}"
    );
}

#[test]
fn a_typed_icon_reference_is_not_a_definition() {
    for line in [
        "fn audit_ref() -> &'static rune::Icon { &crate::icons::ICON_X }",
        "fn plain() -> Icon { crate::icons::ICON_X }",
        "fn borrowed() -> &Icon { &crate::icons::ICON_X }",
        "fn section(section: Section) -> &'static rune::Icon {",
        "impl Icon {",
    ] {
        assert!(
            inline_icon_offsets(line).is_empty(),
            "a return type or impl head is not an inline definition: {line}"
        );
    }
    let definition = "static PROBE: Icon = Icon{ name: \"probe\", view: 24.0, body: \"\" };";
    assert_eq!(
        inline_icon_offsets(definition),
        vec![21],
        "an unspaced `Icon{{` is a definition"
    );
}

#[test]
fn every_registry_icon_is_a_24_view_with_a_body() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/icons.rs");
    let source = fs::read_to_string(&path).expect("read the icon registry");
    let icons = registry_icons(&source);
    assert!(
        icons.len() >= 25,
        "the registry lost icons: parsed {}",
        icons.len()
    );

    let mut broken = Vec::new();
    for icon in &icons {
        if icon.view != 24.0 {
            broken.push(format!(
                "src/icons.rs:{}: {} view {} (chrome icons are 24 view)",
                icon.line, icon.name, icon.view
            ));
        }
        if icon.body.is_empty() {
            broken.push(format!(
                "src/icons.rs:{}: {} has an empty body",
                icon.line, icon.name
            ));
        }
    }
    assert!(
        broken.is_empty(),
        "chrome icons must be 24-view and non-empty:\n{}",
        broken.join("\n")
    );
}

#[test]
fn no_two_registry_icons_share_a_body() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/icons.rs");
    let source = fs::read_to_string(&path).expect("read the icon registry");
    let icons = registry_icons(&source);

    let mut by_body: Vec<(String, Vec<(usize, String)>)> = Vec::new();
    for icon in &icons {
        if icon.body.is_empty() {
            continue;
        }
        match by_body.iter_mut().find(|(body, _)| *body == icon.body) {
            Some((_, names)) => names.push((icon.line, icon.name.clone())),
            None => by_body.push((icon.body.clone(), vec![(icon.line, icon.name.clone())])),
        }
    }
    let copies: Vec<String> = by_body
        .into_iter()
        .filter(|(_, names)| names.len() > 1)
        .map(|(_, names)| {
            let listed: Vec<String> = names
                .into_iter()
                .map(|(line, name)| format!("src/icons.rs:{line}: {name}"))
                .collect();
            listed.join(" and ")
        })
        .collect();

    assert!(
        copies.is_empty(),
        "the same icon body is defined twice; keep one constant and point both call sites at \
         it:\n{}",
        copies.join("\n")
    );
}
