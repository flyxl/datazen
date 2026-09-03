import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import { Button } from '../Button';

describe('Button Variant Audit & Specification', () => {
  it('renders primary variant with accent background and run variant with green query-run', () => {
    const { rerender } = render(<Button variant="primary">Primary Action</Button>);
    const primaryBtn = screen.getByRole('button', { name: 'Primary Action' });
    expect(primaryBtn.className).toContain('bg-accent');
    expect(primaryBtn.className).not.toContain('bg-query-run');

    rerender(<Button variant="run">Run Query</Button>);
    const runBtn = screen.getByRole('button', { name: 'Run Query' });
    expect(runBtn.className).toContain('bg-query-run');
  });

  it('audits that save, create, and commit buttons do not use green run variant', async () => {
    // Import source text or components to verify design system compliance
    const fs = await import('fs');
    const path = await import('path');

    const checkFile = (relPath: string) => {
      const fullPath = path.resolve(__dirname, '../../../../', relPath);
      return fs.readFileSync(fullPath, 'utf-8');
    };

    // TableStructureEditor: save / create table must NOT use variant="run"
    const tableStructureContent = checkFile('src/windows/connection/TableStructureEditor.tsx');
    expect(tableStructureContent).not.toMatch(/variant="run"[\s\S]*?struct-editor-execute/);

    // TableView: commit pending changes must NOT use variant="run"
    const tableViewContent = checkFile('src/windows/connection/TableView.tsx');
    expect(tableViewContent).not.toMatch(/variant="run"[\s\S]*?pending-commit/);

    // PrivilegeView GrantDialog: grant action must NOT use variant="run"
    const privilegeViewContent = checkFile('src/windows/connection/PrivilegeView.tsx');
    expect(privilegeViewContent).not.toMatch(/variant="run"[\s\S]*?privileges\.grant/);

    // DataCleanupSection: data cleanup execution must NOT use variant="run"
    const dataCleanupContent = checkFile('src/windows/settings/DataCleanupSection.tsx');
    expect(dataCleanupContent).not.toMatch(/variant="run"[\s\S]*?data-cleanup-run/);

    // BackupWindow: start backup / restore must NOT use variant="run"
    const backupWindowContent = checkFile('src/windows/backup/BackupWindow.tsx');
    expect(backupWindowContent).not.toMatch(/variant="run"[\s\S]*?backup-start/);
  });
});
