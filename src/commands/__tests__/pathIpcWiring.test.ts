import fs from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';

const ROOT = path.resolve(import.meta.dirname, '../..');

function readSrc(...parts: string[]) {
  return fs.readFileSync(path.join(ROOT, ...parts), 'utf8');
}

describe('path IPC frontend wiring', () => {
  it('ADB UI uses dialog pull only', () => {
    const src = readSrc('components/connection/FileConnectionFields.tsx');
    expect(src).toContain('adbPullDatabaseWithDialog');
    expect(src).not.toMatch(/\badbPullDatabase\s*\(/);
    expect(src).not.toContain('localSavePath');
  });

  it('settings open helpers use dedicated open_* commands', () => {
    const settingsCmd = readSrc('commands/settings.ts');
    expect(settingsCmd).toContain("invoke<void>('open_log_dir')");
    expect(settingsCmd).toContain("invoke<void>('open_workflows_dir')");
    expect(settingsCmd).toContain("invoke<void>('open_context_dir')");

    const settingsContent = readSrc('windows/settings/SettingsContent.tsx');
    expect(settingsContent).toContain('openLogDir');
    expect(settingsContent).not.toMatch(/openPath\(\s*(localDir|dir|defaultDir)/);

    const aiSettings = readSrc('windows/settings/AiSettingsSection.tsx');
    expect(aiSettings).toContain('openContextDir');

    const workflowPage = readSrc('windows/workflow/WorkflowPage.tsx');
    expect(workflowPage).toContain('openWorkflowsDir');
  });

  it('adb command wrapper routes through execute_driver_command (decision 2)', () => {
    const src = readSrc('commands/adb.ts');
    // Unified Driver Command API entry point — no dedicated host adb IPCs.
    expect(src).toContain('driverCommands.execute');
    expect(src).toContain("driverType: ADB_DRIVER_TYPE");
    expect(src).toContain("'adb_pull_database'");
    expect(src).toContain('savedPath');
    // The legacy direct Tauri IPC invocations must be gone.
    expect(src).not.toContain('@tauri-apps/api/core');
    expect(src).not.toMatch(/invoke[<(]/);

    const hostLib = fs.readFileSync(
      path.join(ROOT, '../src-tauri/src/lib.rs'),
      'utf8',
    );
    expect(hostLib).not.toContain('adb_list_packages');
    expect(hostLib).not.toContain('adb_pull_database');

    const hostMod = fs.readFileSync(
      path.join(ROOT, '../src-tauri/src/commands/mod.rs'),
      'utf8',
    );
    expect(hostMod).not.toContain('mod adb');
    expect(fs.existsSync(path.join(ROOT, '../src-tauri/src/commands/adb.rs'))).toBe(false);
  });

  it('backup command wrapper exposes encryption key dialog IPC', () => {
    const backup = readSrc('commands/backup.ts');
    expect(backup).toContain("'save_encryption_key_with_dialog'");
    expect(backup).toContain('saveEncryptionKeyWithDialog');

    const connectionPage = readSrc('windows/connection/ConnectionPage.tsx');
    expect(connectionPage).toContain('saveEncryptionKeyWithDialog');
    expect(connectionPage).toContain('appData.backupKeyTitle');
  });

  it('backup/restore use the merged override_path IPCs only (decision 3+6)', () => {
    const GONE = [
      'backup_database_with_dialog',
      'restore_database',
      'restore_database_with_dialog',
      'execute_sql_file',
      'execute_sql_file_with_dialog',
    ];

    // Restore window: production flow goes through the dialog (no override).
    const restoreWindow = readSrc('windows/backup/BackupWindow.tsx');
    expect(restoreWindow).toContain("invoke<boolean>('backup_database'");
    expect(restoreWindow).not.toContain("'backup_database_with_dialog'");

    // Unified SQL-file wrapper: single command, no per-flow command switch.
    const sqlFileExecution = readSrc('lib/sqlFileExecution.ts');
    expect(sqlFileExecution).toContain("invoke<boolean>('restore_sql_file'");
    expect(sqlFileExecution).not.toContain('command?:');

    for (const gone of GONE) {
      expect(restoreWindow).not.toContain(gone);
      expect(sqlFileExecution).not.toContain(gone);
      expect(readSrc('windows/connection/ExecuteSqlFileDialog.tsx')).not.toContain(gone);
    }

    // Host registration surface matches the merge.
    const hostLib = fs.readFileSync(path.join(ROOT, '../src-tauri/src/lib.rs'), 'utf8');
    expect(hostLib).toContain('commands::backup_database,');
    expect(hostLib).toContain('commands::restore_sql_file,');
    expect(hostLib).toContain('commands::save_encryption_key_with_dialog');
    expect(hostLib).not.toContain('commands::backup_database_with_dialog');
    expect(hostLib).not.toContain('commands::restore_database,');
    expect(hostLib).not.toContain('commands::execute_sql_file');

    // The raw-path parameters stay out of the frontend; override_path is an
    // E2E-only concern and must not appear in any production caller.
    expect(restoreWindow).not.toContain('overridePath');
    expect(sqlFileExecution).not.toContain('overridePath');
  });

  it('import/export commands use the merged override_path IPCs only (decision 3, f4)', () => {
    const GONE_IPCS = [
      'export_connections_with_dialog',
      'export_app_data_with_dialog',
      'import_app_data_with_dialog',
    ];
    const GONE_WRAPPERS = [
      'exportConnectionsWithDialog',
      'importConnectionsWithDialog',
      'exportAppDataWithDialog',
      'importAppDataWithDialog',
    ];

    // Wrapper layer: single invoke per merged command, dialog-era params only.
    const connection = readSrc('commands/connection.ts');
    expect(connection).toContain("invoke<number | null>('export_connections'");
    expect(connection).toContain("'import_connections_preview'");
    expect(connection).toContain("'import_connections_with_dialog'");

    const backup = readSrc('commands/backup.ts');
    expect(backup).toContain("invoke<boolean>('export_app_data'");
    expect(backup).toContain("invoke<boolean>('import_app_data'");

    for (const gone of GONE_IPCS) {
      expect(connection).not.toContain(`'${gone}'`);
      expect(backup).not.toContain(`'${gone}'`);
    }
    for (const gone of GONE_WRAPPERS) {
      expect(connection).not.toContain(gone);
      expect(backup).not.toContain(gone);
    }

    // Production callers: dialog flow only, no overridePath anywhere.
    const shareDialog = readSrc('components/connection/ConnectionShareDialog.tsx');
    expect(shareDialog).toContain('connectionCommands.exportConnections(');
    expect(shareDialog).toContain('connectionCommands.importConnections(');
    const connectionPage = readSrc('windows/connection/ConnectionPage.tsx');
    expect(connectionPage).toContain('backupCommands.exportAppData(');
    expect(connectionPage).toContain('backupCommands.importAppData(');
    for (const prod of [connection, backup, shareDialog, connectionPage]) {
      expect(prod).not.toContain('overridePath');
      for (const gone of GONE_WRAPPERS) {
        expect(prod).not.toContain(gone);
      }
    }

    // Host registration surface matches the merge.
    const hostLib = fs.readFileSync(path.join(ROOT, '../src-tauri/src/lib.rs'), 'utf8');
    for (const kept of [
      'commands::export_connections,',
      'commands::import_connections_preview,',
      'commands::import_connections_with_dialog,',
      'commands::export_app_data,',
      'commands::import_app_data,',
    ]) {
      expect(hostLib).toContain(kept);
    }
    for (const gone of GONE_IPCS) {
      expect(hostLib).not.toContain(`commands::${gone},`);
    }
  });

  it('connection share uses dialog IPC and menu events', () => {
    const connection = readSrc('commands/connection.ts');
    expect(connection).toContain("'export_connections'");
    expect(connection).toContain('exportConnections');
    expect(connection).toContain("'import_connections_with_dialog'");
    expect(connection).toContain('importConnections');
    expect(connection).toContain("'import_connections_from_app'");
    expect(connection).toContain('importConnectionsFromApp');
    expect(connection).toContain("'detect_connection_import_path'");
    expect(connection).toContain("'pick_connection_import_path_with_dialog'");

    const rustConfig = fs.readFileSync(
      path.join(ROOT, '../src-tauri/src/commands/config.rs'),
      'utf8',
    );
    expect(rustConfig).toContain('pub async fn pick_connection_import_path_with_dialog');
    // R-1.3: native dialog invocation is centralised in commands/dialog.rs.
    // The picker routes through the gateway and the blocking execution shim
    // (macOS anti-freeze pattern) lives there — config.rs must not call
    // plugin-dialog directly.
    const rustDialog = fs.readFileSync(
      path.join(ROOT, '../src-tauri/src/commands/dialog.rs'),
      'utf8',
    );
    expect(rustDialog).toContain('async fn run_blocking_dialog');
    expect(rustDialog).toContain('blocking_pick_file');
    expect(rustDialog).toContain('blocking_pick_folder');
    const pickFn = rustConfig.slice(
      rustConfig.indexOf('pub async fn pick_connection_import_path_with_dialog'),
      rustConfig.indexOf('pub async fn import_connections_from_app'),
    );
    expect(pickFn).toContain('super::dialog::pick_folder');
    expect(pickFn).toContain('super::dialog::open_file');
    expect(pickFn).not.toContain('.pick_file(');
    expect(pickFn).not.toContain('.pick_folder(');

    const mainPage = readSrc('windows/main/MainPage.tsx');
    expect(mainPage).toContain('ConnectionShareDialogHost');
    expect(mainPage).toContain('menu:export-connections');
    expect(mainPage).toContain('menu:import-connections');
    expect(mainPage).toContain('menu:import-connections-dbx');
    expect(mainPage).toContain('menu:import-connections-navicat');

    const connectionPage = readSrc('windows/connection/ConnectionPage.tsx');
    expect(connectionPage).toContain('openConnectionShareDialog');

    const menuBar = readSrc('components/MenuBar.tsx');
    expect(menuBar).toContain('export-connections');
    expect(menuBar).toContain('import-connections');
    expect(menuBar).toContain('import-connections-dbx');
    expect(menuBar).toContain('import-connections-file');
  });

  it('overlay chrome has a drag fallback and docs open via official URL', () => {
    const app = readSrc('App.tsx');
    expect(app).toContain('WindowChromeFallback');
    expect(app).not.toContain('fallback={null}');
    expect(app).not.toContain('DocsWindow');
    expect(app).not.toContain('NewConnectionWindow');

    const wm = readSrc('lib/windowManager.ts');
    expect(wm).toContain('buildDocsUrl');
    expect(wm).not.toContain("openSingletonWindow('docs-singleton'");
    expect(wm).not.toContain("openSingletonWindow('new-connection");
    expect(wm).not.toContain("openSingletonWindow('settings-singleton'");

    const hostCaps = fs.readFileSync(
      path.join(ROOT, '../src-tauri/capabilities/default.json.host'),
      'utf8',
    );
    expect(hostCaps).not.toContain('settings-singleton');
    expect(hostCaps).not.toContain('docs-singleton');
    expect(hostCaps).not.toContain('new-connection-*');

    const docsUrls = readSrc('lib/docsUrls.ts');
    expect(docsUrls).toContain('flyxl.github.io/datazen/manual.html');

    const rustMenu = fs.readFileSync(path.join(ROOT, '../src-tauri/src/lib.rs'), 'utf8');
    expect(rustMenu).toContain('register_handler_once');
    expect(rustMenu).toContain('take_once_slot');
  });
});
