# SQL Snippets Settings UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provide an intuitive in-app management interface for SQL snippets in the Settings Editor section, allowing users to view built-in and custom snippets, create/edit/delete snippets via a modal dialog, duplicate built-in templates, import/export settings, and immediately use custom snippets in CodeMirror autocomplete.

**Architecture:** Create `SqlSnippetsCard` to embed in `SettingsContent`'s editor tab, featuring a combined built-in/custom snippets list with search/preview and import/export actions. Implement `SnippetEditDialog` for creating/editing/duplicating snippet templates with CodeMirror `${n:placeholder}` syntax hints and prefix validation. Wire user snippets from `settingsStore` through `SqlEditor` and `editorExtensions` into `createSnippetCompletionSource` to complete the end-to-end autocomplete journey.

**Tech Stack:** React 18, TypeScript, Tailwind CSS 4, `@datazen/ui`, Lucide Icons, Vitest, `@testing-library/react`.

## Global Constraints

- Adhere to single-file complexity limits (<800 lines); split logic into high-cohesion subcomponents (`SqlSnippetsCard.tsx`, `SnippetEditDialog.tsx`).
- Production code must never use bare `unwrap()` or `expect()`.
- Follow soft-boosting and state machine principles for editor completion; do not regress existing CodeMirror completion behavior.
- Only modify `src/locales/en/query.ts` and `src/locales/zh-CN/query.ts` for translations during feature development.
- Safe settings export/import compatibility: preserve `AppSettings.sqlSnippets` contract.

---

### Task 1: Add i18n Translation Keys for Snippets Management

**Files:**
- Modify: `src/locales/en/query.ts`
- Modify: `src/locales/zh-CN/query.ts`
- Test: `src/locales/locales.test.ts`

**Interfaces:**
- Consumes: Existing translation key map in `query.ts`.
- Produces: New translation keys under `query.snippets.*` accessible via `t('query.snippets.add')`, etc.

- [ ] **Step 1: Write the failing test**

Check `src/locales/locales.test.ts` and add an assertion for the new snippet keys:

```ts
// in src/locales/locales.test.ts
it('contains snippet management keys in en and zh-CN', () => {
  expect(getTranslation('en', 'query.snippets.add')).toBe('Add Snippet');
  expect(getTranslation('zh-CN', 'query.snippets.add')).toBe('新增片段');
  expect(getTranslation('en', 'query.snippets.builtin')).toBe('Built-in');
  expect(getTranslation('zh-CN', 'query.snippets.builtin')).toBe('内置');
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/locales/locales.test.ts`
Expected: FAIL due to missing keys `'query.snippets.add'`.

- [ ] **Step 3: Add translation keys**

In `src/locales/en/query.ts`, add:

```ts
  'query.snippets.add': 'Add Snippet',
  'query.snippets.edit': 'Edit Snippet',
  'query.snippets.duplicate': 'Duplicate as Custom',
  'query.snippets.builtin': 'Built-in',
  'query.snippets.custom': 'Custom',
  'query.snippets.prefix': 'Prefix',
  'query.snippets.prefixPlaceholder': 'e.g. selw',
  'query.snippets.prefixHint': 'Letters, numbers, underscores and asterisk (*)',
  'query.snippets.prefixInvalid': 'Prefix must start with a letter/underscore and contain only letters, numbers, _, *',
  'query.snippets.description': 'Description',
  'query.snippets.descriptionPlaceholder': 'e.g. Select with group by',
  'query.snippets.template': 'SQL Template',
  'query.snippets.templatePlaceholder': 'SELECT * FROM ${1:table_name} WHERE ${2:condition};${3}',
  'query.snippets.templateRequired': 'Template is required',
  'query.snippets.syntaxGuide': 'Use ${1:placeholder} for Tab stop navigation. Trailing ${N} ends the session.',
  'query.snippets.deleteConfirmTitle': 'Delete Snippet',
  'query.snippets.deleteConfirmMessage': 'Are you sure you want to delete snippet "{prefix}"?',
  'query.snippets.emptyCustom': 'No custom snippets yet. Click "Add Snippet" to create one.',
```

In `src/locales/zh-CN/query.ts`, add:

```ts
  'query.snippets.add': '新增片段',
  'query.snippets.edit': '编辑片段',
  'query.snippets.duplicate': '复制为自定义',
  'query.snippets.builtin': '内置',
  'query.snippets.custom': '自定义',
  'query.snippets.prefix': '快捷前缀',
  'query.snippets.prefixPlaceholder': '例如 selw',
  'query.snippets.prefixHint': '允许字母、数字、下划线及星号 (*)',
  'query.snippets.prefixInvalid': '前缀必须以字母或下划线开头，且仅包含字母、数字、_、*',
  'query.snippets.description': '描述说明',
  'query.snippets.descriptionPlaceholder': '例如 按用户分组统计',
  'query.snippets.template': 'SQL 模板',
  'query.snippets.templatePlaceholder': 'SELECT * FROM ${1:table_name} WHERE ${2:condition};${3}',
  'query.snippets.templateRequired': '模板内容不能为空',
  'query.snippets.syntaxGuide': '使用 ${1:占位符} 支持 Tab 键逐项跳转，末尾保留 ${N} 以便完成跳转。',
  'query.snippets.deleteConfirmTitle': '删除代码片段',
  'query.snippets.deleteConfirmMessage': '确定要删除代码片段 "{prefix}" 吗？',
  'query.snippets.emptyCustom': '暂无自定义代码片段，点击“新增片段”即可创建。',
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/locales/locales.test.ts`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/locales/en/query.ts src/locales/zh-CN/query.ts src/locales/locales.test.ts
git commit -m "feat(i18n): add translations for SQL snippets management"
```

---

### Task 2: Implement SnippetEditDialog Component

**Files:**
- Create: `src/windows/settings/SnippetEditDialog.tsx`
- Create: `src/windows/settings/__tests__/SnippetEditDialog.test.tsx`

**Interfaces:**
- Consumes:
  - `SqlSnippetItem` from `src/components/sql-editor/snippets/types`
  - `Dialog` from `src/components/ui/Dialog`
  - `Button` from `src/components/ui/Button`
  - `useI18n` from `src/hooks/useI18n`
- Produces:
  ```ts
  export interface SnippetEditDialogProps {
    open: boolean;
    snippet: SqlSnippetItem | null;
    isDuplicate?: boolean;
    onClose: () => void;
    onSave: (snippet: SqlSnippetItem) => void;
  }
  ```

- [ ] **Step 1: Write the failing test**

Create `src/windows/settings/__tests__/SnippetEditDialog.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { SnippetEditDialog } from '../SnippetEditDialog';

describe('SnippetEditDialog', () => {
  it('renders with initial values when editing existing snippet', () => {
    render(
      <SnippetEditDialog
        open={true}
        snippet={{
          id: 's-1',
          prefix: 'selw',
          descriptionKey: 'Select with WHERE',
          template: 'SELECT * FROM ${1:tbl};${2}',
        }}
        onClose={vi.fn()}
        onSave={vi.fn()}
      />
    );

    expect(screen.getByDisplayValue('selw')).toBeInTheDocument();
    expect(screen.getByDisplayValue('Select with WHERE')).toBeInTheDocument();
    expect(screen.getByDisplayValue('SELECT * FROM ${1:tbl};${2}')).toBeInTheDocument();
  });

  it('validates prefix format and prevents saving invalid prefix', () => {
    const onSave = vi.fn();
    render(
      <SnippetEditDialog
        open={true}
        snippet={null}
        onClose={vi.fn()}
        onSave={onSave}
      />
    );

    const prefixInput = screen.getByPlaceholderText('query.snippets.prefixPlaceholder');
    fireEvent.change(prefixInput, { target: { value: '123-invalid' } });

    const saveButton = screen.getByRole('button', { name: 'common.save' });
    fireEvent.click(saveButton);

    expect(onSave).not.toHaveBeenCalled();
    expect(screen.getByText('query.snippets.prefixInvalid')).toBeInTheDocument();
  });

  it('calls onSave with valid data and generated UUID for new snippets', () => {
    const onSave = vi.fn();
    render(
      <SnippetEditDialog
        open={true}
        snippet={null}
        onClose={vi.fn()}
        onSave={onSave}
      />
    );

    fireEvent.change(screen.getByPlaceholderText('query.snippets.prefixPlaceholder'), {
      target: { value: 'my_prefix' },
    });
    fireEvent.change(screen.getByPlaceholderText('query.snippets.descriptionPlaceholder'), {
      target: { value: 'My custom snippet' },
    });
    fireEvent.change(screen.getByPlaceholderText('SELECT * FROM ${1:table_name} WHERE ${2:condition};${3}'), {
      target: { value: 'SELECT 1;${1}' },
    });

    fireEvent.click(screen.getByRole('button', { name: 'common.save' }));

    expect(onSave).toHaveBeenCalledWith({
      id: expect.any(String),
      prefix: 'my_prefix',
      descriptionKey: 'My custom snippet',
      template: 'SELECT 1;${1}',
    });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/windows/settings/__tests__/SnippetEditDialog.test.tsx`
Expected: FAIL because `SnippetEditDialog.tsx` does not exist yet.

- [ ] **Step 3: Implement `SnippetEditDialog.tsx`**

Create `src/windows/settings/SnippetEditDialog.tsx`:

```tsx
import { useState, useEffect } from 'react';
import { Dialog } from '../../components/ui/Dialog';
import { Button } from '../../components/ui/Button';
import { useI18n } from '../../hooks/useI18n';
import type { SqlSnippetItem } from '../../components/sql-editor/snippets/types';

export interface SnippetEditDialogProps {
  open: boolean;
  snippet: SqlSnippetItem | null;
  isDuplicate?: boolean;
  onClose: () => void;
  onSave: (snippet: SqlSnippetItem) => void;
}

const PREFIX_REGEX = /^[A-Za-z_][A-Za-z0-9_*]*$/;

export function SnippetEditDialog({
  open,
  snippet,
  isDuplicate = false,
  onClose,
  onSave,
}: Readonly<SnippetEditDialogProps>) {
  const { t } = useI18n();

  const [prefix, setPrefix] = useState('');
  const [description, setDescription] = useState('');
  const [template, setTemplate] = useState('');
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (open) {
      if (snippet) {
        setPrefix(isDuplicate ? `${snippet.prefix}_copy` : snippet.prefix);
        const resolvedDesc = t(snippet.descriptionKey as any);
        setDescription(resolvedDesc === snippet.descriptionKey ? snippet.descriptionKey : resolvedDesc);
        setTemplate(snippet.template);
      } else {
        setPrefix('');
        setDescription('');
        setTemplate('');
      }
      setError(null);
    }
  }, [open, snippet, isDuplicate, t]);

  if (!open) return null;

  const handleSave = () => {
    const trimmedPrefix = prefix.trim();
    const trimmedTemplate = template.trim();

    if (!trimmedPrefix || !PREFIX_REGEX.test(trimmedPrefix)) {
      setError(t('query.snippets.prefixInvalid'));
      return;
    }

    if (!trimmedTemplate) {
      setError(t('query.snippets.templateRequired'));
      return;
    }

    onSave({
      id: snippet && !isDuplicate ? snippet.id : crypto.randomUUID(),
      prefix: trimmedPrefix,
      descriptionKey: description.trim() || trimmedPrefix,
      template: template,
    });
    onClose();
  };

  const title = isDuplicate
    ? t('query.snippets.duplicate')
    : snippet
      ? t('query.snippets.edit')
      : t('query.snippets.add');

  return (
    <Dialog
      open={open}
      title={title}
      onClose={onClose}
      className="max-w-xl"
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="ghost" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button variant="primary" onClick={handleSave}>
            {t('common.save')}
          </Button>
        </div>
      }
    >
      <div className="space-y-4 py-2">
        {error && (
          <div className="rounded-md bg-destructive/15 p-2 text-xs text-destructive">
            {error}
          </div>
        )}

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-fg-secondary">
            {t('query.snippets.prefix')} <span className="text-destructive">*</span>
          </label>
          <input
            type="text"
            value={prefix}
            onChange={(e) => {
              setPrefix(e.target.value);
              setError(null);
            }}
            placeholder={t('query.snippets.prefixPlaceholder')}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
          <p className="text-[11px] text-fg-muted">{t('query.snippets.prefixHint')}</p>
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-fg-secondary">
            {t('query.snippets.description')}
          </label>
          <input
            type="text"
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder={t('query.snippets.descriptionPlaceholder')}
            className="h-9 w-full rounded-md border border-edge bg-surface px-3 text-sm text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
        </div>

        <div className="space-y-1.5">
          <label className="text-xs font-medium text-fg-secondary">
            {t('query.snippets.template')} <span className="text-destructive">*</span>
          </label>
          <textarea
            value={template}
            onChange={(e) => {
              setTemplate(e.target.value);
              setError(null);
            }}
            placeholder={t('query.snippets.templatePlaceholder')}
            rows={6}
            className="w-full resize-y rounded-md border border-edge bg-surface p-2.5 font-mono text-xs text-fg outline-none focus:border-accent focus:ring-2 focus:ring-accent/25"
          />
          <div className="rounded border border-edge bg-surface-alt/70 p-2 text-[11px] text-fg-muted">
            <span className="font-semibold text-fg-secondary">Syntax Guide: </span>
            {t('query.snippets.syntaxGuide')}
          </div>
        </div>
      </div>
    </Dialog>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/windows/settings/__tests__/SnippetEditDialog.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/windows/settings/SnippetEditDialog.tsx src/windows/settings/__tests__/SnippetEditDialog.test.tsx
git commit -m "feat(settings): create SnippetEditDialog component with validation and tests"
```

---

### Task 3: Implement SqlSnippetsCard Component

**Files:**
- Create: `src/windows/settings/SqlSnippetsCard.tsx`
- Create: `src/windows/settings/__tests__/SqlSnippetsCard.test.tsx`

**Interfaces:**
- Consumes:
  - `AppSettings` from `src/types`
  - `BUILTIN_SQL_SNIPPETS`, `SqlSnippetItem` from `src/components/sql-editor/snippets`
  - `exportEditorSettings`, `importEditorSettings` from `src/lib/settingsExport`
  - `useConfirmDialog` from `src/hooks/useConfirmDialog`
  - `SnippetEditDialog` from `./SnippetEditDialog`
- Produces:
  ```ts
  export interface SqlSnippetsCardProps {
    settings: AppSettings;
    onUpdateSnippets: (snippets: SqlSnippetItem[]) => Promise<void> | void;
    onImportSuccess?: () => void;
  }
  ```

- [ ] **Step 1: Write the failing test**

Create `src/windows/settings/__tests__/SqlSnippetsCard.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { SqlSnippetsCard } from '../SqlSnippetsCard';
import type { AppSettings } from '../../../types';

const mockSettings: AppSettings = {
  theme: { mode: 'dark', packId: null },
  language: 'en',
  limitSelectResults: true,
  queryResultLimit: 5000,
  editorFontSize: 13,
  editorFontFamily: 'Menlo',
  confirmOnDelete: true,
  autoCommit: true,
  safeMode: true,
  defaultPageSize: 50,
  connectionPoolSize: 10,
  checkForUpdatesOnStartup: false,
  logLevel: 'info',
  logPath: '',
  mcpServerEnabled: false,
  mcpDisabledTools: [],
  mcpPermissionMode: 'read_only',
  contextDir: '/tmp',
  driverSettings: {},
  mcpClientServers: [],
  aiStrictEgress: true,
  monitor: { enabled: false, pollIntervalSecs: 60, retentionDays: 7, trayEnabled: false, alertsEnabled: false },
  sqlSnippets: [
    {
      id: 'custom-1',
      prefix: 'selcustom',
      descriptionKey: 'Custom query',
      template: 'SELECT custom FROM tbl;${1}',
    },
  ],
};

describe('SqlSnippetsCard', () => {
  it('renders built-in snippets with builtin badge and custom snippets with custom badge', () => {
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={vi.fn()} />);

    // Builtin prefix
    expect(screen.getByText('sel*')).toBeInTheDocument();
    // Custom prefix
    expect(screen.getByText('selcustom')).toBeInTheDocument();
    expect(screen.getByText('Custom query')).toBeInTheDocument();
  });

  it('allows clicking Add Snippet to open modal', () => {
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={vi.fn()} />);

    fireEvent.click(screen.getByText('query.snippets.add'));
    expect(screen.getByText('query.snippets.add')).toBeInTheDocument();
  });

  it('calls onUpdateSnippets when deleting a custom snippet', async () => {
    const onUpdateSnippets = vi.fn();
    render(<SqlSnippetsCard settings={mockSettings} onUpdateSnippets={onUpdateSnippets} />);

    const deleteBtn = screen.getByTestId('delete-snippet-custom-1');
    fireEvent.click(deleteBtn);

    // Confirm dialog appears
    const confirmBtn = await screen.findByRole('button', { name: 'common.confirm' });
    fireEvent.click(confirmBtn);

    await waitFor(() => {
      expect(onUpdateSnippets).toHaveBeenCalledWith([]);
    });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/windows/settings/__tests__/SqlSnippetsCard.test.tsx`
Expected: FAIL because `SqlSnippetsCard.tsx` does not exist yet.

- [ ] **Step 3: Implement `SqlSnippetsCard.tsx`**

Create `src/windows/settings/SqlSnippetsCard.tsx`:

```tsx
import { useState, useCallback, useMemo } from 'react';
import { Plus, Copy, Edit2, Trash2, Code2, Download, Upload } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Badge } from '../../components/ui/Badge';
import { useI18n } from '../../hooks/useI18n';
import { useConfirmDialog } from '../../hooks/useConfirmDialog';
import { BUILTIN_SQL_SNIPPETS } from '../../components/sql-editor/snippets/builtinSnippets';
import type { SqlSnippetItem } from '../../components/sql-editor/snippets/types';
import type { AppSettings } from '../../types';
import { exportEditorSettings, importEditorSettings } from '../../lib/settingsExport';
import { SnippetEditDialog } from './SnippetEditDialog';
import { useSettingsStore } from '../../stores/settingsStore';

export interface SqlSnippetsCardProps {
  settings: AppSettings;
  onUpdateSnippets: (snippets: SqlSnippetItem[]) => Promise<void> | void;
}

export function SqlSnippetsCard({ settings, onUpdateSnippets }: Readonly<SqlSnippetsCardProps>) {
  const { t } = useI18n();
  const [confirmDelete, confirmDeleteDialog] = useConfirmDialog();
  const updateSettings = useSettingsStore((s) => s.updateSettings);

  const [dialogState, setDialogState] = useState<{
    open: boolean;
    snippet: SqlSnippetItem | null;
    isDuplicate: boolean;
  }>({
    open: false,
    snippet: null,
    isDuplicate: false,
  });

  const [statusMessage, setStatusMessage] = useState<string | null>(null);

  const showStatus = useCallback((msg: string) => {
    setStatusMessage(msg);
    setTimeout(() => setStatusMessage(null), 3000);
  }, []);

  const customSnippets = useMemo(() => settings.sqlSnippets ?? [], [settings.sqlSnippets]);

  const handleExport = useCallback(async () => {
    try {
      const json = exportEditorSettings(settings);
      await navigator.clipboard.writeText(json);
      showStatus(t('query.exportSettingsSuccess'));
    } catch (e) {
      showStatus(String(e));
    }
  }, [settings, showStatus, t]);

  const handleImport = useCallback(async () => {
    try {
      const text = await navigator.clipboard.readText();
      const imported = importEditorSettings(text);
      await updateSettings(imported);
      showStatus(t('query.importSettingsSuccess'));
    } catch {
      showStatus(t('query.importSettingsInvalid'));
    }
  }, [updateSettings, showStatus, t]);

  const handleSaveSnippet = useCallback(
    async (snippet: SqlSnippetItem) => {
      const existingIdx = customSnippets.findIndex((s) => s.id === snippet.id);
      let updated: SqlSnippetItem[];
      if (existingIdx >= 0) {
        updated = [...customSnippets];
        updated[existingIdx] = snippet;
      } else {
        updated = [...customSnippets, snippet];
      }
      await onUpdateSnippets(updated);
    },
    [customSnippets, onUpdateSnippets],
  );

  const handleDeleteSnippet = useCallback(
    async (snippet: SqlSnippetItem) => {
      const ok = await confirmDelete({
        title: t('query.snippets.deleteConfirmTitle'),
        message: t('query.snippets.deleteConfirmMessage', { prefix: snippet.prefix }),
        kind: 'warning',
      });
      if (!ok) return;

      const updated = customSnippets.filter((s) => s.id !== snippet.id);
      await onUpdateSnippets(updated);
    },
    [confirmDelete, customSnippets, onUpdateSnippets, t],
  );

  const resolveSnippetDesc = (descriptionKey: string) => {
    const translated = t(descriptionKey as any);
    return translated === descriptionKey ? descriptionKey : translated;
  };

  return (
    <div className="space-y-4 rounded-lg border border-edge bg-surface-alt p-3.5">
      {confirmDeleteDialog}

      {/* Header */}
      <div className="flex flex-col gap-2 sm:flex-row sm:items-center sm:justify-between">
        <div className="flex items-center gap-2">
          <Code2 className="h-4 w-4 text-accent" />
          <span className="text-xs font-semibold uppercase tracking-wider text-fg">
            {t('query.snippets')}
          </span>
        </div>

        <div className="flex items-center gap-1.5 flex-wrap">
          <Button
            variant="primary"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={() => setDialogState({ open: true, snippet: null, isDuplicate: false })}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('query.snippets.add')}
          </Button>

          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={handleExport}
            title={t('query.exportSettings')}
          >
            <Download className="h-3 w-3" />
            {t('query.exportSettings')}
          </Button>

          <Button
            variant="secondary"
            size="sm"
            className="h-7 gap-1 text-xs"
            onClick={handleImport}
            title={t('query.importSettings')}
          >
            <Upload className="h-3 w-3" />
            {t('query.importSettings')}
          </Button>
        </div>
      </div>

      {statusMessage && <p className="text-xs text-accent">{statusMessage}</p>}

      {/* Custom Snippets */}
      <div className="space-y-2">
        <div className="flex items-center justify-between text-[11px] font-medium text-fg-muted uppercase tracking-wider">
          <span>{t('query.snippets.custom')} ({customSnippets.length})</span>
        </div>

        {customSnippets.length === 0 ? (
          <div className="rounded border border-dashed border-edge/70 p-4 text-center text-xs text-fg-muted">
            {t('query.snippets.emptyCustom')}
          </div>
        ) : (
          <div className="divide-y divide-edge rounded border border-edge bg-surface">
            {customSnippets.map((item) => (
              <div
                key={item.id}
                className="flex items-center justify-between gap-3 p-2.5 hover:bg-surface-raised transition-colors"
              >
                <div className="flex items-center gap-2.5 min-w-0 flex-1">
                  <kbd className="min-w-14 rounded border border-edge bg-surface-raised px-1.5 py-0.5 text-center font-mono text-[11px] font-semibold text-accent">
                    {item.prefix}
                  </kbd>
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-1.5">
                      <span className="truncate text-xs font-medium text-fg">
                        {resolveSnippetDesc(item.descriptionKey)}
                      </span>
                      <Badge variant="secondary" className="px-1.5 py-0 text-[10px]">
                        {t('query.snippets.custom')}
                      </Badge>
                    </div>
                    <p className="truncate font-mono text-[11px] text-fg-muted" title={item.template}>
                      {item.template.replace(/\n/g, ' ')}
                    </p>
                  </div>
                </div>

                <div className="flex items-center gap-1 shrink-0">
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 p-0 text-fg-secondary hover:text-fg"
                    onClick={() => setDialogState({ open: true, snippet: item, isDuplicate: false })}
                    title={t('query.snippets.edit')}
                  >
                    <Edit2 className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-7 w-7 p-0 text-destructive/80 hover:text-destructive"
                    data-testid={`delete-snippet-${item.id}`}
                    onClick={() => handleDeleteSnippet(item)}
                    title={t('common.delete')}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </Button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Builtin Snippets */}
      <div className="space-y-2 pt-2">
        <div className="flex items-center justify-between text-[11px] font-medium text-fg-muted uppercase tracking-wider">
          <span>{t('query.snippets.builtin')} ({BUILTIN_SQL_SNIPPETS.length})</span>
        </div>

        <div className="divide-y divide-edge rounded border border-edge bg-surface">
          {BUILTIN_SQL_SNIPPETS.map((item) => (
            <div
              key={item.id}
              className="flex items-center justify-between gap-3 p-2.5 hover:bg-surface-raised transition-colors"
            >
              <div className="flex items-center gap-2.5 min-w-0 flex-1">
                <kbd className="min-w-14 rounded border border-edge bg-surface-raised px-1.5 py-0.5 text-center font-mono text-[11px] font-semibold text-fg">
                  {item.prefix}
                </kbd>
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-1.5">
                    <span className="truncate text-xs font-medium text-fg">
                      {resolveSnippetDesc(item.descriptionKey)}
                    </span>
                    <Badge variant="outline" className="px-1.5 py-0 text-[10px] text-fg-muted">
                      {t('query.snippets.builtin')}
                    </Badge>
                  </div>
                  <p className="truncate font-mono text-[11px] text-fg-muted" title={item.template}>
                    {item.template.replace(/\n/g, ' ')}
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-1 shrink-0">
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-7 px-2 text-xs gap-1 text-fg-secondary hover:text-fg"
                  onClick={() => setDialogState({ open: true, snippet: item, isDuplicate: true })}
                  title={t('query.snippets.duplicate')}
                >
                  <Copy className="h-3 w-3" />
                  <span className="hidden sm:inline">{t('query.snippets.duplicate')}</span>
                </Button>
              </div>
            </div>
          ))}
        </div>
      </div>

      <SnippetEditDialog
        open={dialogState.open}
        snippet={dialogState.snippet}
        isDuplicate={dialogState.isDuplicate}
        onClose={() => setDialogState({ open: false, snippet: null, isDuplicate: false })}
        onSave={handleSaveSnippet}
      />
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/windows/settings/__tests__/SqlSnippetsCard.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/windows/settings/SqlSnippetsCard.tsx src/windows/settings/__tests__/SqlSnippetsCard.test.tsx
git commit -m "feat(settings): create SqlSnippetsCard component with tests"
```

---

### Task 4: Integrate SqlSnippetsCard into SettingsContent

**Files:**
- Modify: `src/windows/settings/SettingsContent.tsx`
- Test: `src/windows/settings/__tests__/SettingsContent.test.tsx`

**Interfaces:**
- Consumes: `SqlSnippetsCard` from `./SqlSnippetsCard`
- Produces: `SettingsContent` rendering `SqlSnippetsCard` under `activeSection === 'editor'`.

- [ ] **Step 1: Write the failing test**

In `src/windows/settings/__tests__/SettingsContent.test.tsx`, add a test checking that `SqlSnippetsCard` is rendered in editor section:

```tsx
it('renders SqlSnippetsCard in editor section', async () => {
  render(<SettingsContent initialSection="editor" />);
  expect(screen.getByText('query.snippets')).toBeInTheDocument();
  expect(screen.getByText('query.snippets.add')).toBeInTheDocument();
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `npx vitest run src/windows/settings/__tests__/SettingsContent.test.tsx`
Expected: FAIL because `SqlSnippetsCard` is not yet mounted in `SettingsContent`.

- [ ] **Step 3: Mount `SqlSnippetsCard` in `SettingsContent.tsx`**

In `src/windows/settings/SettingsContent.tsx`:
Import `SqlSnippetsCard`:
```tsx
import { SqlSnippetsCard } from './SqlSnippetsCard';
```

Replace the standalone import/export block (lines ~538-555) under `activeSection === 'editor'` with:
```tsx
<SqlSnippetsCard
  settings={settings}
  onUpdateSnippets={(snippets) => updateField('sqlSnippets', snippets)}
/>
```

(The handlers `handleExportSettings` and `handleImportSettings` in `SettingsContent.tsx` can be cleanly removed as they are now fully encapsulated inside `SqlSnippetsCard`.)

- [ ] **Step 4: Run test to verify it passes**

Run: `npx vitest run src/windows/settings/__tests__/SettingsContent.test.tsx`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/windows/settings/SettingsContent.tsx src/windows/settings/__tests__/SettingsContent.test.tsx
git commit -m "feat(settings): integrate SqlSnippetsCard into SettingsContent editor tab"
```

---

### Task 5: Connect User Snippets into CodeMirror Autocomplete in SqlEditor

**Files:**
- Modify: `src/components/sql-editor/editorExtensions.ts`
- Modify: `src/components/sql-editor/SqlEditor.tsx`
- Test: `src/components/sql-editor/snippets/__tests__/snippetJourney.test.ts`

**Interfaces:**
- Consumes:
  - `AppSettings.sqlSnippets` from `useSettingsStore`
  - `BUILTIN_SQL_SNIPPETS` from `src/components/sql-editor/snippets`
- Produces:
  - `CompletionCompartmentOptions.snippets?: readonly SqlSnippetItem[]`
  - `SqlEditor` passes all active snippets into `createCompletionExtensions`

- [ ] **Step 1: Write the failing test**

In `src/components/sql-editor/snippets/__tests__/snippetJourney.test.ts`, add a test testing user snippet completion coexistence:

```ts
it('includes user custom snippets when provided in options', () => {
  const customSnippet: SqlSnippetItem = {
    id: 'custom-foo',
    prefix: 'custom_foo',
    descriptionKey: 'Custom foo template',
    template: 'SELECT foo FROM ${1:tbl};${2}',
  };

  const completions = buildSnippetCompletions({
    snippets: [...BUILTIN_SQL_SNIPPETS, customSnippet],
  });

  expect(completions.some((c) => c.item.prefix === 'custom_foo')).toBe(true);
});
```

- [ ] **Step 2: Run test to verify it fails or passes**

Run: `npx vitest run src/components/sql-editor/snippets/__tests__/snippetJourney.test.ts`
Expected: passes for `buildSnippetCompletions`, but let's test `createCompletionExtensions`:

In `src/components/sql-editor/__tests__/editorHotplug.test.ts`, verify that `createCompletionExtensions` accepts `snippets`:
```ts
it('accepts snippets in completion options without throwing', () => {
  const exts = createCompletionExtensions(
    { snippets: BUILTIN_SQL_SNIPPETS },
    { modelRef: { current: null } }
  );
  expect(exts).toBeDefined();
});
```

- [ ] **Step 3: Update `editorExtensions.ts` and `SqlEditor.tsx`**

In `src/components/sql-editor/editorExtensions.ts`:
```ts
export interface CompletionCompartmentOptions {
  databaseType?: string;
  metadataSnapshot?: EditorMetadataSnapshot;
  schema?: SQLNamespace;
  completionQuotePolicy?: CompletionQuotePolicy;
  /** Resolves snippet description i18n keys; omitted in tests and non-UI callers. */
  translate?: (key: string) => string;
  /** Active snippet library (builtin + user snippets) */
  snippets?: readonly SqlSnippetItem[];
}
```
And inside `createCompletionExtensions`:
```ts
// §4.1: snippet templates with tabstop expansion
createSnippetCompletionSource({ t: opts.translate, snippets: opts.snippets }),
```

In `src/components/sql-editor/SqlEditor.tsx`:
Read user snippets from settings store:
```ts
const userSnippets = useSettingsStore((s) => s.settings.sqlSnippets);
const allSnippets = useMemo(
  () => [...BUILTIN_SQL_SNIPPETS, ...(userSnippets ?? [])],
  [userSnippets],
);
```
Pass `snippets: allSnippets` into `createCompletionExtensions`:
```ts
const completionExts = useMemo(
  () =>
    createCompletionExtensions(
      {
        databaseType,
        metadataSnapshot,
        schema,
        completionQuotePolicy,
        translate,
        snippets: allSnippets,
      },
      { modelRef, metadataSnapshotRef },
    ),
  [
    databaseType,
    metadataSnapshot,
    schema,
    completionQuotePolicy,
    translate,
    allSnippets,
    isSqlEditorProEnhanced,
  ],
);
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `npx vitest run src/components/sql-editor/`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/sql-editor/editorExtensions.ts src/components/sql-editor/SqlEditor.tsx src/components/sql-editor/snippets/__tests__/snippetJourney.test.ts
git commit -m "feat(sql-editor): pipe user snippets to CodeMirror completion extensions"
```

---

### Task 6: End-to-End Regression & Verification Check

**Files:**
- Test files across affected modules:
  - `src/windows/settings/`
  - `src/components/sql-editor/`
  - `src/lib/__tests__/settingsExport.test.ts`

- [ ] **Step 1: Run comprehensive unit test suites**

Run:
```bash
npx vitest run src/windows/settings/ src/components/sql-editor/ src/lib/__tests__/settingsExport.test.ts
```
Expected: All tests pass without warnings or failures.

- [ ] **Step 2: Run typecheck / build check**

Run:
```bash
pnpm build
```
Expected: TypeScript compile and Vite build succeed cleanly without any type errors.

- [ ] **Step 3: Commit and summarize**

```bash
git commit --allow-empty -m "chore: verify SQL snippets settings and completion integration"
```
