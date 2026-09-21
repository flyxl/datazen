//! Tests for the app-data ZIP archive commands.

use super::*;
use std::path::Path;
#[test]
fn export_options_from_settings_reflects_monitor_flag() {
    use crate::store::AppSettings;

    let mut settings = AppSettings::default();
    settings.monitor.export_include_dashboard_runs = false;
    assert!(!export_options_from_settings(&settings).include_dashboard_runs);
    settings.monitor.export_include_dashboard_runs = true;
    assert!(export_options_from_settings(&settings).include_dashboard_runs);
}

#[tokio::test]
async fn app_data_export_lists_store_files_and_import_helper_extracts() {
    use crate::testing::app_state::TestAppState;

    // Export impl: real store dir → ZIP containing the JSON store files.
    let source = TestAppState::new().await;
    source
        .store
        .save_groups(vec!["Alpha".into()])
        .await
        .unwrap();
    let zip_path = source._temp.path().join("app-backup.zip");
    export_app_data_to_dest(&source.state, zip_path.clone())
        .await
        .unwrap();

    let file = std::fs::File::open(&zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    let names: Vec<String> = (0..archive.len())
        .map(|i| archive.by_index(i).unwrap().name().to_string())
        .collect();
    assert!(
        names.iter().any(|n| n.ends_with("groups.json")),
        "export must carry groups.json; entries: {names:?}"
    );

    // Import impl param passthrough: extracts into the target state's data
    // dir. A hand-crafted clean archive keeps live sqlite sidecar files
    // (zero-filled -wal/-shm trip the import zip-bomb ratio guard) out of
    // the fixture.
    let clean_zip = source._temp.path().join("clean.zip");
    {
        use std::io::Write;
        let file = std::fs::File::create(&clean_zip).unwrap();
        let mut writer = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);
        writer.start_file("groups.json", options).unwrap();
        writer.write_all(br#"["Alpha"]"#).unwrap();
        writer.finish().unwrap();
    }

    let target = TestAppState::new().await;
    import_app_data_from_source(
        &target.state,
        clean_zip,
        app_data_archive::ImportOptions::default(),
    )
    .await
    .unwrap();
    let groups_on_disk =
        std::fs::read_to_string(target.state.store.data_dir().join("groups.json")).unwrap();
    assert!(
        groups_on_disk.contains("Alpha"),
        "imported archive must land as groups.json: {groups_on_disk}"
    );
}
