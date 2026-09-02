import fs from 'node:fs';
import path from 'node:path';
import { expect, browser, $ } from '@wdio/globals';
import {
  waitForNewConnectionDialog,
  closeExtraWindows,
  openNewConnectionDialogFromUi,
  selectNewConnectionDriver,
} from '../helpers.js';

const ROOT = path.resolve(import.meta.dirname, '../..');
const FILE_CONNECTION_FIELDS = path.join(
  ROOT,
  'src/components/connection/FileConnectionFields.tsx',
);
const THEMES_CSS = path.join(ROOT, 'src/styles/themes.css');
const TAILWIND_CONFIG = path.join(ROOT, 'tailwind.config.ts');
const CONNECTION_DIR = path.join(ROOT, 'src/components/connection');

const HARDCODED_NEUTRAL_PATTERN = /\bneutral-(300|400|500|600|700|800)\b/;
const REQUIRED_SEMANTIC_CLASSES = [
  'text-fg-muted',
  'text-fg-secondary',
  'border-edge',
  'bg-surface-raised',
  'bg-surface-alt',
];
const REQUIRED_CSS_VARS = [
  '--c-surface-alt',
  '--c-surface-raised',
  '--c-edge',
  '--c-fg-secondary',
  '--c-fg-muted',
];
const REQUIRED_TAILWIND_KEYS = ['surface:', 'edge:', 'fg:'];

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error((result as { __error: string }).__error);
  }
  return result as T;
}

async function setTheme(theme: 'light' | 'dark') {
  await invokeBackend('save_settings', {
    settings: {
      theme,
      language: 'zh-CN',
      limitSelectResults: true,
      queryResultLimit: 5000,
      editorFontSize: 13,
      editorFontFamily: 'Menlo',
      confirmOnDelete: true,
      autoCommit: true,
      safeMode: true,
      defaultPageSize: 50,
    },
  });
  await browser.refresh();
  await browser.pause(300);
}

async function openSqliteConnectionForm() {
  await openNewConnectionDialogFromUi();

  await selectNewConnectionDriver('sqlite');
  await browser.pause(300);

  const fileInput = await $('input[placeholder="/path/to/db.sqlite"]');
  await fileInput.waitForDisplayed({ timeout: 5000 });
}

async function toggleAdbMode() {
  const clicked = await browser.execute(() => {
    const labels = Array.from(document.querySelectorAll('label'));
    const adbLabel = labels.find((l) => l.textContent?.includes('从 Android 设备拉取'));
    if (!adbLabel) return false;
    const checkbox = adbLabel.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
    if (!checkbox) return false;
    checkbox.click();
    return true;
  });
  expect(clicked).toBe(true);
  await browser.pause(500);
}

describe('FileConnectionFields 浅色主题适配 (FCF-001~FCF-010)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  // ── Static / source checks ──────────────────────────────────────

  it('FCF-001: FileConnectionFields 不应包含硬编码 neutral 深色类', () => {
    const source = fs.readFileSync(FILE_CONNECTION_FIELDS, 'utf8');
    expect(HARDCODED_NEUTRAL_PATTERN.test(source)).toBe(false);
  });

  it('FCF-002: FileConnectionFields 应使用语义化 token 类', () => {
    const source = fs.readFileSync(FILE_CONNECTION_FIELDS, 'utf8');
    for (const cls of REQUIRED_SEMANTIC_CLASSES) {
      expect(source).toContain(cls);
    }
  });

  it('FCF-003: themes.css 应定义所需 CSS 变量（含 light/dark）', () => {
    const css = fs.readFileSync(THEMES_CSS, 'utf8');
    for (const varName of REQUIRED_CSS_VARS) {
      expect(css).toContain(varName);
    }
    expect(css).toContain(':root');
    expect(css).toContain('.dark');
  });

  it('FCF-004: tailwind.config.ts 应映射语义化颜色', () => {
    const config = fs.readFileSync(TAILWIND_CONFIG, 'utf8');
    for (const key of REQUIRED_TAILWIND_KEYS) {
      expect(config).toContain(key);
    }
  });

  it('FCF-005: 其他 connection 组件不应包含硬编码 neutral 深色类', () => {
    const files = fs
      .readdirSync(CONNECTION_DIR)
      .filter((f) => f.endsWith('.tsx'))
      .map((f) => path.join(CONNECTION_DIR, f));

    const offenders: string[] = [];
    for (const file of files) {
      const source = fs.readFileSync(file, 'utf8');
      if (HARDCODED_NEUTRAL_PATTERN.test(source)) {
        offenders.push(path.basename(file));
      }
    }
    expect(offenders).toEqual([]);
  });

  // ── Runtime / theme checks ──────────────────────────────────────

  it('FCF-006: 浅色主题下 SQLite 连接表单应显示文件路径与 ADB 开关', async () => {
    await setTheme('light');
    await openSqliteConnectionForm();

    const html = await $('html');
    const cls = await html.getAttribute('class');
    expect(cls ?? '').not.toContain('dark');

    await expect(await $('input[placeholder="/path/to/db.sqlite"]')).toBeDisplayed();
    await expect(await $('label*=从 Android 设备拉取')).toBeDisplayed();
  });

  it('FCF-007: 浅色主题下 ADB 开关 label 应使用 text-fg-muted 语义类', async () => {
    await setTheme('light');
    await openSqliteConnectionForm();

    const hasSemanticLabel = await browser.execute(() => {
      const labels = Array.from(document.querySelectorAll('label'));
      const adbLabel = labels.find((l) => l.textContent?.includes('从 Android 设备拉取'));
      if (!adbLabel) return false;
      return adbLabel.className.includes('text-fg-muted');
    });
    expect(hasSemanticLabel).toBe(true);
  });

  it('FCF-008: 展开 ADB 面板后容器应使用 border-edge 与 bg-surface-alt', async () => {
    await setTheme('light');
    await openSqliteConnectionForm();
    await toggleAdbMode();

    const panelOk = await browser.execute(() => {
      const labels = Array.from(document.querySelectorAll('label'));
      const adbLabel = labels.find((l) => l.textContent?.includes('从 Android 设备拉取'));
      if (!adbLabel) return false;
      let sibling = adbLabel.nextElementSibling;
      while (sibling) {
        const cl = sibling.className || '';
        if (cl.includes('border-edge') && cl.includes('bg-surface-alt')) {
          return true;
        }
        sibling = sibling.nextElementSibling;
      }
      return false;
    });
    expect(panelOk).toBe(true);
  });

  it('FCF-009: 浅色主题下 ADB 面板应绑定 light theme CSS 变量', async () => {
    await setTheme('light');
    await openSqliteConnectionForm();
    await toggleAdbMode();

    const result = await browser.execute(() => {
      const surfaceAlt = getComputedStyle(document.documentElement)
        .getPropertyValue('--c-surface-alt')
        .trim();
      const edge = getComputedStyle(document.documentElement).getPropertyValue('--c-edge').trim();

      const labels = Array.from(document.querySelectorAll('label'));
      const adbLabel = labels.find((l) => l.textContent?.includes('从 Android 设备拉取'));
      if (!adbLabel) return null;

      let sibling = adbLabel.nextElementSibling;
      while (sibling) {
        const cl = sibling.className || '';
        if (cl.includes('border-edge') && cl.includes('bg-surface-alt')) {
          const style = getComputedStyle(sibling as Element);
          return {
            surfaceAlt,
            edge,
            bgColor: style.backgroundColor,
            borderColor: style.borderTopColor,
            hasSemanticClasses: true,
          };
        }
        sibling = sibling.nextElementSibling;
      }
      return null;
    });

    expect(result).not.toBeNull();
    expect(result!.surfaceAlt).toBe('#f8fafc');
    expect(result!.edge).toBe('#e2e8f0');
    expect(result!.hasSemanticClasses).toBe(true);
    expect(result!.bgColor).not.toBe('rgba(0, 0, 0, 0)');
    expect(result!.borderColor).not.toBe('rgba(0, 0, 0, 0)');
  });

  it('FCF-010: 深色主题下相同元素仍使用语义类且 CSS 变量切换为 dark token', async () => {
    await setTheme('dark');
    await openSqliteConnectionForm();

    const html = await $('html');
    expect(await html.getAttribute('class')).toContain('dark');

    await toggleAdbMode();

    const result = await browser.execute(() => {
      const surfaceAlt = getComputedStyle(document.documentElement)
        .getPropertyValue('--c-surface-alt')
        .trim();
      const edge = getComputedStyle(document.documentElement).getPropertyValue('--c-edge').trim();

      const labels = Array.from(document.querySelectorAll('label'));
      const adbLabel = labels.find((l) => l.textContent?.includes('从 Android 设备拉取'));
      if (!adbLabel || !adbLabel.className.includes('text-fg-muted')) {
        return { ok: false, reason: 'missing semantic label class' };
      }

      let sibling = adbLabel.nextElementSibling;
      while (sibling) {
        const cl = sibling.className || '';
        if (cl.includes('border-edge') && cl.includes('bg-surface-alt')) {
          const style = getComputedStyle(sibling as Element);
          return {
            ok: true,
            surfaceAlt,
            edge,
            bgColor: style.backgroundColor,
            borderColor: style.borderTopColor,
          };
        }
        sibling = sibling.nextElementSibling;
      }
      return { ok: false, reason: 'missing semantic panel class' };
    });

    expect(result.ok).toBe(true);
    expect(result.surfaceAlt).toBe('#1e293b');
    expect(result.edge).toBe('#334155');
    expect(result.bgColor).not.toBe('rgba(0, 0, 0, 0)');
    expect(result.borderColor).not.toBe('rgba(0, 0, 0, 0)');
  });
});
