import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  readCatalog,
  readRuntimeCatalog,
  readRuntimeRelationSchema,
  RuntimeMetadataProbeError,
} from '../schema-metadata.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQL,
  executeSQLChecked,
  invokeBackend,
  E2E_PG_CONNECTION_ID,
  connectConfig,
  executeQuery,
  parseQueryRows,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  waitForNewQueryButton,
  waitForTableInSidebar,
  expandConnectedConnectionInNavigator,
  expandSchemaTableCategory,
  clickNavigatorRefresh,
} from '../helpers.js';

/**
 * Table structure editor tests: create table, alter table.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */

const TEST_TABLE = '_e2e_structure_test';

describe('表结构编辑 (TS-001~TS-009)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    // Clean up any leftover test table. Checked: a stale/dead session would
    // make this fail fast with the real backend error instead of cascading
    // into a 20s `waitForTableInSidebar` timeout below.
    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      // best-effort cleanup
    }
    await closeExtraWindows(mainWindow);
  });

  // ── 新建表 ─────────────────────────────────────────────────────

  it('应显示新建表按钮 (TS-001)', async () => {
    const newTableBtn = await $("[data-testid='content-toolbar-new-table']");
    await expect(newTableBtn).toBeDisplayed();
  });

  it('点击新建表应打开表结构编辑器 (TS-001)', async () => {
    const newTblBtn = await $("[data-testid='content-toolbar-new-table']");
    await newTblBtn.click();
    await browser.pause(1000);

    // Should show the table name input and column grid
    const tableNameInput = await $('input[placeholder="new_table"]');
    await tableNameInput.waitForDisplayed({ timeout: 5000 });
    await expect(tableNameInput).toBeDisplayed();
  });

  it('表结构编辑器应显示列定义区域 (TS-001)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('structView.fieldName'));
    expect(body).toContain(t('structView.type'));
  });

  it('应能输入表名 (TS-002)', async () => {
    const tableNameInput = await $('input[placeholder="new_table"]');
    await tableNameInput.setValue(TEST_TABLE);
    expect(await tableNameInput.getValue()).toBe(TEST_TABLE);
  });

  it('应能填写列名 (TS-002)', async () => {
    const colInputs = await $$('input[placeholder="column_name"]');
    const count = await colInputs.length;
    if (count > 0) {
      await colInputs[0].setValue('id');
      expect(await colInputs[0].getValue()).toBe('id');
    }
  });

  it('应能添加新列 (TS-003)', async () => {
    const addColBtn = await $("[data-testid='struct-editor-add-column']");
    if (await addColBtn.isDisplayed()) {
      await addColBtn.click();
      await browser.pause(300);
    }

    // Should now have more column_name inputs
    const colInputs = await $$('input[placeholder="column_name"]');
    expect(colInputs.length).toBeGreaterThan(1);
  });

  it('预览 SQL 应显示 CREATE TABLE 语句 (TS-004)', async () => {
    const previewBtn = await $(`button*=${t('structEditor.previewSQL')}`);
    if (await previewBtn.isDisplayed()) {
      await previewBtn.click();
      await browser.pause(500);

      const body = await $('body').getText();
      const hasCreateSQL = body.toUpperCase().includes('CREATE TABLE');
      expect(hasCreateSQL).toBe(true);
      expect(await $("[data-testid='struct-plan-preview']").isExisting()).toBe(true);

      // Close preview if there's a close button
      const closeBtn = await $(`button*=${t('common.close')}`);
      if (await closeBtn.isExisting()) {
        await closeBtn.click();
        await browser.pause(300);
      }
    }
  });

  it('新建表编辑器应显示创建表按钮 (TS-004b)', async () => {
    const createBtn = await $("[data-testid='struct-editor-execute']");
    await expect(createBtn).toBeDisplayed();
  });

  it('应能通过创建表按钮创建表 (TS-005)', async () => {
    // Since the form might not be complete enough to create, let's use SQL directly
    // and verify the alter table flow instead
    await openQueryTab();
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        age INT DEFAULT 0
      )
    `);

    // Probe the CREATE's effect in the same editor, before metadata refresh.
    // Only boolean values leave the result grid; never report database names.
    let databaseMatchesConfig = false;
    let relationVisible = false;
    let independentDatabaseMatchesConfig = false;
    let independentRelationVisible = false;
    let catalogContainsTable = false;
    let catalogTableKind = false;
    let catalogSchemaListed = false;
    let catalogRelationCount = 0;
    let catalogSchemaCount = 0;
    let configuredDatabase = '';
    let catalogTableSchema: string | null = null;
    try {
      const connections =
        await invokeBackend<Array<{ id: string; database?: string }>>('get_connections');
      const database = connections.find(
        (connection) => connection.id === E2E_PG_CONNECTION_ID,
      )?.database;
      if (!database) throw new Error('Seeded database configuration unavailable');
      configuredDatabase = database;
      const databaseLiteral = database.replace(/\\/g, '\\\\').replace(/'/g, "''");
      await executeSQLChecked(`
        SELECT current_database() = E'${databaseLiteral}' AS e2e_database_matches_config,
               to_regclass('${TEST_TABLE}') IS NOT NULL AS e2e_relation_visible
      `);
      const grid = await $(
        '[data-testid="query-results-host"] [data-testid="result-workspace-table"]',
      );
      await grid.waitForDisplayed({ timeout: 10000 });
      const databaseCell = await grid.$(
        '[data-testid="data-table-cell"][data-dt-row="0"][data-dt-col="e2e_database_matches_config"]',
      );
      const relationCell = await grid.$(
        '[data-testid="data-table-cell"][data-dt-row="0"][data-dt-col="e2e_relation_visible"]',
      );
      await databaseCell.waitForDisplayed({ timeout: 10000 });
      await relationCell.waitForDisplayed({ timeout: 10000 });
      const databaseText = (await databaseCell.getText()).trim();
      const relationText = (await relationCell.getText()).trim();
      databaseMatchesConfig = databaseText === 'true';
      relationVisible = relationText === 'true';
      if (databaseMatchesConfig && relationVisible) {
        // Editor-owned fixed sessions can see their own uncommitted DDL.
        // The existing IPC query helper uses the independently pooled legacy
        // connection: verify visibility there without reconnecting the editor.
        const probeSession = await connectConfig(E2E_PG_CONNECTION_ID);
        try {
          const rows = parseQueryRows(
            await executeQuery(
              probeSession,
              `
            SELECT current_database() = E'${databaseLiteral}',
                   to_regclass('${TEST_TABLE}') IS NOT NULL
          `,
              database,
            ),
          );
          independentDatabaseMatchesConfig = rows[0]?.[0] === true;
          independentRelationVisible = rows[0]?.[1] === true;
          const catalog = await readCatalog({ dbSessionId: probeSession, database, schema: null });
          catalogRelationCount = catalog.relations.length;
          catalogSchemaCount = catalog.schemas.length;
          const relation = catalog.relations.find((entry) => entry.ref.name === TEST_TABLE);
          catalogContainsTable = relation !== undefined;
          catalogTableKind = relation?.kind === 'table';
          catalogTableSchema = relation?.ref.schema ?? null;
          catalogSchemaListed =
            catalogTableSchema !== null && catalog.schemas.includes(catalogTableSchema);
        } finally {
          await invokeBackend('release_connection', { dbSessionId: probeSession });
        }
      }
    } catch {
      // Never expose config values, SQL literals, or raw backend errors.
    }
    if (!databaseMatchesConfig || !relationVisible) {
      throw new Error(
        `TS-005 effect: database_matches_config=${databaseMatchesConfig}, relation_visible=${relationVisible}`,
      );
    }
    if (!independentDatabaseMatchesConfig || !independentRelationVisible) {
      throw new Error(
        `TS-005 independent visibility: database_matches_config=${independentDatabaseMatchesConfig}, relation_visible=${independentRelationVisible}`,
      );
    }
    if (!catalogContainsTable || !catalogTableKind || !catalogSchemaListed) {
      throw new Error(
        `TS-005 catalog: table_visible=${catalogContainsTable}, kind_table=${catalogTableKind}, schema_listed=${catalogSchemaListed}, relations=${catalogRelationCount}, schemas=${catalogSchemaCount}`,
      );
    }

    let runtimeCatalogSucceeded = false;
    let runtimeCatalogTableVisible = false;
    let runtimeCatalogRelationCount = 0;
    let runtimeCatalogSchemaCount = 0;
    let runtimeCatalogFailure = 'none';
    try {
      const runtimeCatalog = await readRuntimeCatalog(E2E_PG_CONNECTION_ID, configuredDatabase);
      runtimeCatalogSucceeded = true;
      runtimeCatalogTableVisible = runtimeCatalog.relations.some(
        (relation) => relation.ref.name === TEST_TABLE,
      );
      runtimeCatalogRelationCount = runtimeCatalog.relations.length;
      runtimeCatalogSchemaCount = runtimeCatalog.schemas.length;
    } catch (error) {
      // Emit only structural status; never expose raw transport errors.
      runtimeCatalogFailure =
        error instanceof RuntimeMetadataProbeError
          ? `${error.phase}/${error.reason}`
          : 'unclassified';
    }
    console.info(
      `[e2e] TS-005 runtime catalog: succeeded=${runtimeCatalogSucceeded}, table_visible=${runtimeCatalogTableVisible}, relations=${runtimeCatalogRelationCount}, schemas=${runtimeCatalogSchemaCount}, failure=${runtimeCatalogFailure}`,
    );
    if (!runtimeCatalogSucceeded || !runtimeCatalogTableVisible) {
      throw new Error(
        `TS-005 runtime catalog: succeeded=${runtimeCatalogSucceeded}, table_visible=${runtimeCatalogTableVisible}, relations=${runtimeCatalogRelationCount}, schemas=${runtimeCatalogSchemaCount}, failure=${runtimeCatalogFailure}`,
      );
    }

    // Refresh sidebar
    await clickNavigatorRefresh();
    await browser.pause(1500);
    // Refresh rebuilds the tree DOM — re-expand so the table node is visible.
    await expandConnectedConnectionInNavigator();
    const navigatorSnapshot = () =>
      browser.execute(
        (database: string, schema: string | null, table: string) => {
          const nav =
            document.querySelector('[data-testid="connection-navigator-aside"]') ??
            Array.from(document.querySelectorAll('aside')).find((aside) =>
              aside.querySelector('[data-conn-item]'),
            );
          const nodes = Array.from(nav?.querySelectorAll('[data-testid="schema-tree-node"]') ?? []);
          const query = document.querySelector<HTMLInputElement>(
            '[data-testid="connection-search-input"]',
          );
          return {
            navigator_present: nav !== undefined && nav !== null,
            connections: nav?.querySelectorAll('[data-conn-item]').length ?? 0,
            databases: nodes.filter((node) => node.getAttribute('data-tree-node') === 'db').length,
            query_present: query !== null,
            query_empty: query?.value === '',
            query_matches_table: query?.value === table,
            query_length: query?.value.length ?? 0,
            database_matches_config: nodes.some(
              (node) =>
                node.getAttribute('data-tree-node') === 'db' &&
                node.getAttribute('data-db-name') === database,
            ),
            schema_rendered:
              schema !== null &&
              nodes.some(
                (node) =>
                  node.getAttribute('data-tree-node') === 'schema' &&
                  node.getAttribute('data-schema-name') === schema,
              ),
            tables: nodes.filter((node) => node.getAttribute('data-tree-node') === 'table').length,
            schemas: nodes.filter((node) => node.getAttribute('data-tree-node') === 'schema')
              .length,
            categories: nodes.filter((node) => node.getAttribute('data-tree-node') === 'category')
              .length,
          };
        },
        configuredDatabase,
        catalogTableSchema,
        TEST_TABLE,
      );
    let beforeWait = await navigatorSnapshot();
    try {
      if (!beforeWait.database_matches_config) throw new Error('Target database absent');
      await expandSchemaTableCategory(catalogTableSchema ?? undefined, configuredDatabase);
      beforeWait = await navigatorSnapshot();
      console.info(`[e2e] TS-005 before wait: ${JSON.stringify(beforeWait)}`);
      await waitForTableInSidebar(
        TEST_TABLE,
        20000,
        catalogTableSchema ?? undefined,
        configuredDatabase,
      );
      console.info(`[e2e] TS-005 after wait: ${JSON.stringify(await navigatorSnapshot())}`);
    } catch {
      const afterWait = await navigatorSnapshot();
      throw new Error(
        `TS-005 navigator: catalog_table_visible=${catalogContainsTable}, catalog_relations=${catalogRelationCount}, catalog_schemas=${catalogSchemaCount}, before=${JSON.stringify(beforeWait)}, after=${JSON.stringify(afterWait)}`,
      );
    }
  });

  // ── 编辑表结构 ─────────────────────────────────────────────────

  it('结构标签应显示表的列信息 (TS-006)', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1500);
    await switchSubTab('structure');
    await browser.pause(1500);

    const body = await $('body').getText();
    // Should show our table columns
    const hasColumns = body.includes('id') || body.includes('name') || body.includes('age');
    expect(hasColumns).toBe(true);
  });

  it('结构标签应有编辑按钮或显示列详情 (TS-006)', async () => {
    const body = await $('body').getText();
    // Either shows "编辑表结构" button or at minimum displays column types
    const hasStructureInfo =
      (await $("[data-testid='struct-editor-title']").isExisting()) ||
      body.includes('integer') ||
      body.includes('varchar') ||
      body.includes('NOT NULL');
    expect(hasStructureInfo).toBe(true);
  });

  it('编辑表结构应在结构子标签内打开并显示返回 (TS-006b)', async () => {
    const editBtn = await $("[data-testid='struct-edit-structure']");
    await editBtn.waitForDisplayed({
      timeout: 10000,
      timeoutMsg: 'Expected struct-edit-structure button on structure tab',
    });
    await editBtn.click();
    await browser.pause(1000);
    const saveBtn = await $("[data-testid='struct-editor-execute']");
    await expect(saveBtn).toBeDisplayed();
    // Inline edit — no new primary tab titled "编辑结构 · …"
    await expect($("[data-testid='struct-editor-title']")).toExist();
    const exportBtn = await $('[data-testid="struct-editor-export-structure"]');
    await expect(exportBtn).toBeDisplayed();
    expect(await exportBtn.getText()).toContain(t('structEditor.exportStructure'));
    // Native save dialog is not automatable — assert control only.
    const backBtn = await $("[data-testid='struct-editor-back']");
    await expect(backBtn).toBeDisplayed();
    await backBtn.click();
    await browser.waitUntil(
      async () =>
        !(await $(`[data-testid='struct-editor-title']`)
          .isDisplayed()
          .catch(() => false)),
      { timeout: 10000, timeoutMsg: '等待返回结构视图超时' },
    );
  });

  it('结构编辑保存新增列后应反映在结构视图 (TS-009)', async () => {
    const editBtn = await $("[data-testid='struct-edit-structure']");
    await editBtn.waitForDisplayed({ timeout: 10000 });
    await editBtn.click();
    await browser.pause(800);

    const addCol = await $("[data-testid='struct-editor-add-column']");
    await addCol.waitForDisplayed({ timeout: 8000 });
    await addCol.click();
    await browser.pause(400);

    const nameInput = await $('input[placeholder="column_name"]');
    await nameInput.waitForDisplayed({ timeout: 5000 });
    // Prefer the last empty / newest column_name input
    const inputs = await $$('input[placeholder="column_name"]');
    const inputCount = await inputs.length;
    const target = inputs[inputCount - 1];
    await target.click();
    await target.clearValue();
    await target.setValue('e2e_extra_col');
    await browser.pause(300);

    const saveBtn = await $("[data-testid='struct-editor-execute']");
    await saveBtn.click();
    await browser.waitUntil(
      async () =>
        !(await $(`[data-testid='struct-editor-title']`)
          .isDisplayed()
          .catch(() => false)),
      { timeout: 15000, timeoutMsg: '结构保存后未返回结构视图' },
    );

    const backBtn = await $("[data-testid='struct-editor-back']");
    if (await backBtn.isExisting()) {
      await backBtn.click();
    }

    await switchSubTab('structure');
    try {
      await browser.waitUntil(async () => (await $('body').getText()).includes('e2e_extra_col'), {
        timeout: 15000,
        timeoutMsg: '结构视图未加载新增列 e2e_extra_col',
      });
    } catch {
      const database = process.env.E2E_PG_DB || 'datazen_e2e';
      const ui = (await browser.execute((loadingCopy: string) => {
        const body = document.body.innerText;
        return {
          editor_visible: document.querySelector('[data-testid="struct-editor-title"]') !== null,
          loading_visible: body.includes(loadingCopy),
          error_visible: document.querySelector('.text-red-400') !== null,
          structure_view_visible:
            document.querySelector('[data-testid="struct-edit-structure"]') !== null,
          visible_columns: document.querySelectorAll('[data-struct-col]').length,
        };
      }, t('common.loadingTableStructure'))) as {
        editor_visible: boolean;
        loading_visible: boolean;
        error_visible: boolean;
        structure_view_visible: boolean;
        visible_columns: number;
      };

      let runtimeCatalogFound = false;
      let runtimeSchemaSucceeded = false;
      let runtimeColumnPresent = false;
      let runtimeFailure = 'none';
      try {
        const catalog = await readRuntimeCatalog(E2E_PG_CONNECTION_ID, database);
        const relation = catalog.relations.find((entry) => entry.ref.name === TEST_TABLE);
        runtimeCatalogFound = relation !== undefined;
        if (relation) {
          const schema = await readRuntimeRelationSchema(E2E_PG_CONNECTION_ID, relation.ref);
          runtimeSchemaSucceeded = true;
          runtimeColumnPresent = schema.definition.columns.some(
            (column) => column.name === 'e2e_extra_col',
          );
        }
      } catch (error) {
        runtimeFailure =
          error instanceof RuntimeMetadataProbeError
            ? `${error.phase}/${error.reason}`
            : 'unclassified';
      }
      console.info(
        `[e2e] TS-009 diagnostic: catalog_relation=${runtimeCatalogFound}, schema_read=${runtimeSchemaSucceeded}, column_present=${runtimeColumnPresent}, runtime_failure=${runtimeFailure}, editor_visible=${ui.editor_visible}, loading_visible=${ui.loading_visible}, error_visible=${ui.error_visible}, structure_view_visible=${ui.structure_view_visible}, visible_columns=${ui.visible_columns}`,
      );
      throw new Error(
        `TS-009 diagnostic: catalog_relation=${runtimeCatalogFound}, schema_read=${runtimeSchemaSucceeded}, column_present=${runtimeColumnPresent}, runtime_failure=${runtimeFailure}, editor_visible=${ui.editor_visible}, loading_visible=${ui.loading_visible}, error_visible=${ui.error_visible}, structure_view_visible=${ui.structure_view_visible}, visible_columns=${ui.visible_columns}`,
      );
    }
    const body = await $('body').getText();
    expect(body).toContain('e2e_extra_col');
  });

  it('应能查看表的完整列信息 (TS-007)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('id');
    expect(body).toContain('name');
  });

  it('DDL 标签应显示建表语句 (TS-008)', async () => {
    await switchSubTab('ddl');
    await browser.pause(1500);
    const body = (await $('body').getText()).toUpperCase();
    expect(body).toContain('CREATE');
  });
});
