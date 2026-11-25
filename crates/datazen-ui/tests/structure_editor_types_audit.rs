//! Independent tester audit for T156 (`lib/structureEditor/types.ts` contract).
//!
//! The port's own tests round-trip the obvious enums; these probes pin the
//! parts that are easy to get subtly wrong and that its `..Default::default()`
//! construction cannot see:
//!
//! 1. all 20 `StructureCapabilities` fields must be constructible by their
//!    exact snake_case names (17 booleans + alterStrategy/dialectId/indexMethods);
//! 2. the full UiConfig mirror's nested option structs default to React's
//!    "absent" values, and `to_legacy` on that default stays empty;
//! 3. a source guard: no duplicate `pub(crate) struct/enum` definitions across
//!    the structure_editor module and no bare panic/unwrap in the new types file.

use std::collections::HashMap;
use std::path::Path;

use datazen_ui::structure_editor::control_hints::is_control_enabled;
use datazen_ui::structure_editor::types::{
    ExtensionStructureEditorUiConfig, StructureEditorColumnTypeOption, StructureEditorFields,
};
use datazen_ui::structure_editor::{
    AlterStrategy, StatementRisk, StructureCapabilities, StructureCapabilityFlag,
};

#[test]
fn all_twenty_capability_fields_are_constructible_by_their_exact_names() {
    // React `StructureCapabilities` has 20 properties: 17 booleans plus
    // alterStrategy, dialectId and indexMethods. Writing every field literally
    // (no `..Default`) proves the complete field surface and its names.
    let caps = StructureCapabilities {
        create_table: true,
        add_column: true,
        drop_column: true,
        rename_column: true,
        alter_type: true,
        alter_nullability: true,
        alter_default: true,
        alter_primary_key: true,
        reorder_column: true,
        comment: true,
        create_index: true,
        drop_index: true,
        rebuild_index: true,
        index_type: true,
        index_include: true,
        index_filter: true,
        index_comment: true,
        alter_strategy: AlterStrategy::SqliteRebuild,
        dialect_id: "postgresql".to_owned(),
        index_methods: vec!["btree".to_owned(), "gin".to_owned()],
    };
    for flag in StructureCapabilityFlag::ALL {
        assert!(
            is_control_enabled(&caps, flag),
            "explicitly-true field for {flag:?} must be readable"
        );
    }
    assert_eq!(caps.alter_strategy, AlterStrategy::SqliteRebuild);
    assert_eq!(caps.dialect_id, "postgresql");
    assert_eq!(caps.index_methods, vec!["btree".to_owned(), "gin".to_owned()]);
    assert_eq!(StatementRisk::Rewrite.as_str(), "rewrite");
}

#[test]
fn ui_config_nested_defaults_are_absent_and_project_to_an_empty_legacy() {
    let fields = StructureEditorFields::default();
    assert_eq!(fields.comment, None);
    assert_eq!(fields.charset, None);
    assert_eq!(fields.collation, None);
    assert_eq!(fields.unsigned, None);
    assert_eq!(fields.length, None);

    let option = StructureEditorColumnTypeOption::default();
    assert_eq!(option.value, "");
    assert_eq!(option.label, "");

    let full = ExtensionStructureEditorUiConfig::default();
    assert_eq!(full.enabled, None);
    assert!(full.column_types.is_empty());
    assert_eq!(full.default_column_type, "");
    assert!(full.index_methods.is_empty());

    let legacy = full.to_legacy();
    assert_eq!(legacy.enabled, None, "default config has no enabled flag");
    assert_eq!(legacy.default_column_type, "", "empty default projects verbatim");
}

#[test]
fn no_duplicate_contract_definitions_and_no_bare_panics_in_the_new_types_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/structure_editor");
    let mut defined: HashMap<String, String> = HashMap::new();
    let mut sources: Vec<(String, String)> = Vec::new();

    let entries = std::fs::read_dir(&root).expect("structure_editor directory exists");
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let text = std::fs::read_to_string(&path).expect("structure_editor source is readable");
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_owned();
        for line in text.lines() {
            let trimmed = line.trim_start();
            for prefix in ["pub struct ", "pub enum "] {
                if let Some(rest) = trimmed.strip_prefix(prefix) {
                    let ident: String = rest
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if !ident.is_empty() {
                        if let Some(previous) = defined.insert(ident.clone(), name.clone()) {
                            panic!("duplicate {ident} defined in {previous} and {name}");
                        }
                    }
                }
            }
        }
        sources.push((name, text));
    }

    assert!(
        defined.contains_key("StructureCapabilities")
            && defined.contains_key("StructureCapabilityFlag")
            && defined.contains_key("ExtensionStructureEditorUiConfig")
            && defined.contains_key("StructureEditorFields")
            && defined.contains_key("StructureEditorColumnTypeOption"),
        "the T156 contract types must be defined exactly once: {defined:?}"
    );

    let (_, types_src) = sources
        .into_iter()
        .find(|(name, _)| name == "types.rs")
        .expect("types.rs exists");
    for forbidden in [".unwrap()", ".expect(", "todo!()", "unimplemented!()", "panic!("] {
        assert!(
            !types_src.contains(forbidden),
            "production types.rs must not contain {forbidden}"
        );
    }
}
