//! T0-04: chrome icons come from one lucide registry.
//!
//! The kit's 16-view `rune_kit::glyphs` belong to kit components; chrome draws lucide (24 view,
//! stroke outline) to match the React UI. That only holds while every chrome icon's SVG body
//! lives in `src/icons.rs`: an inline `Icon { … }` at a call site is a second definition, and two
//! registry constants with the same body are the same icon named twice. This guard reads the
//! source text (the registry is plain statics) and fails with the offending files and lines.

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
    let mut chars = text.chars();
    let mut out = String::new();
    let mut escaped = false;
    for c in chars.by_ref() {
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
    let mut files = Vec::new();
    collect_rs_files(&src, &mut files);

    let registry = src.join("icons.rs");
    let tests_dir = src.join("tests");
    let mut violations = Vec::new();

    for path in files {
        if path == registry || path.starts_with(&tests_dir) {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read rust source");
        for (idx, line) in content.lines().enumerate() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("//") {
                continue;
            }
            let Some(at) = line.find("Icon {") else {
                continue;
            };
            // `-> Icon {` opens a function body, it does not define an icon.
            if line[..at].trim_end().ends_with("->") {
                continue;
            }
            let rel = path
                .strip_prefix(&src)
                .unwrap_or(&path)
                .display()
                .to_string();
            violations.push(format!("src/{}:{}: {}", rel, idx + 1, line.trim()));
        }
    }

    assert!(
        violations.is_empty(),
        "a chrome icon body may only live in src/icons.rs; move these inline definitions into \
         the registry and reference the constant:\n{}",
        violations.join("\n")
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
