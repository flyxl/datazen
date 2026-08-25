export const config: WebdriverIO.Config = {
  runner: 'local',
  specs: ['./specs/**/*.ts'],
  /**
   * Named groups run via `pnpm e2e:<group>` (package.json) → `--suite <group>`.
   * Single source of truth for group membership; paths are relative to this
   * config file (same resolution as `specs`). Keep in sync with docs:
   * docs/development/e2e-testing.md §2.
   */
  suite: {
    // Core UI, no real DB required (was `pnpm e2e:core`)
    core: [
      './specs/main-window.ts',
      './specs/new-connection.ts',
      './specs/edit-delete-connection.ts',
      './specs/connection-search-group.ts',
      './specs/settings.ts',
      './specs/i18n-menu.ts',
      './specs/homepage-features.ts',
      './specs/drag-drop-groups.ts',
      './specs/backup-database.ts',
      './specs/backup-window.ts',
      './specs/schema-diff-window.ts',
      './specs/data-sync-window.ts',
    ],
    // Real-DB Host specs incl. the host contract matrix (was `pnpm e2e:db`)
    db: [
      './specs/connection-window.ts',
      './specs/sql-query.ts',
      './specs/table-data.ts',
      './specs/table-filter.ts',
      './specs/table-indexes.ts',
      './specs/table-edit.ts',
      './specs/table-structure.ts',
      './specs/export-import.ts',
      './specs/object-browser.ts',
      './specs/data-types.ts',
      './specs/mysql.ts',
      './specs/mysql-multi-database.ts',
      './specs/postgres-multi-database.ts',
      './specs/data-sync-real.ts',
      './specs/client-parity.ts',
      './specs/host-contract-matrix.ts',
    ],
    // Host contract matrix × PG/MySQL/SQLite (`pnpm e2e:contract:matrix`,
    // `pnpm e2e:contract:pg` adds --mochaOpts.grep 'Host contract @ postgres')
    contract: ['./specs/host-contract-matrix.ts'],
    // Redis driver's own E2E, not part of default full run (`pnpm e2e:redis`)
    redis: ['../packages/drivers/redis/e2e/*.ts'],
    // AI features (`pnpm e2e:ai`)
    ai: [
      './specs/ai-features.ts',
      './specs/ai-context.ts',
      './specs/ai-context-tables.ts',
      './specs/ai-code-block.ts',
    ],
    // App-data backup + i18n locales (`pnpm e2e:i18n-backup`)
    'i18n-backup': [
      './specs/app-data-backup.ts',
      './specs/i18n-10-locales.ts',
      './specs/system-locale.ts',
      './specs/i18n-menu.ts',
    ],
    // Path IPC hardening + workflow / driver commands (`pnpm e2e:path-ipc`)
    'path-ipc': [
      './specs/path-ipc-hardening.ts',
      './specs/workflow-window.ts',
      './specs/driver-commands.ts',
      './specs/app-data-backup.ts',
    ],
    // Dashboard (`pnpm e2e:dashboard`)
    dashboard: ['./specs/data-dashboard*.ts'],
  },
  maxInstances: 1,
  capabilities: [{}],
  hostname: '127.0.0.1',
  port: 4445,
  path: '/',
  logLevel: 'warn',
  waitforTimeout: 10000,
  connectionRetryTimeout: 30000,
  connectionRetryCount: 3,
  framework: 'mocha',
  reporters: ['spec'],
  mochaOpts: {
    ui: 'bdd',
    timeout: 60000,
  },
  before: async function () {
    await browser.url('tauri://localhost');
    await browser.pause(2000);

    // Force language to zh-CN so all Chinese selectors work
    await browser.executeAsync((done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke('save_settings', {
          settings: {
            theme: 'dark',
            language: 'zh-CN',
            limitSelectResults: true,
            queryResultLimit: 1000,
            editorFontSize: 14,
            editorFontFamily: 'monospace',
            confirmOnDelete: true,
            autoCommit: true, // per-statement isolation; false leaves PG aborted after first error
            safeMode: true,
            defaultPageSize: 50,
          },
        })
        .then(() => done(null))
        .catch((e: unknown) => done(String(e)));
    });

    // Ensure a PostgreSQL connection with env credentials exists (upsert)
    {
      const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
      const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
      const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
      const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
      const pgDatabase = process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';
      await browser.executeAsync(
        (
          host: string,
          port: number,
          user: string,
          pw: string,
          db: string,
          done: (r: unknown) => void,
        ) => {
          const config = {
            id: 'conn_e2e_pg',
            name: '本地 PostgreSQL',
            databaseType: 'postgresql',
            host,
            port,
            username: user,
            password: pw,
            database: db,
            group: 'E2E 测试',
            colorTag: 'blue',
            sslMode: 'disable',
          };
          (window as any).__TAURI_INTERNALS__
            .invoke('save_connection', { config })
            .then(() => done(null))
            .catch((e: unknown) => done(String(e)));
        },
        pgHost,
        pgPort,
        pgUser,
        pgPassword,
        pgDatabase,
      );
    }

    // Reload page so the new language and seeded connections take effect
    await browser.execute(() => location.reload());
    await browser.pause(2000);

    // Expand all connection groups so items are visible
    await browser.execute(() => {
      document.querySelectorAll('[data-group-header]').forEach((el) => {
        const parent = el.closest('[data-group-name]');
        if (parent && !parent.querySelector('[data-conn-item]')) {
          (el as HTMLElement).click();
        }
      });
    });
    await browser.pause(500);
  },
  beforeSuite: async function () {
    // Same Tauri process is reused across spec files; close leftover sub-windows
    // so Host specs do not attach to a previous MultiDb / SQLite session.
    try {
      await browser.url('tauri://localhost');
      await browser.pause(400);
      const handles = await browser.getWindowHandles();
      const main = handles[0];
      for (const h of handles) {
        if (h === main) continue;
        try {
          await browser.switchToWindow(h);
          await browser.closeWindow();
        } catch {
          /* ignore */
        }
      }
      if (main) await browser.switchToWindow(main);
    } catch {
      /* ignore */
    }
  },
};
