/**
 * Editor Blog Screenshots — 编辑器系列 SEO 博客（#3–#8）截图生成器
 *
 * 每个 describe 块对应一篇独立博客文章，构造真实测试数据后
 * 驱动 DataZen 编辑器的单一功能并截图。
 *
 * 博客映射：
 *   #3  告别"全选再执行"：语句级运行           → describe('Blog03')
 *   #4  不离开编辑器看 Schema：悬浮卡片与跳转   → describe('Blog04')
 *   #5  写 JOIN 不用查文档：外键感知智能补全     → describe('Blog05')
 *   #6  INSERT 语句秒写完：Alt+Enter 智能意图    → describe('Blog06')
 *   #7  实时 SQL 诊断：写错即刻提示             → describe('Blog07')
 *   #8  智能粘贴与拖拽：Schema 树到编辑器        → describe('Blog08')
 *
 * 截图输出: e2e/screenshots/blog/
 *
 * 前置条件:
 *   1. pnpm tauri:build:webdriver（或 pnpm e2e:skip-build 前已构建）
 *   2. E2E_PG_HOST / E2E_PG_PORT / E2E_PG_USER / E2E_PG_PASSWORD / E2E_PG_DB 环境变量
 *      或本地默认 PostgreSQL (127.0.0.1:5432, postgres, 无密码, postgres 库)
 *
 * 运行:
 *   pnpm e2e:skip-build -- --spec e2e/specs/editor-blog-screenshots.ts
 */
import { browser, $ } from '@wdio/globals';
import fs from 'node:fs';
import path from 'node:path';
import url from 'node:url';
import {
  openConnectionsWorkspace,
  clickCardConnectButton,
  waitForConnectionToolbar,
  openQueryTab,
} from '../../e2e/helpers.js';

// ── paths ──────────────────────────────────────────────────────────────
const ROOT = path.resolve(path.dirname(url.fileURLToPath(import.meta.url)), '..', '..');
const OUT = path.join(ROOT, 'e2e', 'screenshots', 'blog');

// ── connection config ──────────────────────────────────────────────────
const PG = {
  host: process.env.E2E_PG_HOST || '127.0.0.1',
  port: Number(process.env.E2E_PG_PORT) || 5432,
  user: process.env.E2E_PG_USER || 'postgres',
  password: process.env.E2E_PG_PASSWORD || '',
  database: process.env.E2E_PG_DB || 'postgres',
};
const CONN_ID = 'blog_editor_screenshot';
const CONN_NAME = 'Blog-Screenshot-PG';

// ── helpers ────────────────────────────────────────────────────────────

async function invoke<T = unknown>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  return browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (c: string, a: string) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  ) as Promise<T>;
}

async function shot(name: string, settleMs = 1000) {
  await browser.pause(settleMs);
  fs.mkdirSync(OUT, { recursive: true });
  const filePath = path.join(OUT, name);
  await browser.saveScreenshot(filePath);
  const size = fs.statSync(filePath).size;
  console.log(`[shot] ${name} (${size} bytes)`);
}

async function setWindowSize(w = 2400, h = 1600) {
  await invoke('plugin:window|set_size', { size: { width: w, height: h } });
  await browser.pause(600);
}

async function setEditorContent(text: string) {
  const editor = $('.cm-editor .cm-content');
  await editor.waitForDisplayed({ timeout: 10000 });
  await editor.click();
  await browser.waitUntil(
    async () =>
      browser.execute(
        () =>
          document.activeElement?.closest('.cm-editor .cm-content') != null &&
          document.querySelector('[id^="dz-select-listbox-"]') == null,
      ),
    { timeout: 3000, timeoutMsg: 'editor focus did not settle' },
  );
  await browser.execute((t: string) => {
    const el = document.querySelector('.cm-editor .cm-content') as HTMLElement | null;
    if (!el) return;
    el.focus();
    const sel = window.getSelection();
    if (sel) {
      sel.selectAllChildren(el);
      sel.deleteFromDocument();
    }
    document.execCommand('insertText', false, t);
  }, text);
  await browser.pause(400);
}

async function executeSQL(sql: string) {
  const result = await invoke<{ __error?: string }>('execute_sql', {
    connectionId: CONN_ID,
    sql,
  });
  if (result && '__error' in result) {
    console.warn(`[warn] SQL failed: ${result.__error}\n  ${sql}`);
  }
  await browser.pause(300);
}

async function focusEditor() {
  const editor = $('.cm-editor .cm-content');
  await editor.waitForDisplayed({ timeout: 5000 });
  await editor.click();
  await browser.pause(200);
}

async function triggerAutocomplete() {
  await browser.keys(['Control', ' ']);
  await browser.pause(1000);
}

async function pressAltEnter() {
  await browser.keys(['Alt', 'Enter']);
  await browser.pause(1200);
}

async function moveCursorToOffset(offset: number) {
  await browser.execute((pos: number) => {
    const editor = document.querySelector('.cm-editor');
    const view = (editor as any)?.cmView?.view;
    if (!view) return;
    const text = view.state.doc.toString();
    const clamped = Math.max(0, Math.min(pos, text.length));
    view.dispatch({ selection: { anchor: clamped } });
  }, offset);
  await browser.pause(200);
}

async function selectSubstring(literal: string) {
  await browser.execute((needle: string) => {
    const editor = document.querySelector('.cm-editor');
    const view = (editor as any)?.cmView?.view;
    if (!view) return;
    const doc = view.state.doc.toString();
    const start = doc.indexOf(needle);
    if (start < 0) return;
    view.dispatch({ selection: { anchor: start, head: start + needle.length } });
  }, literal);
  await browser.pause(200);
}

/** Get the current editor text content. */
async function getEditorText(): Promise<string> {
  return browser.execute(() => {
    const el = document.querySelector('.cm-editor .cm-content');
    return el?.textContent || '';
  });
}

/** Right-click on a token containing the given text, to open context menu. */
async function rightClickToken(tokenText: string) {
  await browser.execute((needle: string) => {
    const editor = document.querySelector('.cm-editor .cm-content');
    if (!editor) return;
    const all = editor.querySelectorAll(
      '.cm-tag, .cm-variable, .cm-def, .cm-property, .cm-keyword, .cm-string',
    );
    for (const el of all) {
      if (el.textContent?.includes(needle)) {
        const rect = el.getBoundingClientRect();
        const cx = rect.left + rect.width / 2;
        const cy = rect.top + rect.height / 2;
        const eventInit: MouseEventInit = {
          bubbles: true,
          cancelable: true,
          clientX: cx,
          clientY: cy,
          button: 2,
        };
        el.dispatchEvent(new MouseEvent('contextmenu', eventInit));
        return;
      }
    }
  }, tokenText);
  await browser.pause(600);
}

// ── shared seed data ───────────────────────────────────────────────────

/**
 * Schema: 4 tables with FK relationships, JSONB, NUMERIC, DATE, BOOLEAN, TIMESTAMP.
 *
 *   _blog_dept (id, name, budget, created_at)
 *       ↑ FK
 *   _blog_employee (id, dept_id→dept, name, email, salary, hire_date, is_active, profile:JSONB)
 *       ↑ FK                                    ↑ FK
 *   _blog_project (id, title, status, budget)   _blog_emp_project (emp_id→employee, project_id→project, role, hours_logged)
 */
const SEED_SQL = [
  `CREATE TABLE IF NOT EXISTS _blog_dept (
    id SERIAL PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    budget NUMERIC(12,2) DEFAULT 0,
    created_at TIMESTAMP DEFAULT now()
  )`,
  `CREATE TABLE IF NOT EXISTS _blog_employee (
    id SERIAL PRIMARY KEY,
    dept_id INT NOT NULL REFERENCES _blog_dept(id),
    name VARCHAR(100) NOT NULL,
    email VARCHAR(200),
    salary NUMERIC(10,2),
    hire_date DATE,
    is_active BOOLEAN DEFAULT true,
    profile JSONB,
    dept_id_2 INT REFERENCES _blog_dept(id)
  )`,
  `CREATE TABLE IF NOT EXISTS _blog_project (
    id SERIAL PRIMARY KEY,
    title VARCHAR(200) NOT NULL,
    status VARCHAR(20) DEFAULT 'active',
    budget NUMERIC(10,2)
  )`,
  `CREATE TABLE IF NOT EXISTS _blog_emp_project (
    id SERIAL PRIMARY KEY,
    emp_id INT NOT NULL REFERENCES _blog_employee(id),
    project_id INT NOT NULL REFERENCES _blog_project(id),
    role VARCHAR(50),
    hours_logged INT DEFAULT 0
  )`,
  `INSERT INTO _blog_dept (name, budget) VALUES
    ('Engineering', 500000), ('Marketing', 200000), ('Sales', 300000)
    ON CONFLICT DO NOTHING`,
  `INSERT INTO _blog_employee (dept_id, name, email, salary, hire_date, profile) VALUES
    (1, 'Alice Chen', 'alice@example.com', 95000, '2022-03-15', '{"skills":["Rust","React"],"level":"senior"}'),
    (1, 'Bob Wang', 'bob@example.com', 82000, '2023-01-10', '{"skills":["TypeScript","SQL"],"level":"mid"}'),
    (2, 'Carol Li', 'carol@example.com', 78000, '2022-08-20', '{"skills":["SEO","Content"],"level":"senior"}'),
    (3, 'David Zhang', 'david@example.com', 88000, '2021-11-05', '{"skills":["CRM","Negotiation"],"level":"senior"}')
    ON CONFLICT DO NOTHING`,
  `INSERT INTO _blog_project (title, status, budget) VALUES
    ('DataZen v0.3', 'active', 120000),
    ('Marketing Campaign Q4', 'active', 50000),
    ('Sales Pipeline', 'planned', 30000)
    ON CONFLICT DO NOTHING`,
  `INSERT INTO _blog_emp_project (emp_id, project_id, role, hours_logged) VALUES
    (1, 1, 'Lead Developer', 320), (2, 1, 'Frontend Dev', 240),
    (3, 2, 'Content Lead', 160), (4, 3, 'Account Manager', 100)
    ON CONFLICT DO NOTHING`,
];

const CLEANUP_SQL = [
  `DROP TABLE IF EXISTS _blog_emp_project`,
  `DROP TABLE IF EXISTS _blog_project`,
  `DROP TABLE IF EXISTS _blog_employee`,
  `DROP TABLE IF EXISTS _blog_dept`,
];

// ── suite ──────────────────────────────────────────────────────────────

describe('Editor Blog Screenshots (editor-blog-screenshots.ts)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setWindowSize(2400, 1600);

    // Save connection
    await invoke('save_connection', {
      config: {
        id: CONN_ID,
        name: CONN_NAME,
        databaseType: 'postgresql',
        host: PG.host,
        port: PG.port,
        username: PG.user,
        password: PG.password,
        database: PG.database,
        group: 'SEO 博客',
        colorTag: 'blue',
        sslMode: 'disable',
        options: {},
      },
    });
    await browser.refresh();
    await browser.pause(2000);

    // Connect via UI using E2E helpers
    await openConnectionsWorkspace(mainWindow);
    await clickCardConnectButton(CONN_NAME);
    await waitForConnectionToolbar();
    await openQueryTab();
    await browser.pause(1500);

    // Seed test data
    for (const sql of SEED_SQL) {
      await executeSQL(sql);
    }
    await browser.pause(500);

    // Refresh schema cache so editor metadata is fresh
    await invoke('refresh_schema_cache', { connectionId: CONN_ID }).catch(() => {});
    await browser.pause(1000);
  });

  after(async () => {
    try {
      for (const sql of CLEANUP_SQL) {
        await executeSQL(sql);
      }
      await invoke('delete_connection', { id: CONN_ID });
    } catch {
      /* best-effort cleanup */
    }
    try {
      await browser.switchWindow(mainWindow);
    } catch {
      /* ignore */
    }
  });

  // ════════════════════════════════════════════════════════════════════
  //  #3 告别"全选再执行"：SQL 编辑器的语句级运行
  //
  //  痛点：编辑器里写了 10 条 SQL，只想跑其中一条，传统做法是
  //  全选再 Ctrl+Enter，容易选错范围。
  //
  //  卖点：Gutter 运行按钮 + 当前语句高亮 + 多语句边界检测。
  // ════════════════════════════════════════════════════════════════════
  describe('Blog03 — 语句级运行 (statement-level execution)', () => {
    // 截图 3a: 三条 SQL 语句，Gutter 运行按钮 + 语句高亮
    it('03a-statement-gutter', async () => {
      await openQueryTab();
      const sql = `-- 每条语句独立运行，无需全选
SELECT COUNT(*) AS total_employees FROM _blog_employee;

SELECT d.name, COUNT(e.id) AS headcount
FROM _blog_dept d
LEFT JOIN _blog_employee e ON e.dept_id = d.id
GROUP BY d.name;

SELECT p.title, p.status
FROM _blog_project p
WHERE p.status = 'active';`;

      await setEditorContent(sql);
      await browser.pause(600);
      await shot('03-statement-gutter.png');
    });

    // 截图 3b: 光标在第二条语句上，高亮当前语句边界
    it('03b-statement-highlight', async () => {
      await openQueryTab();
      const sql = `SELECT name, email FROM _blog_employee WHERE is_active = true;

SELECT d.name AS department, d.budget
FROM _blog_dept d
ORDER BY d.budget DESC;

SELECT ep.role, ep.hours_logged
FROM _blog_emp_project ep
WHERE ep.hours_logged > 100;`;

      await setEditorContent(sql);
      await browser.pause(400);

      // Move cursor to the second statement (around position of "SELECT d.name")
      const docText = await getEditorText();
      const secondStmt = docText.indexOf('SELECT d.name');
      if (secondStmt >= 0) {
        await moveCursorToOffset(secondStmt + 5);
      }
      await browser.pause(400);
      await shot('03-statement-highlight.png');
    });
  });

  // ════════════════════════════════════════════════════════════════════
  //  #4 不离开编辑器看 Schema：表结构悬浮卡片与快捷跳转
  //
  //  痛点：写 SQL 时频繁切换到 Schema 树查看表结构，打断心流。
  //
  //  卖点：悬停表名弹出列定义卡片 + Cmd/Ctrl+Click 跳转。
  // ════════════════════════════════════════════════════════════════════
  describe('Blog04 — 悬浮卡片与快捷跳转 (hover tooltip + navigation)', () => {
    // 截图 4a: 悬停表名 → 弹出列定义卡片（含外键关系）
    it('04a-hover-tooltip', async () => {
      await openQueryTab();
      await setEditorContent('SELECT * FROM _blog_employee');
      await browser.pause(400);

      // Hover over the table name to trigger tooltip
      await browser.execute(() => {
        const editor = document.querySelector('.cm-editor .cm-content');
        if (!editor) return;
        const tokens = editor.querySelectorAll('.cm-tag, .cm-variable, .cm-def, .cm-property');
        for (const token of tokens) {
          if (token.textContent?.includes('_blog_employee')) {
            const rect = token.getBoundingClientRect();
            const cx = rect.left + rect.width / 2;
            const cy = rect.top + rect.height / 2;
            token.dispatchEvent(
              new MouseEvent('mouseenter', { bubbles: true, clientX: cx, clientY: cy }),
            );
            editor.dispatchEvent(
              new MouseEvent('mousemove', { bubbles: true, clientX: cx, clientY: cy }),
            );
            break;
          }
        }
      });
      await browser.pause(1500);

      const hasTooltip = await browser.execute(() => {
        const tooltip = document.querySelector('.cm-tooltip');
        return tooltip !== null && (tooltip.textContent || '').length > 10;
      });
      console.log(`[info] hover tooltip visible: ${hasTooltip}`);

      await shot('04-hover-tooltip.png');
    });

    // 截图 4b: JOIN 场景中悬停两张表，对比列名
    it('04b-hover-multi-table', async () => {
      await openQueryTab();
      await setEditorContent(
        'SELECT e.name, e.salary, d.name AS department\nFROM _blog_employee e\nJOIN _blog_dept d ON e.dept_id = d.id',
      );
      await browser.pause(400);

      // Hover over _blog_dept to show its tooltip
      await browser.execute(() => {
        const editor = document.querySelector('.cm-editor .cm-content');
        if (!editor) return;
        const tokens = editor.querySelectorAll('.cm-tag, .cm-variable, .cm-def, .cm-property');
        for (const token of tokens) {
          if (token.textContent?.includes('_blog_dept')) {
            const rect = token.getBoundingClientRect();
            const cx = rect.left + rect.width / 2;
            const cy = rect.top + rect.height / 2;
            token.dispatchEvent(
              new MouseEvent('mouseenter', { bubbles: true, clientX: cx, clientY: cy }),
            );
            editor.dispatchEvent(
              new MouseEvent('mousemove', { bubbles: true, clientX: cx, clientY: cy }),
            );
            break;
          }
        }
      });
      await browser.pause(1500);
      await shot('04-hover-multi-table.png');
    });
  });

  // ════════════════════════════════════════════════════════════════════
  //  #5 写 JOIN 不用查文档：外键感知的智能 SQL 补全
  //
  //  痛点：写 JOIN 时需要手动查外键关系；列名也要一个一个敲。
  //
  //  卖点：FK JOIN 补全 + 列名补全 + 无前缀消歧 + 自动 FROM。
  // ════════════════════════════════════════════════════════════════════
  describe('Blog05 — 外键感知智能补全 (FK-aware completion)', () => {
    // 截图 5a: JOIN 后触发补全 → 推荐关联表（基于外键）
    it('05a-fk-join-completion', async () => {
      await openQueryTab();
      await setEditorContent('SELECT * FROM _blog_employee e JOIN ');
      await browser.pause(400);
      await focusEditor();
      await triggerAutocomplete();
      await browser.pause(1000);

      await shot('05-fk-join-completion.png');

      await browser.keys('Escape');
      await browser.pause(300);
    });

    // 截图 5b: 输入表别名 + 点号 → 列名补全
    it('05b-column-completion', async () => {
      await openQueryTab();
      await setEditorContent(
        'SELECT e. FROM _blog_employee e JOIN _blog_dept d ON e.dept_id = d.id',
      );
      await browser.pause(400);

      // Position cursor after "e."
      const docText = await getEditorText();
      const eDotIdx = docText.indexOf('e.');
      if (eDotIdx >= 0) {
        await moveCursorToOffset(eDotIdx + 2);
      }
      await browser.pause(200);
      await focusEditor();
      await triggerAutocomplete();
      await browser.pause(1000);

      await shot('05-column-completion.png');

      await browser.keys('Escape');
      await browser.pause(300);
    });

    // 截图 5c: 第二条 JOIN 触发补全 → 推荐 _blog_project（另一个 FK 路径）
    it('05c-multi-join-completion', async () => {
      await openQueryTab();
      await setEditorContent(
        'SELECT e.name, ep.role\nFROM _blog_employee e\nJOIN _blog_emp_project ep ON ep.emp_id = e.id\nJOIN ',
      );
      await browser.pause(400);

      // Move cursor after the last "JOIN "
      const docText = await getEditorText();
      const lastJoin = docText.lastIndexOf('JOIN ');
      if (lastJoin >= 0) {
        await moveCursorToOffset(lastJoin + 5);
      }
      await browser.pause(200);
      await focusEditor();
      await triggerAutocomplete();
      await browser.pause(1000);

      await shot('05-multi-join-completion.png');

      await browser.keys('Escape');
      await browser.pause(300);
    });
  });

  // ════════════════════════════════════════════════════════════════════
  //  #6 INSERT 语句秒写完：Alt+Enter 智能意图操作
  //
  //  痛点：写 INSERT 语句时手动敲每一列名和值，繁琐且易错；
  //  SELECT * 想展开为列列表要手动改。
  //
  //  卖点：星号展开 + INSERT 模板 + 限定符操作 + 子查询包裹。
  // ════════════════════════════════════════════════════════════════════
  describe('Blog06 — Alt+Enter 智能意图 (code intentions)', () => {
    // 截图 6a: SELECT * 上 Alt+Enter → 展开为列列表
    it('06a-intention-star-expand', async () => {
      await openQueryTab();
      await setEditorContent('SELECT * FROM _blog_employee');
      await browser.pause(400);

      await selectSubstring('*');
      await browser.pause(200);
      await pressAltEnter();
      await browser.pause(600);

      await shot('06-intention-star-expand.png');
    });

    // 截图 6b: INSERT INTO ... () 内 Alt+Enter → 展开完整模板
    it('06b-intention-insert-template', async () => {
      await openQueryTab();
      await setEditorContent('INSERT INTO _blog_employee () VALUES ');
      await browser.pause(400);

      // Position cursor inside the empty parentheses
      const docText = await getEditorText();
      const insertIdx = docText.indexOf('()');
      if (insertIdx >= 0) {
        await moveCursorToOffset(insertIdx + 1);
      }
      await browser.pause(200);
      await focusEditor();
      await pressAltEnter();
      await browser.pause(800);

      await shot('06-intention-insert-template.png');
    });

    // 截图 6c: INSERT INTO 表名后 Alt+Enter → 展开列名模板
    it('06c-intention-insert-columns', async () => {
      await openQueryTab();
      await setEditorContent('INSERT INTO _blog_dept');
      await browser.pause(400);

      // Position cursor at end of table name
      const docText = await getEditorText();
      const endIdx = docText.length;
      await moveCursorToOffset(endIdx);
      await browser.pause(200);
      await focusEditor();
      await pressAltEnter();
      await browser.pause(800);

      await shot('06-intention-insert-columns.png');
    });

    // 截图 6d: 函数签名提示（作为意图功能的补充展示）
    it('06d-signature-help', async () => {
      await openQueryTab();
      await setEditorContent('SELECT SUBSTRING(name, 1, 5) FROM _blog_employee');
      await browser.pause(400);

      // Move cursor inside function parentheses
      const docText = await getEditorText();
      const funcStart = docText.indexOf('SUBSTRING(');
      const firstComma = docText.indexOf(',', funcStart);
      if (funcStart >= 0 && firstComma >= 0) {
        await moveCursorToOffset(firstComma + 2);
      }
      await browser.pause(300);

      // Delete and retype to trigger signature help
      await browser.keys('Backspace');
      await browser.pause(200);
      await browser.keys('1');
      await browser.pause(1500);

      await shot('06-signature-help.png');
    });
  });

  // ════════════════════════════════════════════════════════════════════
  //  #7 实时 SQL 诊断：写错即刻提示，不用等执行报错
  //
  //  痛点：SQL 写错了，执行后才看到报错，来回调试浪费时间。
  //
  //  卖点：实时波浪线标记 + 未知表/列 + 类型不匹配 + 性能预警。
  // ════════════════════════════════════════════════════════════════════
  describe('Blog07 — 实时 SQL 诊断 (linter / diagnostics)', () => {
    // 截图 7a: 未知表名 + 未知列名 → 波浪线标记
    it('07a-linter-unknown-table-column', async () => {
      await openQueryTab();
      const sql = `SELECT nonexistent_column, name
FROM _blog_employee e
JOIN _nonexistent_table t ON e.id = t.emp_id
WHERE t.status = 'active';`;

      await setEditorContent(sql);
      await browser.pause(1500); // Linter needs time to analyze

      await shot('07-linter-unknown-table-column.png');
    });

    // 截图 7b: 类型不匹配 + 语法错误
    it('07b-linter-type-mismatch', async () => {
      await openQueryTab();
      const sql = `-- 类型提示：salary 是 NUMERIC，但和字符串比较
SELECT name, salary FROM _blog_employee WHERE salary > 'not_a_number';

-- 语法错误：缺少 FROM
SELECT name, email WHERE is_active = true;`;

      await setEditorContent(sql);
      await browser.pause(1500);

      await shot('07-linter-type-mismatch.png');
    });

    // 截图 7c: 危险操作预警（UPDATE/DELETE 无 WHERE）
    it('07c-linter-dangerous-operation', async () => {
      await openQueryTab();
      const sql = `-- ⚠ 缺少 WHERE 的 UPDATE 可能影响全表
UPDATE _blog_employee SET salary = salary * 1.1;

-- ⚠ 缺少 WHERE 的 DELETE 可能清空数据
DELETE FROM _blog_employee;`;

      await setEditorContent(sql);
      await browser.pause(1500);

      await shot('07-linter-dangerous-operation.png');
    });
  });

  // ════════════════════════════════════════════════════════════════════
  //  #8 智能粘贴与拖拽：从 Schema 树到编辑器的无缝衔接
  //
  //  痛点：想把多个列名粘贴为 IN (...) 子句需要手动加引号和逗号；
  //  从 Schema 树复制列名到编辑器格式不对。
  //
  //  卖点：Paste-as-IN + Drop Caret + 参数绑定面板。
  // ════════════════════════════════════════════════════════════════════
  describe('Blog08 — 智能粘贴与拖拽 (smart paste & drop)', () => {
    // 截图 8a: 右键菜单 Paste-as-IN（在已有值列表上右键）
    it('08a-paste-as-in', async () => {
      await openQueryTab();
      // Write a query with comma-separated values that could be pasted
      await setEditorContent(
        "SELECT name, email FROM _blog_employee WHERE name IN ('Alice', 'Bob', 'Carol')",
      );
      await browser.pause(400);

      // Right-click on the IN clause values to show context menu
      await rightClickToken("'Alice'");
      await browser.pause(600);

      await shot('08-paste-as-in.png');

      // Close context menu
      await browser.keys('Escape');
      await browser.pause(300);
    });

    // 截图 8b: 参数绑定面板（$1 $2 $3 占位符）
    it('08b-bind-params', async () => {
      await openQueryTab();
      const sql = `SELECT name, salary, email
FROM _blog_employee
WHERE dept_id = $1
  AND salary > $2
  AND is_active = $3;`;

      await setEditorContent(sql);
      await browser.pause(600);

      // Wait for bind param panel to appear
      await browser.pause(1000);

      await shot('08-bind-params.png');
    });

    // 截图 8c: 命名参数（:name 格式）绑定面板
    it('08c-bind-params-named', async () => {
      await openQueryTab();
      const sql = `SELECT name, email, salary
FROM _blog_employee
WHERE name = :emp_name
  AND salary > :min_salary;`;

      await setEditorContent(sql);
      await browser.pause(1500);

      await shot('08-bind-params-named.png');
    });

    // 截图 8d: SQL Guard 安全拦截（与 #8 交叉引流，展示安全性）
    it('08d-sql-guard-danger', async () => {
      await openQueryTab();
      await setEditorContent('DELETE FROM _blog_employee WHERE 1=1;');
      await browser.pause(400);

      const execBtn = await $('[data-testid="editor-execute-button"]');
      await execBtn.waitForDisplayed({ timeout: 5000 });
      await execBtn.click();
      await browser.pause(1500);

      await shot('08-sql-guard-danger.png');
    });
  });
});
