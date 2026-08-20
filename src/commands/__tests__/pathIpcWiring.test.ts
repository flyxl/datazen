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

  it('adb command wrapper exposes dialog IPC', () => {
    const src = readSrc('commands/adb.ts');
    expect(src).toContain("'adb_pull_database_with_dialog'");
    expect(src).toContain('adbPullDatabaseWithDialog');
  });

  it('backup command wrapper exposes encryption key dialog IPC', () => {
    const backup = readSrc('commands/backup.ts');
    expect(backup).toContain("'save_encryption_key_with_dialog'");
    expect(backup).toContain('saveEncryptionKeyWithDialog');

    const connectionPage = readSrc('windows/connection/ConnectionPage.tsx');
    expect(connectionPage).toContain('saveEncryptionKeyWithDialog');
    expect(connectionPage).toContain('appData.backupKeyTitle');
  });

  it('connection share uses dialog IPC and menu events', () => {
    const connection = readSrc('commands/connection.ts');
    expect(connection).toContain("'export_connections_with_dialog'");
    expect(connection).toContain('exportConnectionsWithDialog');
    expect(connection).toContain("'import_connections_with_dialog'");
    expect(connection).toContain('importConnectionsWithDialog');
    expect(connection).toContain("'import_connections_from_app'");
    expect(connection).toContain('importConnectionsFromApp');
    expect(connection).toContain("'detect_connection_import_path'");
    expect(connection).toContain("'pick_connection_import_path_with_dialog'");

    const rustConfig = fs.readFileSync(
      path.join(ROOT, '../src-tauri/src/commands/config.rs'),
      'utf8',
    );
    expect(rustConfig).toContain('pub async fn pick_connection_import_path_with_dialog');
    expect(rustConfig).toContain('run_blocking_dialog');
    const pickFn = rustConfig.slice(
      rustConfig.indexOf('pub async fn pick_connection_import_path_with_dialog'),
      rustConfig.indexOf('pub async fn import_connections_from_app'),
    );
    expect(pickFn).toContain('blocking_pick_file');
    expect(pickFn).toContain('blocking_pick_folder');
    expect(pickFn).not.toContain('.pick_file(');
    expect(pickFn).not.toContain('.pick_folder(');

    const connectionPage = readSrc('windows/connection/ConnectionPage.tsx');
    expect(connectionPage).toContain('ConnectionShareDialog');
    expect(connectionPage).toContain('menu:export-connections');
    expect(connectionPage).toContain('menu:import-connections');
    expect(connectionPage).toContain('menu:import-connections-dbx');
    expect(connectionPage).toContain('menu:import-connections-navicat');

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

    const wm = readSrc('lib/windowManager.ts');
    expect(wm).toContain('buildDocsUrl');
    expect(wm).not.toContain("openSingletonWindow('docs-singleton'");

    const docsUrls = readSrc('lib/docsUrls.ts');
    expect(docsUrls).toContain('flyxl.github.io/datazen/docs.html');

    const rustMenu = fs.readFileSync(path.join(ROOT, '../src-tauri/src/lib.rs'), 'utf8');
    expect(rustMenu).toContain('register_handler_once');
    expect(rustMenu).toContain('take_once_slot');
  });
});
