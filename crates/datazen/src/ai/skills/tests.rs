use super::*;
use crate::ai::domain::types::{AiSkillsSettings, ToolCall};

fn settings(enabled: bool, custom: Option<&str>) -> AiSkillsSettings {
    AiSkillsSettings {
        enabled,
        custom_dir: custom.map(str::to_string),
        auto_use: true,
        selected: Vec::new(),
    }
}

fn catalog(home: &Path, custom: Option<&Path>) -> SkillCatalog {
    let custom = custom.map(|path| path.to_string_lossy().into_owned());
    SkillCatalog::from_settings_and_home(
        &settings(true, custom.as_deref()),
        Some(home.to_path_buf()),
    )
}

fn write_skill(home: &Path, dir: &str, frontmatter: &str, body: &str) -> PathBuf {
    let root = home.join(DEFAULT_SKILLS_DIR).join(dir);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(SKILL_FILE),
        format!("---\n{frontmatter}\n---\n{body}"),
    )
    .unwrap();
    root
}

#[test]
fn parses_frontmatter_and_falls_back_to_directory_name() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "fallback",
        "description: Uses the directory name\nversion: 2.1",
        "body",
    );
    let catalog = catalog(home.path(), None);
    let skills = catalog.list();
    assert_eq!(skills.len(), 1);
    assert_eq!(skills[0].name, "fallback");
    assert_eq!(skills[0].description, "Uses the directory name");
    assert_eq!(skills[0].version.as_deref(), Some("2.1"));
    assert_eq!(skills[0].source, SkillSource::User);
}

#[test]
fn metadata_is_parsed_only_when_the_body_is_loaded() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha skill\nmetadata:\n  owner: datazen\n  tier: r2",
        "Alpha body",
    );
    let catalog = catalog(home.path(), None);
    let listing = catalog.list();
    assert_eq!(listing[0].name, "alpha");
    let loaded = catalog.use_skill("alpha").unwrap();
    assert_eq!(loaded.content.trim(), "Alpha body");
    assert_eq!(
        loaded.metadata.get("owner").map(String::as_str),
        Some("datazen")
    );
}

#[test]
fn listing_never_contains_body_and_use_reads_it_on_demand() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha description",
        "SECRET_BODY_CONTENT",
    );
    let catalog = catalog(home.path(), None);
    let listing = serde_json::to_string(&catalog.list()).unwrap();
    assert!(!listing.contains("SECRET_BODY_CONTENT"));
    assert!(listing.contains("alpha"));
    assert!(listing.contains("Alpha description"));

    let loaded = catalog.use_skill("alpha").unwrap();
    assert!(loaded.content.contains("SECRET_BODY_CONTENT"));
}

#[test]
fn missing_skill_file_is_ignored() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(DEFAULT_SKILLS_DIR).join("not-a-skill");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("README.md"), "not a skill").unwrap();
    assert!(catalog(home.path(), None).list().is_empty());
}

#[test]
fn resources_are_listed_and_read_from_inside_the_skill() {
    let home = tempfile::tempdir().unwrap();
    let root = write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha",
        "body",
    );
    fs::create_dir_all(root.join("references")).unwrap();
    fs::write(root.join("references/guide.md"), "guide body").unwrap();
    let catalog = catalog(home.path(), None);
    let loaded = catalog.use_skill("alpha").unwrap();
    assert_eq!(loaded.resources, vec!["references/guide.md"]);
    assert_eq!(
        catalog
            .read_skill_file("alpha", "references/guide.md")
            .unwrap(),
        "guide body"
    );
}

#[test]
fn rejects_traversal_absolute_and_symlink_resources() {
    let home = tempfile::tempdir().unwrap();
    let root = write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha",
        "body",
    );
    fs::write(home.path().join("secret.txt"), "outside").unwrap();
    let catalog = catalog(home.path(), None);

    assert!(matches!(
        catalog.read_skill_file("alpha", "../secret.txt"),
        Err(SkillError::InvalidPath(_))
    ));
    assert!(matches!(
        catalog.read_skill_file("alpha", "/etc/passwd"),
        Err(SkillError::InvalidPath(_))
    ));

    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(home.path().join("secret.txt"), root.join("link.txt")).unwrap();
        assert!(matches!(
            catalog.read_skill_file("alpha", "link.txt"),
            Err(SkillError::Symlink(_))
        ));
    }
}

#[test]
fn rejects_oversized_and_non_utf8_files() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(DEFAULT_SKILLS_DIR).join("big");
    fs::create_dir_all(&root).unwrap();
    let mut content = b"---\nname: big\ndescription: Big\n---\n".to_vec();
    content.extend(std::iter::repeat_n(b'a', MAX_SKILL_BYTES as usize + 1));
    fs::write(root.join(SKILL_FILE), content).unwrap();
    let big_catalog = catalog(home.path(), None);
    assert!(matches!(
        big_catalog.use_skill("big"),
        Err(SkillError::TooLarge { .. })
    ));

    let root = home.path().join(DEFAULT_SKILLS_DIR).join("binary");
    fs::create_dir_all(&root).unwrap();
    let mut content = b"---\nname: binary\ndescription: Binary\n---\n".to_vec();
    content.extend([0xff, 0xfe, 0xfd]);
    fs::write(root.join(SKILL_FILE), content).unwrap();
    let binary_catalog = catalog(home.path(), None);
    assert!(matches!(
        binary_catalog.use_skill("binary"),
        Err(SkillError::NotUtf8(_))
    ));
}

#[test]
fn disabled_catalog_has_no_skills_and_rejects_use() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha",
        "body",
    );
    let catalog = SkillCatalog::from_settings_and_home(
        &settings(false, None),
        Some(home.path().to_path_buf()),
    );
    assert!(catalog.list().is_empty());
    assert!(matches!(
        catalog.use_skill("alpha"),
        Err(SkillError::Disabled)
    ));
}

#[test]
fn custom_directory_is_scanned_and_duplicate_names_are_listed_side_by_side() {
    let home = tempfile::tempdir().unwrap();
    let custom = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "shared",
        "name: shared\ndescription: User copy",
        "user",
    );
    let custom_root = custom.path().join("shared");
    fs::create_dir_all(&custom_root).unwrap();
    fs::write(
        custom_root.join(SKILL_FILE),
        "---\nname: shared\ndescription: Custom copy\n---\ncustom",
    )
    .unwrap();
    let catalog = catalog(home.path(), Some(custom.path()));
    let listing = catalog.list();
    assert_eq!(listing.len(), 2);
    assert_eq!(listing[0].name, "shared");
    assert_eq!(listing[1].name, "shared");
    assert_ne!(listing[0].id, listing[1].id);
    assert_eq!(listing[0].source, SkillSource::User);
    assert_eq!(listing[1].source, SkillSource::Custom);
    assert_eq!(listing[0].description, "User copy");
    assert_eq!(listing[1].description, "Custom copy");
    assert!(matches!(
        catalog.use_skill("shared"),
        Err(SkillError::Ambiguous(_))
    ));
    assert_eq!(
        catalog
            .use_skill_by_id(&listing[1].id)
            .unwrap()
            .content
            .trim(),
        "custom"
    );
}

#[test]
fn skill_tools_list_without_body_and_use_returns_body() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha description",
        "PRIVATE_BODY",
    );
    let catalog = catalog(home.path(), None);
    let call = |name: &str, arguments: &str| ToolCall {
        id: "call-1".into(),
        name: name.into(),
        arguments: arguments.into(),
    };
    let listing = tools::execute_skill_tool(&catalog, &call("list_skills", "{}"));
    assert!(!listing.contains("PRIVATE_BODY"));
    assert!(!listing.contains("version"));
    assert!(!listing.contains("source"));
    assert!(listing.contains("Alpha description"));
    let loaded = tools::execute_skill_tool(&catalog, &call("use_skill", r#"{"name":"alpha"}"#));
    assert!(loaded.contains("PRIVATE_BODY"));
    let missing = tools::execute_skill_tool(&catalog, &call("use_skill", r#"{"name":"nope"}"#));
    assert!(missing.starts_with("Error:"));
    assert!(
        !missing.contains(&home.path().display().to_string()),
        "tool errors must not expose host paths: {missing}"
    );
    assert!(tools::skill_tool_definitions()
        .iter()
        .any(|definition| definition.name == "read_skill_file"));
}

#[test]
fn bom_frontmatter_is_listed_and_body_is_not_decoded_for_listing() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join(DEFAULT_SKILLS_DIR).join("bom");
    fs::create_dir_all(&root).unwrap();
    let mut bytes = vec![0xef, 0xbb, 0xbf];
    bytes.extend_from_slice(b"---\nname: bom\ndescription: BOM skill\n---\n\xff\xfe");
    fs::write(root.join(SKILL_FILE), bytes).unwrap();

    let listing = catalog(home.path(), None).list();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "bom");
    assert_eq!(listing[0].description, "BOM skill");
    assert!(matches!(
        catalog(home.path(), None).use_skill("bom"),
        Err(SkillError::NotUtf8(_))
    ));
}

#[test]
fn tilde_custom_directory_expands_against_the_injected_home() {
    let home = tempfile::tempdir().unwrap();
    let root = home.path().join("team-skills").join("team");
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join(SKILL_FILE),
        "---\nname: team\ndescription: Team skill\n---\nTeam body\n",
    )
    .unwrap();

    let catalog = SkillCatalog::from_settings_and_home(
        &settings(true, Some("~/team-skills")),
        Some(home.path().to_path_buf()),
    );
    let listing = catalog.list();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].name, "team");
    assert_eq!(listing[0].source, SkillSource::Custom);
}

fn gated_catalog(home: &Path, auto_use: bool, selected: Vec<String>) -> SkillCatalog {
    SkillCatalog::from_settings_and_home(
        &AiSkillsSettings {
            enabled: true,
            custom_dir: None,
            auto_use,
            selected,
        },
        Some(home.to_path_buf()),
    )
}

#[test]
fn auto_use_off_without_selection_hides_listing_and_blocks_use() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha description",
        "Alpha body",
    );
    let catalog = gated_catalog(home.path(), false, Vec::new());

    assert!(catalog.list().is_empty());
    assert_eq!(catalog.list_all().len(), 1);
    assert!(matches!(
        catalog.use_skill("alpha"),
        Err(SkillError::NotSelected(_))
    ));
    let listing = tools::execute_skill_tool(
        &catalog,
        &ToolCall {
            id: "call-1".into(),
            name: "list_skills".into(),
            arguments: "{}".into(),
        },
    );
    assert_eq!(listing, "[]");
}

#[test]
fn auto_use_off_selected_id_is_visible_and_usable() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "alpha",
        "name: alpha\ndescription: Alpha description",
        "Alpha body",
    );
    let discovered = gated_catalog(home.path(), true, Vec::new());
    let id = discovered.list_all()[0].id.clone();
    let catalog = gated_catalog(home.path(), false, vec![id.clone()]);

    let listing = catalog.list();
    assert_eq!(listing.len(), 1);
    assert_eq!(listing[0].id, id);
    assert_eq!(
        catalog.use_skill_by_id(&id).unwrap().content.trim(),
        "Alpha body"
    );
    let tool_listing = tools::execute_skill_tool(
        &catalog,
        &ToolCall {
            id: "call-1".into(),
            name: "list_skills".into(),
            arguments: "{}".into(),
        },
    );
    assert!(tool_listing.contains(&id));
    assert!(!tool_listing.contains("Alpha body"));
}

#[test]
fn third_distinct_body_is_rejected_before_reading_it() {
    let home = tempfile::tempdir().unwrap();
    write_skill(
        home.path(),
        "one",
        "name: one\ndescription: One",
        "one body",
    );
    write_skill(
        home.path(),
        "two",
        "name: two\ndescription: Two",
        "two body",
    );
    let third = home.path().join(DEFAULT_SKILLS_DIR).join("three");
    fs::create_dir_all(&third).unwrap();
    let mut bytes = b"---\nname: three\ndescription: Three\n---\n".to_vec();
    bytes.extend([0xff, 0xfe]);
    fs::write(third.join(SKILL_FILE), bytes).unwrap();

    let catalog = gated_catalog(home.path(), true, Vec::new());
    assert!(catalog.use_skill("one").is_ok());
    assert!(
        catalog.use_skill("one").is_ok(),
        "reloading is not a new body"
    );
    assert!(catalog.use_skill("two").is_ok());
    assert_eq!(catalog.loaded_skill_ids().len(), 2);
    assert!(matches!(
        catalog.use_skill("three"),
        Err(SkillError::TooManyLoaded { limit: 2 })
    ));
    assert_eq!(catalog.loaded_skill_ids().len(), 2);
}
