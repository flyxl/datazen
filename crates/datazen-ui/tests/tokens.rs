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

#[test]
fn no_oklch_literals_outside_palette_module() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    collect_rs_files(&src, &mut files);

    let palette = src.join("ui").join("palette.rs");
    let tests_dir = src.join("tests");
    let mut violations = Vec::new();

    for path in files {
        if path == palette || path.starts_with(&tests_dir) {
            continue;
        }
        let content = fs::read_to_string(&path).expect("read rust source");
        for (idx, line) in content.lines().enumerate() {
            if line.contains("oklch(") || line.contains("oklcha(") {
                let rel = path.strip_prefix(&src).unwrap_or(&path).display().to_string();
                violations.push(format!("src/{}:{}: {}", rel, idx + 1, line.trim()));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "oklch/oklcha literals must live only in src/ui/palette.rs (outside src/tests):\n{}",
        violations.join("\n")
    );
}
