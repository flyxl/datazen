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

    const settingsWin = readSrc('windows/settings/SettingsWindow.tsx');
    expect(settingsWin).toContain('openLogDir');
    expect(settingsWin).not.toMatch(/openPath\(\s*(localDir|dir|defaultDir)/);

    const aiSettings = readSrc('windows/settings/AiSettingsSection.tsx');
    expect(aiSettings).toContain('openContextDir');

    const workflowWindow = readSrc('windows/workflow/WorkflowWindow.tsx');
    expect(workflowWindow).toContain('openWorkflowsDir');
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

    const main = readSrc('windows/main/MainWindow.tsx');
    expect(main).toContain('saveEncryptionKeyWithDialog');
    expect(main).toContain('appData.backupKeyTitle');
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

    const main = readSrc('windows/main/MainWindow.tsx');
    expect(main).toContain('ConnectionShareDialog');
    expect(main).toContain('menu:export-connections');
    expect(main).toContain('menu:import-connections');
    expect(main).toContain('menu:import-connections-dbx');
    expect(main).toContain('menu:import-connections-navicat');

    const menuBar = readSrc('components/MenuBar.tsx');
    expect(menuBar).toContain('export-connections');
    expect(menuBar).toContain('import-connections');
    expect(menuBar).toContain('import-connections-dbx');
    expect(menuBar).toContain('import-connections-file');
  });
});
