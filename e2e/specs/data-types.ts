/**
 * E2E tests for data type display correctness.
 *
 * Verifies that all common PostgreSQL and MySQL column types are decoded by
 * the Rust backend and rendered correctly in the frontend grid.
 *
 * The tests use SQL queries (SELECT … literal) so they exercise the full
 * decode_rows path without depending on table browsing UX details.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  openConnectionWindow,
  createAndConnectMySQL,
  closeExtraWindows,
  openQueryTab,
  executeSQL,
  clickTableInSidebar,
  switchSubTab,
} from '../helpers.js';

// ── helpers ─────────────────────────────────────────────────────────

const PG_TABLE = '_e2e_pg_types';
const MY_TABLE = '_e2e_my_types';

/** Wait for query result grid to contain every expected string. */
async function expectBodyContains(expected: string[], timeout = 10000) {
  for (const str of expected) {
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(str),
      { timeout, timeoutMsg: `前端未显示预期值: "${str}"` },
    );
  }
}

/** Run a query and verify every expected string appears in the result. */
async function queryAndExpect(sql: string, expected: string[]) {
  await openQueryTab();
  await executeSQL(sql);
  await expectBodyContains(expected);
}

/**
 * Click the table in sidebar, switch to 数据 tab, and verify expected
 * strings appear in the grid body.
 */
async function browseTableAndExpect(table: string, expected: string[]) {
  await clickTableInSidebar(table);
  await browser.pause(3000);
  await switchSubTab('数据');
  // Wait for data to finish loading (spinner gone)
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return !body.includes('加载表数据') && (body.includes('全选') || body.includes('每页'));
    },
    { timeout: 20000, timeoutMsg: `等待表 "${table}" 数据加载完成超时` },
  );
  await browser.pause(500);
  await expectBodyContains(expected, 15000);
}

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  PostgreSQL data types
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('PostgreSQL 字段类型前端展示 (PG-TYPE-001~015)', () => {
  let mainWindow: string;

  before(async () => {
    const handles = await browser.getWindowHandles();
    mainWindow = handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    const { connWindow } = await openConnectionWindow();
    await openQueryTab();

    // Create a comprehensive types table
    await executeSQL(`DROP TABLE IF EXISTS ${PG_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${PG_TABLE} (
        c_serial       SERIAL PRIMARY KEY,
        c_smallint     SMALLINT NOT NULL,
        c_integer      INTEGER NOT NULL,
        c_bigint       BIGINT NOT NULL,
        c_real         REAL NOT NULL,
        c_double       DOUBLE PRECISION NOT NULL,
        c_numeric      NUMERIC(12,4) NOT NULL,
        c_boolean      BOOLEAN NOT NULL,
        c_varchar      VARCHAR(100) NOT NULL,
        c_text         TEXT NOT NULL,
        c_char         CHAR(5) NOT NULL,
        c_date         DATE NOT NULL,
        c_time         TIME NOT NULL,
        c_timestamp    TIMESTAMP NOT NULL,
        c_timestamptz  TIMESTAMPTZ NOT NULL,
        c_uuid         UUID NOT NULL,
        c_json         JSON NOT NULL,
        c_jsonb        JSONB NOT NULL,
        c_inet         INET,
        c_interval     INTERVAL
      )
    `);

    await executeSQL(`
      INSERT INTO ${PG_TABLE} (
        c_smallint, c_integer, c_bigint, c_real, c_double, c_numeric,
        c_boolean, c_varchar, c_text, c_char, c_date, c_time,
        c_timestamp, c_timestamptz, c_uuid, c_json, c_jsonb,
        c_inet, c_interval
      ) VALUES (
        32767, 2147483647, 9223372036854775807, 3.14, 2.718281828459045, 12345678.1234,
        TRUE, 'Hello World', 'Long text content here', 'ABCDE',
        '2026-01-15', '14:30:00', '2026-01-15 14:30:00', '2026-01-15T14:30:00+08:00',
        'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11',
        '{"key":"value","num":42}', '{"nested":{"arr":[1,2,3]}}',
        '192.168.1.1', '2 hours 30 minutes'
      )
    `);

    await executeSQL(`
      INSERT INTO ${PG_TABLE} (
        c_smallint, c_integer, c_bigint, c_real, c_double, c_numeric,
        c_boolean, c_varchar, c_text, c_char, c_date, c_time,
        c_timestamp, c_timestamptz, c_uuid, c_json, c_jsonb,
        c_inet, c_interval
      ) VALUES (
        -32768, -2147483648, -9223372036854775808, -0.001, 0.0, 0.0000,
        FALSE, '', 'NULL-like text', 'X    ',
        '1970-01-01', '00:00:00', '1970-01-01 00:00:00', '1970-01-01T00:00:00Z',
        '00000000-0000-0000-0000-000000000000',
        '[]', 'null',
        NULL, NULL
      )
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
        await executeSQL(`DROP TABLE IF EXISTS ${PG_TABLE}`);
      }
    } catch { /* best-effort */ }
    try {
      const handles = await browser.getWindowHandles();
      if (handles.length > 1) {
        await closeExtraWindows(mainWindow);
      }
      await browser.switchToWindow(mainWindow);
      await browser.pause(1000);
    } catch { /* ignore */ }
  });

  // ── Integer types ──

  it('SERIAL 主键应正确显示自增 ID (PG-TYPE-001)', async () => {
    await browseTableAndExpect(PG_TABLE, ['1', '2']);
  });

  it('SMALLINT 应正确显示边界值 (PG-TYPE-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('32767');
    expect(body).toContain('-32768');
  });

  it('INTEGER 应正确显示边界值 (PG-TYPE-003)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('2147483647');
    expect(body).toContain('-2147483648');
  });

  it('BIGINT 应正确显示边界值 (PG-TYPE-004)', async () => {
    const body = await $('body').getText();
    // BIGINT max exceeds JS MAX_SAFE_INTEGER, backend sends as string
    expect(body).toContain('9223372036854775807');
    expect(body).toContain('-9223372036854775808');
  });

  // ── Floating-point types ──

  it('REAL 应正确显示浮点数 (PG-TYPE-005)', async () => {
    const body = await $('body').getText();
    // REAL is float32, so 3.14 may show as 3.14 or 3.140000104904175
    const hasReal = body.includes('3.14');
    expect(hasReal).toBe(true);
  });

  it('DOUBLE PRECISION 应正确显示浮点数 (PG-TYPE-006)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('2.71828');
  });

  it('NUMERIC 应正确显示定点数 (PG-TYPE-007)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('12345678.1234');
  });

  // ── Boolean ──

  it('BOOLEAN 应正确显示 true/false (PG-TYPE-008)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('true');
    expect(body).toContain('false');
  });

  // ── String types ──

  it('VARCHAR 应正确显示文本 (PG-TYPE-009)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('Hello World');
  });

  it('TEXT 应正确显示文本 (PG-TYPE-009)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('Long text content here');
  });

  // ── Date/Time types ──

  it('DATE 应正确显示日期 (PG-TYPE-010)', async () => {
    const body = await $('body').getText();
    // Backend sends "2026-01-15", frontend formatTimestamp converts to ISO
    expect(body).toContain('2026-01-15');
    expect(body).toContain('1970-01-01');
  });

  it('TIMESTAMP 应正确显示时间戳 (PG-TYPE-011)', async () => {
    const body = await $('body').getText();
    // Backend sends "2026-01-15 14:30:00", frontend formats as ISO timestamp
    expect(body).toContain('2026-01-15');
    expect(body).toContain('14:30');
  });

  // ── UUID ──

  it('UUID 应正确显示 UUID 值 (PG-TYPE-012)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11');
  });

  // ── JSON types ──

  it('JSON/JSONB 应正确显示 JSON 内容 (PG-TYPE-013)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('key');
    expect(body).toContain('value');
    expect(body).toContain('nested');
  });

  // ── Network / Interval ──

  it('INTERVAL 应正确显示 (PG-TYPE-014)', async () => {
    const body = await $('body').getText();
    // Interval "2 hours 30 minutes" → "02:30:00"
    expect(body).toContain('02:30:00');
  });

  // ── NULL 值 ──

  it('NULL 值应正确显示为 NULL (PG-TYPE-015)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('NULL');
  });

  // ── Query-based verification for precision ──

  it('SELECT 查询应正确返回所有 PG 整数类型 (PG-TYPE-Q01)', async () => {
    await queryAndExpect(
      `SELECT 42::smallint AS si, 2147483647::integer AS i, 9223372036854775807::bigint AS bi`,
      ['42', '2147483647', '9223372036854775807'],
    );
  });

  it('SELECT 查询应正确返回 PG 浮点/定点类型 (PG-TYPE-Q02)', async () => {
    await queryAndExpect(
      `SELECT 3.14::real AS r, 2.718281828::double precision AS d, 99999.9999::numeric(10,4) AS n`,
      ['3.14', '2.71828', '99999.9999'],
    );
  });

  it('SELECT 查询应正确返回 PG BOOLEAN 类型 (PG-TYPE-Q03)', async () => {
    await queryAndExpect(
      `SELECT true AS t, false AS f`,
      ['true', 'false'],
    );
  });

  it('SELECT 查询应正确返回 PG 日期时间类型 (PG-TYPE-Q04)', async () => {
    await queryAndExpect(
      `SELECT '2026-04-16'::date AS d, '14:30:00'::time AS t, '2026-04-16 10:30:00'::timestamp AS ts`,
      ['2026-04-16', '14:30', '10:30'],
    );
  });

  it('SELECT 查询应正确返回 PG UUID (PG-TYPE-Q05)', async () => {
    await queryAndExpect(
      `SELECT 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'::uuid AS id`,
      ['a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11'],
    );
  });

  it('SELECT 查询应正确返回 PG JSON/JSONB (PG-TYPE-Q06)', async () => {
    await queryAndExpect(
      `SELECT '{"a":1}'::json AS j, '{"b":2}'::jsonb AS jb`,
      ['{"a":1}', '{"b":2}'],
    );
  });
});

// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
//  MySQL data types
// ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

describe('MySQL 字段类型前端展示 (MY-TYPE-001~020)', () => {
  let mainWindow: string;

  before(async () => {
    const handles = await browser.getWindowHandles();
    mainWindow = handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    await createAndConnectMySQL({
      name: 'E2E-MySQL-Types',
      host: '127.0.0.1',
      port: '3306',
      user: 'root',
      password: '',
      database: 'datazen_test',
    });

    await openQueryTab();

    await executeSQL(`DROP TABLE IF EXISTS ${MY_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${MY_TABLE} (
        c_tinyint       TINYINT NOT NULL,
        c_tinyint_u     TINYINT UNSIGNED NOT NULL,
        c_smallint      SMALLINT NOT NULL,
        c_smallint_u    SMALLINT UNSIGNED NOT NULL,
        c_mediumint     MEDIUMINT NOT NULL,
        c_mediumint_u   MEDIUMINT UNSIGNED NOT NULL,
        c_int           INT NOT NULL,
        c_int_u         INT UNSIGNED NOT NULL,
        c_bigint        BIGINT NOT NULL,
        c_bigint_u      BIGINT UNSIGNED NOT NULL,
        c_float         FLOAT NOT NULL,
        c_double        DOUBLE NOT NULL,
        c_decimal_10_2  DECIMAL(10,2) NOT NULL,
        c_decimal_18_4  DECIMAL(18,4) NOT NULL,
        c_decimal_int   DECIMAL(10,0) NOT NULL,
        c_boolean       BOOLEAN NOT NULL,
        c_varchar       VARCHAR(200) NOT NULL,
        c_char          CHAR(10) NOT NULL,
        c_text          TEXT NOT NULL,
        c_date          DATE NOT NULL,
        c_time          TIME NOT NULL,
        c_datetime      DATETIME NOT NULL,
        c_timestamp     TIMESTAMP NOT NULL,
        c_year          YEAR NOT NULL,
        c_json          JSON,
        c_enum          ENUM('small', 'medium', 'large') NOT NULL,
        c_set_col       SET('read', 'write', 'exec') NOT NULL,
        c_nullable_int  INT,
        c_nullable_vc   VARCHAR(100)
      )
    `);

    // Row 1: positive / max-range boundary values
    await executeSQL(`
      INSERT INTO ${MY_TABLE} (
        c_tinyint, c_tinyint_u, c_smallint, c_smallint_u,
        c_mediumint, c_mediumint_u, c_int, c_int_u,
        c_bigint, c_bigint_u,
        c_float, c_double, c_decimal_10_2, c_decimal_18_4, c_decimal_int,
        c_boolean, c_varchar, c_char, c_text,
        c_date, c_time, c_datetime, c_timestamp, c_year,
        c_json, c_enum, c_set_col,
        c_nullable_int, c_nullable_vc
      ) VALUES (
        -128, 255, -32768, 65535,
        -8388608, 16777215, -2147483648, 4294967295,
        9223372036854775807, 18446744073709551615,
        3.14, 2.718281828459045, 12345678.99, 9876543210.1234, 42,
        TRUE, 'Hello DataZen', 'ABCDEFGHIJ', 'Long text content for testing',
        '2026-04-16', '14:30:59',
        '2026-04-16 14:30:59', '2026-04-16 14:30:59',
        2026,
        '{"key":"value","nums":[1,2,3]}',
        'medium', 'read,write',
        NULL, NULL
      )
    `);

    // Row 2: opposite boundary values
    await executeSQL(`
      INSERT INTO ${MY_TABLE} (
        c_tinyint, c_tinyint_u, c_smallint, c_smallint_u,
        c_mediumint, c_mediumint_u, c_int, c_int_u,
        c_bigint, c_bigint_u,
        c_float, c_double, c_decimal_10_2, c_decimal_18_4, c_decimal_int,
        c_boolean, c_varchar, c_char, c_text,
        c_date, c_time, c_datetime, c_timestamp, c_year,
        c_json, c_enum, c_set_col,
        c_nullable_int, c_nullable_vc
      ) VALUES (
        127, 0, 32767, 0,
        8388607, 0, 2147483647, 0,
        -9223372036854775807, 0,
        -0.001, 0.0, 0.01, 0.0001, 0,
        FALSE, 'TestStr', 'X', 'another text',
        '1970-01-01', '00:00:00',
        '1970-01-01 00:00:00', '1970-01-02 00:00:00',
        1970,
        NULL,
        'small', 'exec',
        999, 'not null'
      )
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
        await executeSQL(`DROP TABLE IF EXISTS ${MY_TABLE}`);
      }
    } catch { /* best-effort */ }
    try {
      const handles = await browser.getWindowHandles();
      if (handles.length > 1) {
        await closeExtraWindows(mainWindow);
      }
      await browser.switchToWindow(mainWindow);
    } catch { /* ignore */ }
  });

  // ── Integer types ──

  it('TINYINT 应正确显示有符号和无符号边界值 (MY-TYPE-001)', async () => {
    await browseTableAndExpect(MY_TABLE, ['-128', '127', '255']);
  });

  it('SMALLINT 应正确显示有符号和无符号边界值 (MY-TYPE-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('-32768');
    expect(body).toContain('32767');
    expect(body).toContain('65535');
  });

  it('MEDIUMINT 应正确显示有符号和无符号边界值 (MY-TYPE-003)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('-8388608');
    expect(body).toContain('8388607');
    expect(body).toContain('16777215');
  });

  it('INT 应正确显示有符号和无符号边界值 (MY-TYPE-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('-2147483648');
    expect(body).toContain('2147483647');
    expect(body).toContain('4294967295');
  });

  it('BIGINT 有符号应正确显示边界值 (MY-TYPE-005)', async () => {
    const body = await $('body').getText();
    // BIGINT max exceeds JS MAX_SAFE_INTEGER, backend sends as string
    expect(body).toContain('9223372036854775807');
    expect(body).toContain('-9223372036854775807');
  });

  it('BIGINT UNSIGNED 应正确显示最大值 (MY-TYPE-006)', async () => {
    const body = await $('body').getText();
    // u64 max exceeds JS MAX_SAFE_INTEGER, backend sends as string
    expect(body).toContain('18446744073709551615');
  });

  // ── Floating-point types ──

  it('FLOAT 应正确显示浮点数 (MY-TYPE-007)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('3.14');
  });

  it('DOUBLE 应正确显示高精度浮点数 (MY-TYPE-008)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('2.71828');
  });

  // ── Decimal types ──

  it('DECIMAL(10,2) 应正确显示定点数 (MY-TYPE-009)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('12345678.99');
  });

  it('DECIMAL(18,4) 应正确显示高精度定点数 (MY-TYPE-010)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('9876543210.1234');
  });

  it('DECIMAL(10,0) 整数精度应正确显示 (MY-TYPE-011)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('42');
  });

  // ── Boolean ──

  it('BOOLEAN 应正确显示 (MY-TYPE-012)', async () => {
    const body = await $('body').getText();
    // MySQL BOOLEAN is TINYINT(1), displayed as integer 1/0
    expect(body).toContain('1');
    expect(body).toContain('0');
  });

  // ── String types ──

  it('VARCHAR 应正确显示文本 (MY-TYPE-013)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('Hello DataZen');
  });

  it('CHAR 应正确显示固定长度字符串 (MY-TYPE-013)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('ABCDEFGHIJ');
  });

  it('TEXT 应正确显示长文本 (MY-TYPE-013)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('Long text content for testing');
  });

  // ── Date/Time types ──

  it('DATE 应正确显示日期 (MY-TYPE-014)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('2026-04-16');
    expect(body).toContain('1970-01-01');
  });

  it('DATETIME/TIMESTAMP 应正确显示时间 (MY-TYPE-015)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('14:30');
  });

  it('YEAR 应正确显示年份 (MY-TYPE-016)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('2026');
    expect(body).toContain('1970');
  });

  // ── JSON ──

  it('JSON 应正确显示 JSON 内容 (MY-TYPE-017)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('key');
    expect(body).toContain('value');
  });

  // ── ENUM and SET ──

  it('ENUM 应正确显示枚举值 (MY-TYPE-018)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('medium');
    expect(body).toContain('small');
  });

  it('SET 应正确显示集合值 (MY-TYPE-018)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('read,write');
    expect(body).toContain('exec');
  });

  // ── NULL values ──

  it('NULL 值应正确显示为 NULL (MY-TYPE-019)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('NULL');
  });

  it('非 NULL 可空列应正确显示值 (MY-TYPE-019)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('999');
    expect(body).toContain('not null');
  });

  // ── Query-based verification ──

  it('SELECT 查询应正确返回 MySQL 整数类型 (MY-TYPE-Q01)', async () => {
    await queryAndExpect(
      `SELECT CAST(127 AS SIGNED) AS si, CAST(2147483647 AS SIGNED) AS i, CAST(9223372036854775807 AS SIGNED) AS bi`,
      ['127', '2147483647', '9223372036854775807'],
    );
  });

  it('SELECT 查询应正确返回 MySQL DECIMAL (MY-TYPE-Q02)', async () => {
    await queryAndExpect(
      `SELECT CAST(99999.9999 AS DECIMAL(10,4)) AS d`,
      ['99999.9999'],
    );
  });

  it('SELECT 查询应正确返回 MySQL 日期时间 (MY-TYPE-Q03)', async () => {
    await queryAndExpect(
      `SELECT CAST('2026-04-16' AS DATE) AS d, CAST('14:30:59' AS TIME) AS t`,
      ['2026-04-16', '14:30:59'],
    );
  });

  it('SELECT 查询应正确返回 MySQL JSON (MY-TYPE-Q04)', async () => {
    await queryAndExpect(
      `SELECT JSON_OBJECT('a', 1, 'b', 'hello') AS j`,
      ['hello'],
    );
  });

  // ── Structure tab type labels ──

  it('结构 tab 应正确显示 MySQL 列类型标签 (MY-TYPE-020)', async () => {
    await clickTableInSidebar(MY_TABLE);
    await browser.pause(1500);
    await switchSubTab('结构');
    await browser.waitUntil(
      async () => {
        const b = await $('body').getText();
        return b.includes('c_tinyint') || b.includes('字段名');
      },
      { timeout: 10000, timeoutMsg: '等待结构 tab 加载超时' },
    );

    const body = await $('body').getText();
    const lower = body.toLowerCase();
    expect(lower).toContain('tinyint');
    expect(lower).toContain('smallint');
    expect(lower).toContain('mediumint');
    expect(lower).toContain('bigint');
    expect(lower).toContain('decimal');
    expect(lower).toContain('float');
    expect(lower).toContain('double');
    expect(lower).toContain('varchar');
    expect(lower).toContain('date');
    expect(lower).toContain('json');
    expect(lower).toContain('enum');
    expect(lower).toContain('set');
  });
});
