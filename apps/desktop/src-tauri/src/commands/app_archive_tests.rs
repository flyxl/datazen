//! Tests for the app-data ZIP archive commands.

use super::*;

#[test]
fn app_archive_reads_settings_through_the_application_services() {
    let production = include_str!("app_archive.rs")
        .split("#[cfg(test)]")
        .next()
        .expect("app_archive.rs 必须有生产半边");
    assert!(
        production.contains("get_settings_impl(state)")
            && !production.contains("state.store.get_settings"),
        "归档选项必须经设置用例读取，不能直读宿主 store"
    );
    assert_eq!(
        production.matches("state.store").count(),
        2,
        "归档命令只允许保留 export/import 的 data_dir() 宿主路径访问"
    );
}

#[test]
fn export_options_from_settings_reflects_monitor_flag() {
    // 归档选项吃共享行为域（F4a 分域后 get_settings_impl 返回 SharedSettings）；
    // 用 host 全量默认值的共享投影构造测试输入，贴近真实读取路径。
    let mut settings = crate::store::AppSettings::default().shared_settings();
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
