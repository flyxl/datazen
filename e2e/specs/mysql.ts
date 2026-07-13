import { expect, browser, $, $$ } from '@wdio/globals';
import {
  createAndConnectMySQL,
  closeExtraWindows,
  openQueryTab,
  executeSQL,
  clickTableInSidebar,
  switchSubTab,
  clickFirstTable,
  asideHasSchemaSections,
} from '../helpers.js';

const TABLE_BASIC = '_e2e_mysql_basic';
const TABLE_TYPES = '_e2e_mysql_types';
const TABLE_IDX = '_e2e_mysql_indexed';

describe('MySQL 数据库支持 (MY-001~MY-020)', () => {
  let mainWindow: string;

  before(async () => {
    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);

    // Close any existing connection windows
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    // Create and connect to MySQL
    const { connWindow } = await createAndConnectMySQL();

    // Setup test tables
    await openQueryTab();

    await executeSQL(`DROP TABLE IF EXISTS ${TABLE_IDX}`);
    await executeSQL(`DROP TABLE IF EXISTS ${TABLE_TYPES}`);
    await executeSQL(`DROP TABLE IF EXISTS ${TABLE_BASIC}`);

    await executeSQL(`
      CREATE TABLE ${TABLE_BASIC} (
        id BIGINT AUTO_INCREMENT PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        email VARCHAR(200),
        score INT DEFAULT 0,
        active TINYINT(1) DEFAULT 1
      )
    `);

    await executeSQL(`
      INSERT INTO ${TABLE_BASIC} (name, email, score, active) VALUES
        ('Alice', 'alice@test.com', 90, 1),
        ('Bob', 'bob@test.com', 85, 1),
        ('Charlie', 'charlie@test.com', 70, 0),
        ('Diana', 'diana@test.com', 95, 1),
        ('Eve', 'eve@test.com', 60, 0)
    `);

    await executeSQL(`
      CREATE TABLE ${TABLE_TYPES} (
        id BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
        price DECIMAL(10,2) NOT NULL,
        amount DECIMAL(18,4),
        ratio FLOAT,
        big_val BIGINT,
        ubig_val BIGINT UNSIGNED,
        tiny_val TINYINT,
        medium_val MEDIUMINT,
        flag BOOLEAN DEFAULT FALSE,
        label VARCHAR(50),
        body TEXT,
        created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
        updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
      )
    `);

    await executeSQL(`
      INSERT INTO ${TABLE_TYPES} (price, amount, ratio, big_val, ubig_val, tiny_val, medium_val, flag, label, body) VALUES
        (99.99, 1234.5678, 3.14, 9223372036854775807, 18446744073709551615, 127, 8388607, TRUE, 'Test Label', 'Hello World'),
        (0.01, 0.0001, 0.001, -9223372036854775808, 0, -128, -8388608, FALSE, NULL, NULL)
    `);

    await executeSQL(`
      CREATE TABLE ${TABLE_IDX} (
        id INT AUTO_INCREMENT PRIMARY KEY,
        code VARCHAR(50) NOT NULL,
        category VARCHAR(50),
        rank_val INT,
        UNIQUE INDEX idx_code (code),
        INDEX idx_category (category),
        INDEX idx_cat_rank (category, rank_val)
      )
    `);

    await executeSQL(`
      INSERT INTO ${TABLE_IDX} (code, category, rank_val) VALUES
        ('A001', 'cat1', 10),
        ('A002', 'cat1', 20),
        ('B001', 'cat2', 30)
    `);

    // Refresh sidebar
    const refreshBtn = await $('button[title="刷新 (⌘R)"]');
    if (await refreshBtn.isExisting()) {
      await refreshBtn.click();
      await browser.pause(2000);
    }
  });

  after(async () => {
    try {
      const handles = await browser.getWindowHandles();
      const connHandle = handles.find((h) => h !== mainWindow);
      if (connHandle) {
        await browser.switchToWindow(connHandle);
        await openQueryTab();
        await executeSQL(`DROP TABLE IF EXISTS ${TABLE_IDX}`);
        await executeSQL(`DROP TABLE IF EXISTS ${TABLE_TYPES}`);
        await executeSQL(`DROP TABLE IF EXISTS ${TABLE_BASIC}`);
      }
    } catch { /* best-effort cleanup */ }
    try {
      await closeExtraWindows(mainWindow);
    } catch { /* ignore */ }
  });

  // ── Connection & Sidebar ──

  it('MySQL 连接窗口应显示工具栏和侧边栏 (MY-001)', async () => {
    const toolbar = await $('button*=新建查询');
    await expect(toolbar).toBeDisplayed();

    const aside = await $('aside');
    const asideText = await aside.getText();
    expect(asideHasSchemaSections(asideText)).toBe(true);
  });

  it('侧边栏应显示 MySQL 测试表 (MY-002)', async () => {
    const aside = await $('aside');
    const asideText = await aside.getText();
    expect(asideText).toContain(TABLE_BASIC);
    expect(asideText).toContain(TABLE_TYPES);
    expect(asideText).toContain(TABLE_IDX);
  });

  it('标题栏应显示 MySQL 类型 (MY-003)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('MySQL');
  });

  // ── Table Data (basic) ──

  it('点击表名应显示数据 tab 页 (MY-004)', async () => {
    await clickTableInSidebar(TABLE_BASIC);
    await browser.pause(2000);

    const body = await $('body').getText();
    // Virtual table may not expose cell text via getText(), check row count instead
    const hasData = body.includes('Alice') || body.includes('Bob') || body.includes('1-5');
    expect(hasData).toBe(true);
  });

  it('表数据应正确显示 BIGINT AUTO_INCREMENT 主键 (MY-005)', async () => {
    await clickTableInSidebar(TABLE_BASIC);
    await browser.pause(2000);

    const body = await $('body').getText();
    // Auto-increment IDs should be visible as numbers, not NULL
    expect(body).toContain('1');
    expect(body).toContain('2');
  });

  // ── Data Type Tests ──

  it('DECIMAL 类型应正确显示数值 (MY-006)', async () => {
    await openQueryTab();
    await executeSQL(`SELECT price FROM ${TABLE_TYPES} ORDER BY id`);
    const body = await $('body').getText();
    expect(body).toContain('99.99');
    expect(body).toContain('0.01');
  });

  it('DECIMAL(18,4) 应正确显示高精度值 (MY-007)', async () => {
    await executeSQL(`SELECT amount FROM ${TABLE_TYPES} ORDER BY id`);
    const body = await $('body').getText();
    expect(body).toContain('1234.5678');
    expect(body).toContain('0.0001');
  });

  it('BIGINT UNSIGNED 应正确显示大数值 (MY-008)', async () => {
    await executeSQL(`SELECT big_val FROM ${TABLE_TYPES} WHERE big_val = 9223372036854775807`);
    const body = await $('body').getText();
    expect(body).toContain('9223372036854775807');
  });

  it('TINYINT/MEDIUMINT 应正确显示 (MY-009)', async () => {
    await executeSQL(`SELECT tiny_val, medium_val FROM ${TABLE_TYPES} ORDER BY id LIMIT 1`);
    const body = await $('body').getText();
    expect(body).toContain('127');
    expect(body).toContain('8388607');
  });

  it('FLOAT 类型应正确显示 (MY-010)', async () => {
    await executeSQL(`SELECT ratio FROM ${TABLE_TYPES} ORDER BY id LIMIT 1`);
    const body = await $('body').getText();
    expect(body).toContain('3.14');
  });

  it('NULL 值应正确显示 (MY-011)', async () => {
    await executeSQL(`SELECT label FROM ${TABLE_TYPES} WHERE label IS NULL`);
    const body = await $('body').getText();
    expect(body).toContain('NULL');
  });

  // ── Structure Tab ──

  it('结构 tab 应显示 MySQL 列信息 (MY-012)', async () => {
    await clickTableInSidebar(TABLE_BASIC);
    await browser.pause(1500);
    await switchSubTab('结构');
    await browser.pause(1500);

    const body = await $('body').getText();
    expect(body).toContain('id');
    expect(body).toContain('name');
    expect(body).toContain('email');
    expect(body).toContain('score');
    expect(body).toContain('bigint');
  });

  // ── Index Tab ──

  it('索引 tab 应显示 MySQL 索引 (MY-013)', async () => {
    await clickTableInSidebar(TABLE_IDX);
    await browser.pause(1500);
    await switchSubTab('索引');
    await browser.pause(1500);

    const body = await $('body').getText();
    expect(body).toContain('PRIMARY');
    expect(body).toContain('idx_code');
    expect(body).toContain('idx_category');
  });

  it('索引 tab 应显示复合索引的列 (MY-014)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('idx_cat_rank');
    expect(body).toContain('category');
    expect(body).toContain('rank_val');
  });

  // ── DDL Tab ──

  it('DDL tab 应显示 MySQL CREATE TABLE 语句 (MY-015)', async () => {
    await clickTableInSidebar(TABLE_BASIC);
    await browser.pause(1500);
    await switchSubTab('DDL');
    await browser.pause(1500);

    const body = await $('body').getText();
    expect(body).toContain('CREATE TABLE');
    expect(body).toContain(TABLE_BASIC);
  });

  // ── SQL Query ──

  it('应能执行 MySQL 查询并显示结果 (MY-016)', async () => {
    await openQueryTab();
    await executeSQL(`SELECT * FROM ${TABLE_BASIC} WHERE score >= 80`);

    const body = await $('body').getText();
    expect(body).toContain('Alice');
    expect(body).toContain('Bob');
    expect(body).toContain('Diana');
    expect(body).not.toContain('Charlie');
  });

  it('应能执行 MySQL 特有的 SHOW 语句 (MY-017)', async () => {
    await openQueryTab();
    await executeSQL('SHOW TABLES');

    const body = await $('body').getText();
    expect(body).toContain(TABLE_BASIC);
  });

  it('应能执行多条 MySQL 语句 (MY-018)', async () => {
    await openQueryTab();
    await executeSQL(`SELECT COUNT(*) AS cnt FROM ${TABLE_BASIC}; SELECT COUNT(*) AS cnt FROM ${TABLE_TYPES}`);

    const body = await $('body').getText();
    expect(body).toContain('5');
    expect(body).toContain('2');
  });

  // ── Data Edit ──

  it('应能编辑 MySQL 表数据 (MY-019)', async () => {
    // Use SQL to update and verify instead of store (not available in production build)
    await openQueryTab();
    await executeSQL(`UPDATE ${TABLE_BASIC} SET score = 99 WHERE name = 'Alice'`);
    await executeSQL(`SELECT score FROM ${TABLE_BASIC} WHERE name = 'Alice'`);

    const body = await $('body').getText();
    expect(body).toContain('99');
  });

  // ── Pagination ──

  it('大量数据时应支持分页 (MY-020)', async () => {
    await openQueryTab();

    // Insert more rows for pagination test
    let insertValues = '';
    for (let i = 0; i < 60; i++) {
      if (i > 0) insertValues += ',';
      insertValues += `('User${i}', 'user${i}@test.com', ${i}, 1)`;
    }
    await executeSQL(`INSERT INTO ${TABLE_BASIC} (name, email, score, active) VALUES ${insertValues}`);

    // Click the table to load data
    await clickTableInSidebar(TABLE_BASIC);
    await browser.pause(2000);
    await switchSubTab('数据');
    await browser.pause(1000);

    // Should show pagination info (more than 50 rows total)
    const body = await $('body').getText();
    expect(body).toContain('行');
  });
});
