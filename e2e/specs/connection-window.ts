import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  clickCardConnectButton,
  closeExtraWindows,
  clickFirstTable,
  openQueryTab,
  executeSQL,
  clickTableInSidebar,
  switchSubTab,
  waitForSchemaTreeLoaded,
  connectSeededPgInWorkspace,
  waitForNewQueryButton,
  rightClickTableInSidebar,
} from '../helpers.js';

const TEST_PARENT = '_e2e_idx_parent';
const TEST_CHILD = '_e2e_idx_child';

async function waitForWebContextMenu() {
  await browser.waitUntil(
    async () => await $(`[data-testid='web-context-menu']`).isExisting(),
    { timeout: 3000, timeoutMsg: 'Timed out waiting for web context menu' },
  );
}

async function openDataCellContextMenu() {
  await browser.execute(() => {
    const cell = document.querySelector<HTMLElement>('[data-dt-row="0"][data-dt-col]');
    if (!cell) throw new Error('No DataTable cell is available for context-menu testing');
    cell.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: 500,
        clientY: 400,
      }),
    );
  });
  await waitForWebContextMenu();
}

async function openDdlContextMenu() {
  await browser.execute(() => {
    const editor = document.querySelector('.cm-editor');
    const content = editor?.closest<HTMLElement>('div.flex.min-h-0.flex-1.flex-col');
    if (!content) throw new Error('No DDL content container is available for context-menu testing');
    content.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: 500,
        clientY: 400,
      }),
    );
  });
  await waitForWebContextMenu();
}

async function closeWebContextMenu() {
  await browser.execute(() => {
    document.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
  });
  await browser.pause(300);
}

async function closeGlobalObjectSearch() {
  await $(`[data-testid='global-object-search'] button`).click();
  await browser.pause(300);
}

/**
 * Database browsing & data viewing tests.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */
describe('数据库浏览模块 (DB-001~DB-010, DE-001, DE-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_CHILD}`);
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_PARENT}`);
    await executeSQL(`
      CREATE TABLE ${TEST_PARENT} (
        id SERIAL PRIMARY KEY,
        code VARCHAR(50) UNIQUE NOT NULL
      )
    `);
    await executeSQL(`
      CREATE TABLE ${TEST_CHILD} (
        id SERIAL PRIMARY KEY,
        parent_id INT NOT NULL,
        name VARCHAR(100) NOT NULL,
        email VARCHAR(200),
        score INT NOT NULL DEFAULT 0,
        CONSTRAINT fk_parent FOREIGN KEY (parent_id)
          REFERENCES ${TEST_PARENT}(id) ON DELETE CASCADE ON UPDATE CASCADE
      )
    `);
    await executeSQL(`CREATE INDEX idx_child_name ON ${TEST_CHILD}(name)`);
    await executeSQL(`CREATE UNIQUE INDEX idx_child_email ON ${TEST_CHILD}(email)`);
    await executeSQL(`CREATE INDEX idx_child_name_score ON ${TEST_CHILD}(name, score)`);
    await executeSQL(`INSERT INTO ${TEST_PARENT}(code) VALUES ('P001'), ('P002')`);
    await executeSQL(`
      INSERT INTO ${TEST_CHILD}(parent_id, name, email, score) VALUES
        (1, 'Alice', 'alice@test.com', 90),
        (2, 'Bob', 'bob@test.com', 85)
    `);

    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
    await browser.pause(2000);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_CHILD}`);
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_PARENT}`);
    } catch {
      // best-effort cleanup
    }
    if (mainWindow === 'main') {
      await closeExtraWindows(mainWindow);
    }
  });

  // ── 工具栏 ──────────────────────────────────────────────────────

  it('连接窗口应显示工具栏 (DB-001)', async () => {
    await expect(await $(`button[title="${t('connWin.refresh')} (⌘R)"]`)).toBeDisplayed();
    await expect(await waitForNewQueryButton()).toBeDisplayed();
  });

  it('工具栏应显示新建表按钮 (DB-001)', async () => {
    await expect($("[data-testid='content-toolbar-new-table']")).toExist();
  });

  it('应显示全局对象搜索入口和输入框 (DB-008)', async () => {
    await $(`[data-testid='global-object-search-toggle']`).click();
    await expect(await $(`[data-testid='global-object-search-input']`)).toBeDisplayed();
    await closeGlobalObjectSearch();
  });

  it('连接窗口状态栏应显示已连接和 PostgreSQL (DB-001)', async () => {
    await expect(await $(`span*=${t('connWin.connected')}`)).toBeDisplayed();
    expect(await $('body').getText()).toContain('PostgreSQL');
  });

  // ── 模式/表列表 ────────────────────────────────────────────────

  it('左侧应显示数据库和表列表 (DB-001, DB-002)', async () => {
    await waitForSchemaTreeLoaded();
  });

  it('点击表名应打开数据标签页 (DB-002, DB-007)', async () => {
    const tableName = await clickFirstTable();
    expect(tableName).not.toBeNull();

    await browser.pause(2000);
    const dataTab = await $("[data-testid='sub-tab-data']");
    await dataTab.waitForDisplayed({ timeout: 8000 });
    await expect(dataTab).toBeDisplayed();
  });

  it('应显示结构/数据/索引/外键/DDL 子标签 (DB-003~DB-005, DB-010)', async () => {
    for (const label of ['数据', '结构', '索引', '外键', 'DDL']) {
      await expect(await $(`button*=${label}`)).toBeDisplayed();
    }
  });

  // ── 结构 tab ────────────────────────────────────────────────────

  it('点击结构标签应显示字段信息 (DB-003)', async () => {
    const structTab = await $("[data-testid='sub-tab-structure']");
    await structTab.click();
    await browser.pause(2000);

    const body = await $('body').getText();
    const hasStructure =
      body.includes('字段名') ||
      body.includes('类型') ||
      body.includes('integer') ||
      body.includes('varchar') ||
      body.includes('text') ||
      body.includes('boolean') ||
      body.includes('timestamp');
    expect(hasStructure).toBe(true);
  });

  it('结构标签应显示列名和数据类型 (DB-003)', async () => {
    const body = await $('body').getText();
    // Any real table will have at least one recognizable type
    const hasColumns =
      body.includes('NOT NULL') ||
      body.includes('NULL') ||
      body.includes('PRIMARY') ||
      body.includes('DEFAULT') ||
      body.includes('integer') ||
      body.includes('varchar') ||
      body.includes('text');
    expect(hasColumns).toBe(true);
  });

  // ── 索引 tab（使用带索引的测试表）─────────────────────────────────

  it('索引标签应显示索引列表 (DB-004)', async () => {
    await clickTableInSidebar(TEST_CHILD);
    await browser.pause(1500);
    await switchSubTab('indexes');
    await browser.pause(2000);

    const body = await $('body').getText();
    expect(body).toContain(t('indexes.colName'));
    expect(body).toContain('个索引');
  });

  it('索引标签应显示主键索引 (DB-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('pkey');
    expect(body).toContain('id');
  });

  it('索引标签应显示自定义索引 (DB-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('idx_child_name');
    expect(body).toContain('name');
  });

  it('索引标签应显示唯一索引 (DB-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('idx_child_email');
    expect(body).toContain('email');
    expect(body).toContain('YES');
  });

  it('索引标签应显示复合索引的多列 (DB-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('idx_child_name_score');
    expect(body).toContain('score');
  });

  it('索引标签应显示在表结构中编辑按钮 (DB-004)', async () => {
    const editBtn = await $("[data-testid='idx-edit-in-structure']");
    await expect(editBtn).toBeDisplayed();
  });

  it('点击在表结构中编辑应切换到结构子标签 (DB-004a)', async () => {
    const editBtn = await $("[data-testid='idx-edit-in-structure']");
    await editBtn.click();
    await browser.pause(800);

    const body = await $('body').getText();
    // Stays on the same table primary tab; secondary tab switches to Structure.
    expect(body).toContain(t('structView.fieldName'));
    expect(await $("[data-testid='struct-editor-title']").isExisting()).toBe(false);
  });

  // ── 外键 tab ────────────────────────────────────────────────────

  it('外键标签应显示外键列表 (DB-005)', async () => {
    await switchSubTab('foreignKeys');
    await browser.pause(2000);

    const body = await $('body').getText();
    expect(body).toContain(t('fk.constraintName'));
    expect(body).toContain('1 个外键');
  });

  it('外键标签应显示外键详情 (DB-005)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('fk_parent');
    expect(body).toContain('parent_id');
    expect(body).toContain(TEST_PARENT);
    expect(body).toContain('CASCADE');
  });

  // ── DDL tab ─────────────────────────────────────────────────────

  it('点击 DDL 标签应显示建表语句 (DB-010)', async () => {
    const ddlTab = await $('button*=DDL');
    await ddlTab.click();
    await browser.pause(2000);
    const body = (await $('body').getText()).toUpperCase();
    expect(body).toContain('CREATE');
  });

  it('DDL 标签应有复制按钮 (DB-010)', async () => {
    const copyBtn = await $(`button*=${t('common.copy')}`);
    await expect(copyBtn).toBeDisplayed();
  });

  it('点击复制按钮后应显示已复制 (DB-010)', async () => {
    const copyBtn = await $(`button*=${t('common.copy')}`);
    await copyBtn.click();
    await browser.pause(500);

    await browser.waitUntil(async () => (await $('body').getText()).includes('已复制'), {
      timeout: 3000,
      timeoutMsg: 'Timed out waiting for copied toast',
    });
  });

  // ── 数据 tab ────────────────────────────────────────────────────

  it('切回数据标签应正常显示 (DE-001)', async () => {
    const dataTab = await $("[data-testid='sub-tab-data']");
    await dataTab.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('common.selectAll')) || body.includes('行') || body.includes('加载中')
        );
      },
      { timeout: 15000, timeoutMsg: 'Timed out waiting for data view to render' },
    );
  });

  // ── 右键菜单 ──────────────────────────────────────────────────

  it('数据标签右键菜单应包含复制单元格 (CTX-001)', async () => {
    await clickTableInSidebar(TEST_CHILD);
    await browser.pause(1500);
    await switchSubTab('data');
    await browser.pause(2000);

    await openDataCellContextMenu();

    await expect($("[data-testid='web-context-item-copy']")).toExist();
    await expect($("[data-testid='web-context-item-copy-row']")).toExist();
    await expect($("[data-testid='web-context-item-filter-by-value']")).toExist();
    await expect($("[data-testid='web-context-submenu-trigger-more-actions']")).toExist();
    await expect($("[data-testid='web-context-item-open-structure']")).not.toExist();
    await expect($("[data-testid='web-context-item-refresh']")).not.toExist();
    await expect($("[data-testid='web-context-item-new-query']")).not.toExist();
    await expect($("[data-testid='web-context-item-copy-ddl']")).not.toExist();

    await closeWebContextMenu();
  });

  it('结构标签对应的表节点右键菜单应包含结构入口 (CTX-002)', async () => {
    await switchSubTab('structure');
    await browser.pause(2000);

    await rightClickTableInSidebar(TEST_CHILD);

    await expect($("[data-testid='web-context-item-open-structure']")).toExist();
    await expect($("[data-testid='web-context-item-new-query']")).toExist();
    await expect($("[data-testid='web-context-item-copy']")).not.toExist();
    await expect($("[data-testid='web-context-item-copy-name']")).toExist();
    await expect($("[data-testid='web-context-item-copy-ddl']")).toExist();
    await expect($("[data-testid='web-context-item-refresh']")).not.toExist();

    await closeWebContextMenu();
  });

  it('索引标签对应的表节点右键菜单应包含 SQL 操作 (CTX-003)', async () => {
    await switchSubTab('indexes');
    await browser.pause(2000);

    await rightClickTableInSidebar(TEST_CHILD);

    await expect($("[data-testid='web-context-item-open-structure']")).toExist();
    await expect($("[data-testid='web-context-item-new-query']")).toExist();
    await expect($("[data-testid='web-context-item-copy']")).not.toExist();
    await expect($("[data-testid='web-context-item-copy-name']")).toExist();
    await expect($("[data-testid='web-context-item-copy-ddl']")).toExist();
    await expect($("[data-testid='web-context-item-refresh']")).not.toExist();

    await closeWebContextMenu();
  });

  it('外键标签对应的表节点右键菜单不应包含数据专属项 (CTX-004)', async () => {
    await switchSubTab('foreignKeys');
    await browser.pause(2000);

    await rightClickTableInSidebar(TEST_CHILD);

    await expect($("[data-testid='web-context-item-new-query']")).toExist();
    await expect($("[data-testid='web-context-item-copy']")).not.toExist();
    await expect($("[data-testid='web-context-item-filter-by-value']")).not.toExist();
    await expect($("[data-testid='web-context-item-refresh']")).not.toExist();
    await expect($("[data-testid='web-context-item-copy-ddl']")).toExist();

    await closeWebContextMenu();
  });

  it('DDL 标签右键菜单应包含复制 DDL (CTX-005)', async () => {
    await switchSubTab('ddl');
    await browser.pause(2000);

    await openDdlContextMenu();

    await expect($("[data-testid='web-context-item-copy-ddl']")).toExist();
    await expect($("[data-testid='web-context-item-copy']")).not.toExist();
    await expect($("[data-testid='web-context-item-open-structure']")).not.toExist();
    await expect($("[data-testid='web-context-item-refresh']")).not.toExist();
    await expect($("[data-testid='web-context-item-new-query']")).not.toExist();

    await closeWebContextMenu();
  });

  it('索引标签右键菜单编辑结构应在结构子标签内打开编辑器 (CTX-006)', async () => {
    await switchSubTab('indexes');
    await browser.pause(2000);

    await rightClickTableInSidebar(TEST_CHILD);

    await $("[data-testid='web-context-item-open-structure']").click();
    await browser.pause(800);

    await expect($("[data-testid='struct-editor-title']")).toExist();
    const backBtn = await $("[data-testid='struct-editor-back']");
    await expect(backBtn).toBeDisplayed();
  });

  // ── 搜索表 ─────────────────────────────────────────────────────

  it('全局对象搜索应能输入文字并定位测试表 (DB-008)', async () => {
    await $(`[data-testid='global-object-search-toggle']`).click();
    const search = await $(`[data-testid='global-object-search-input']`);
    await search.setValue(TEST_CHILD);
    await browser.waitUntil(
      async () => (await $$(`[data-testid='global-object-search-result']`)).length > 0,
      { timeout: 5000, timeoutMsg: `未找到全局搜索结果 ${TEST_CHILD}` },
    );
    expect(await $('body').getText()).toContain(TEST_CHILD);
    await closeGlobalObjectSearch();
  });

  it('全局对象搜索清空后应恢复结果列表 (DB-008)', async () => {
    await $(`[data-testid='global-object-search-toggle']`).click();
    const search = await $(`[data-testid='global-object-search-input']`);
    await search.setValue('nonexistent_xyz_table_12345');
    await browser.waitUntil(
      async () => (await $$(`[data-testid='global-object-search-result']`)).length === 0,
      { timeout: 5000, timeoutMsg: '全局搜索无结果状态未出现' },
    );
    await search.clearValue();
    await browser.waitUntil(
      async () => (await $$(`[data-testid='global-object-search-result']`)).length > 0,
      { timeout: 5000, timeoutMsg: '清空全局搜索后未恢复结果' },
    );
    expect(await $('body').getText()).toContain(TEST_CHILD);
    await closeGlobalObjectSearch();
  });

  // ── 新建查询 ───────────────────────────────────────────────────

  it('应能打开新建查询标签 (SQ-003)', async () => {
    const newQueryBtn = await waitForNewQueryButton();
    await newQueryBtn.click();
    await browser.pause(1000);
    await expect(await $(`button*=${t('query.execute')}`)).toBeDisplayed();
  });

  it('新建查询不应弹出对象加载补全框 (SQ-AC-001)', async () => {
    const newQueryBtn = await waitForNewQueryButton();
    await newQueryBtn.click();
    let loadingHint = false;
    const deadline = Date.now() + 1500;
    while (Date.now() < deadline) {
      const tooltip = await $('.cm-tooltip-autocomplete');
      if (await tooltip.isExisting()) {
        const text = await tooltip.getText();
        if (text.includes(t('query.namespaceLoading'))) {
          loadingHint = true;
          break;
        }
      }
      await browser.pause(100);
    }
    expect(loadingHint).toBe(false);
    await expect(await $(`button*=${t('query.execute')}`)).toBeDisplayed();
  });
});
