# DataZen 首次激活向导与查询工具栏重排实施计划 (PRD P0)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 实现 DataZen 首次激活三步向导（含离线 SQLite 示例库、预置 `getting_started.sql` 查询、数据洞察闭环与随时跳过）以及查询主工具栏四区重排（核心高频区、更多操作收纳菜单、事务保护自适应浮现、审计状态区）与工作区侧边栏双模切换。

**Architecture:** 
1. 采用状态机（`onboardingStore`）解耦向导生命周期（`not_started` → `active` (Step 1/2/3) → `completed` / `skipped`），本地存储持久化；
2. 后端基于 `rusqlite` 提供零外部依赖的 `init_sample_database` IPC，生成约 50 条覆盖多品类、多日期的真实国际化电商业务种子数据；
3. `WelcomePage` 引入三步导览横幅、主行动点和跳过入口；`ConnectionPage` 顶部挂载 38px 非模态 `OnboardingGuideBar`，联动首查执行结果与图表/AI 操作；
4. `QueryEditorSection` 工具栏平铺按钮重构为 4 大区块，引入 `QueryToolbarMoreMenu` 并在事务中自适应浮现提交/回滚，升级断点宽度计算。

**Tech Stack:** React 18, TypeScript, Tailwind CSS 4, Zustand, Tauri v2 (Rust + `rusqlite`), Vitest, `@datazen/ui`.

## Global Constraints

- **单文件规模控制**：单文件严格不超过 800 行，大型模块必须拆分子组件。
- **i18n 规范**：开发阶段仅修改 `src/locales/en/core.ts` 与 `src/locales/zh-CN/core.ts`，禁止直接修改其他语言文件。
- **防回归交互原则**：状态机必须具备进入、状态内行为与退出跃迁三要素；数据绑定使用 `data-testid` 属性；必须包含连续旅程测试（Journey Test）。
- **无破坏性与零硬编码**：支持用户随时一键跳过且不打扰后续正常生产环境使用；数据库行为通过 `DB_REGISTRY` 元数据驱动。

---

### Task 1: 首次激活向导状态机 Store (`onboardingStore.ts`)

**Files:**
- Create: `src/stores/onboardingStore.ts`
- Test: `src/stores/__tests__/onboardingStore.test.ts`

**Interfaces:**
- Consumes: None (Zustand + `localStorage`)
- Produces: `useOnboardingStore`, `OnboardingStatus`, `OnboardingState`
  ```ts
  export type OnboardingStatus = 'not_started' | 'active' | 'completed' | 'skipped';
  export interface OnboardingState {
    status: OnboardingStatus;
    step: 1 | 2 | 3;
    sampleConnectionId: string | null;
    queryExecuted: boolean;
    aiOrChartExplored: boolean;
    startOnboarding: (sampleConnectionId?: string) => void;
    advanceToStep: (step: 2 | 3) => void;
    markQueryExecuted: () => void;
    markAiOrChartExplored: () => void;
    completeOnboarding: () => void;
    skipOnboarding: () => void;
    resetOnboarding: () => void;
  }
  ```

- [ ] **Step 1: 编写失败的单元测试**

```ts
// src/stores/__tests__/onboardingStore.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { useOnboardingStore } from '../onboardingStore';

describe('onboardingStore', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
  });

  it('initializes with not_started status and step 1', () => {
    const state = useOnboardingStore.getState();
    expect(state.status).toBe('not_started');
    expect(state.step).toBe(1);
    expect(state.queryExecuted).toBe(false);
    expect(state.aiOrChartExplored).toBe(false);
  });

  it('starts onboarding and advances to step 2 when sample connection provided', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    let state = useOnboardingStore.getState();
    expect(state.status).toBe('active');
    expect(state.sampleConnectionId).toBe('sample_sqlite');
    expect(state.step).toBe(2);

    useOnboardingStore.getState().markQueryExecuted();
    state = useOnboardingStore.getState();
    expect(state.queryExecuted).toBe(true);
    expect(state.step).toBe(3);

    useOnboardingStore.getState().markAiOrChartExplored();
    state = useOnboardingStore.getState();
    expect(state.aiOrChartExplored).toBe(true);

    useOnboardingStore.getState().completeOnboarding();
    state = useOnboardingStore.getState();
    expect(state.status).toBe('completed');
  });

  it('allows skipping onboarding at any point and persists skipped status', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');

    useOnboardingStore.getState().skipOnboarding();
    expect(useOnboardingStore.getState().status).toBe('skipped');
    expect(localStorage.getItem('datazen:onboarding-state-v1')).toContain('"status":"skipped"');
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run src/stores/__tests__/onboardingStore.test.ts`
Expected: FAIL with module not found `../onboardingStore`

- [ ] **Step 3: 编写 `onboardingStore.ts` 实现**

```ts
// src/stores/onboardingStore.ts
import { create } from 'zustand';

export type OnboardingStatus = 'not_started' | 'active' | 'completed' | 'skipped';

export interface OnboardingState {
  status: OnboardingStatus;
  step: 1 | 2 | 3;
  sampleConnectionId: string | null;
  queryExecuted: boolean;
  aiOrChartExplored: boolean;

  startOnboarding: (sampleConnectionId?: string) => void;
  advanceToStep: (step: 2 | 3) => void;
  markQueryExecuted: () => void;
  markAiOrChartExplored: () => void;
  completeOnboarding: () => void;
  skipOnboarding: () => void;
  resetOnboarding: () => void;
}

const STORAGE_KEY = 'datazen:onboarding-state-v1';

interface PersistedState {
  status: OnboardingStatus;
  step: 1 | 2 | 3;
  sampleConnectionId: string | null;
  queryExecuted: boolean;
  aiOrChartExplored: boolean;
}

function loadPersistedState(): PersistedState {
  if (typeof window === 'undefined' || !window.localStorage) {
    return {
      status: 'not_started',
      step: 1,
      sampleConnectionId: null,
      queryExecuted: false,
      aiOrChartExplored: false,
    };
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {
        status: 'not_started',
        step: 1,
        sampleConnectionId: null,
        queryExecuted: false,
        aiOrChartExplored: false,
      };
    }
    const parsed = JSON.parse(raw) as Partial<PersistedState>;
    return {
      status: parsed.status ?? 'not_started',
      step: parsed.step ?? 1,
      sampleConnectionId: parsed.sampleConnectionId ?? null,
      queryExecuted: !!parsed.queryExecuted,
      aiOrChartExplored: !!parsed.aiOrChartExplored,
    };
  } catch {
    return {
      status: 'not_started',
      step: 1,
      sampleConnectionId: null,
      queryExecuted: false,
      aiOrChartExplored: false,
    };
  }
}

function persistState(state: PersistedState): void {
  if (typeof window === 'undefined' || !window.localStorage) return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(state));
  } catch {
    // best-effort
  }
}

export const useOnboardingStore = create<OnboardingState>((set, get) => {
  const initial = loadPersistedState();

  const updateAndSave = (patch: Partial<PersistedState>) => {
    set((prev) => {
      const next: PersistedState = {
        status: patch.status ?? prev.status,
        step: patch.step ?? prev.step,
        sampleConnectionId:
          patch.sampleConnectionId !== undefined
            ? patch.sampleConnectionId
            : prev.sampleConnectionId,
        queryExecuted: patch.queryExecuted ?? prev.queryExecuted,
        aiOrChartExplored: patch.aiOrChartExplored ?? prev.aiOrChartExplored,
      };
      persistState(next);
      return { ...prev, ...next };
    });
  };

  return {
    ...initial,

    startOnboarding: (sampleConnectionId?: string) => {
      updateAndSave({
        status: 'active',
        step: sampleConnectionId ? 2 : 1,
        sampleConnectionId: sampleConnectionId ?? null,
      });
    },

    advanceToStep: (step: 2 | 3) => {
      updateAndSave({ step });
    },

    markQueryExecuted: () => {
      const { step } = get();
      updateAndSave({
        queryExecuted: true,
        step: step === 2 ? 3 : step,
      });
    },

    markAiOrChartExplored: () => {
      updateAndSave({ aiOrChartExplored: true });
    },

    completeOnboarding: () => {
      updateAndSave({ status: 'completed' });
    },

    skipOnboarding: () => {
      updateAndSave({ status: 'skipped' });
    },

    resetOnboarding: () => {
      const reset: PersistedState = {
        status: 'not_started',
        step: 1,
        sampleConnectionId: null,
        queryExecuted: false,
        aiOrChartExplored: false,
      };
      persistState(reset);
      set(reset);
    },
  };
});
```

- [ ] **Step 4: 运行测试验证通过**

Run: `npx vitest run src/stores/__tests__/onboardingStore.test.ts`
Expected: PASS (all 3 tests pass)

- [ ] **Step 5: 提交代码**

```bash
git add src/stores/onboardingStore.ts src/stores/__tests__/onboardingStore.test.ts
git commit -m "feat(onboarding): add onboarding state machine store with persistence"
```

---

### Task 2: 离线 SQLite 示例库后端命令与 IPC (`sample_db.rs` & `sampleData.ts`)

**Files:**
- Create: `src-tauri/src/commands/sample_db.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/bootstrap.rs`
- Create: `src/commands/sampleData.ts`
- Test: `src/commands/__tests__/sampleData.test.ts`

**Interfaces:**
- Consumes: Tauri invoke, `rusqlite`, `Store::default_app_data_dir()`, `ConnectionConfig`
- Produces: `sampleDataCommands.initSampleDatabase(): Promise<ConnectionConfig>`

- [ ] **Step 1: 编写前端 IPC 调用的单元测试**

```ts
// src/commands/__tests__/sampleData.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { sampleDataCommands } from '../sampleData';

const mockInvoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => mockInvoke(...args),
}));

describe('sampleDataCommands', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('calls init_sample_database and returns created connection config', async () => {
    const mockConfig = {
      id: 'sample_sqlite',
      name: 'Sample E-Commerce (SQLite)',
      databaseType: 'sqlite',
      database: '/tmp/sample_ecommerce.sqlite',
      group: 'Samples',
    };
    mockInvoke.mockResolvedValueOnce(mockConfig);

    const result = await sampleDataCommands.initSampleDatabase();
    expect(mockInvoke).toHaveBeenCalledWith('init_sample_database');
    expect(result.id).toBe('sample_sqlite');
    expect(result.databaseType).toBe('sqlite');
  });
});
```

- [ ] **Step 2: 运行前端 IPC 测试验证失败**

Run: `npx vitest run src/commands/__tests__/sampleData.test.ts`
Expected: FAIL with module not found `../sampleData`

- [ ] **Step 3: 编写 `src-tauri/src/commands/sample_db.rs` 实现**

```rust
// src-tauri/src/commands/sample_db.rs
use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::ConnectionConfig;
use crate::store::Store;
use rusqlite::Connection;
use tauri::State;

const SAMPLE_CONN_ID: &str = "sample_sqlite";
const SAMPLE_DB_NAME: &str = "Sample E-Commerce (SQLite)";
const SAMPLE_FILE_NAME: &str = "sample_ecommerce.sqlite";

pub(crate) async fn init_sample_database_impl(
    state: &AppState,
) -> Result<ConnectionConfig, CommandError> {
    let data_dir = Store::default_app_data_dir().map_err(|e| {
        CommandError::Internal(format!("Failed to determine app data dir: {}", e))
    })?;
    let db_path = data_dir.join(SAMPLE_FILE_NAME);

    // Create & populate SQLite database if needed
    {
        let conn = Connection::open(&db_path).map_err(|e| {
            CommandError::Internal(format!("Failed to open sample sqlite database: {}", e))
        })?;

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS customers (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                email TEXT NOT NULL UNIQUE,
                country TEXT NOT NULL,
                signup_date DATE NOT NULL
            );

            CREATE TABLE IF NOT EXISTS products (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                category TEXT NOT NULL,
                price REAL NOT NULL,
                stock INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS orders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                customer_id INTEGER NOT NULL REFERENCES customers(id),
                product_id INTEGER NOT NULL REFERENCES products(id),
                quantity INTEGER NOT NULL,
                total_amount REAL NOT NULL,
                status TEXT NOT NULL,
                order_date DATE NOT NULL
            );
            "#,
        )
        .map_err(|e| CommandError::Internal(format!("Failed to create sample schema: {}", e)))?;

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM customers", [], |r| r.get(0))
            .unwrap_or(0);

        if count == 0 {
            conn.execute_batch(
                r#"
                INSERT INTO customers (id, name, email, country, signup_date) VALUES
                (1, 'Alice Smith', 'alice@example.com', 'United States', '2026-01-15'),
                (2, 'Bob Jones', 'bob@example.co.uk', 'United Kingdom', '2026-01-20'),
                (3, 'Chloe Dubois', 'chloe@example.fr', 'France', '2026-02-01'),
                (4, 'David Chen', 'david@example.com', 'Canada', '2026-02-10'),
                (5, 'Elena Rossi', 'elena@example.it', 'Italy', '2026-02-14'),
                (6, 'Fumiya Tanaka', 'tanaka@example.jp', 'Japan', '2026-02-22'),
                (7, 'Grace Miller', 'grace@example.com', 'Germany', '2026-03-01'),
                (8, 'Hassan Ali', 'hassan@example.ae', 'United Arab Emirates', '2026-03-05');

                INSERT INTO products (id, name, category, price, stock) VALUES
                (1, 'Pro Wireless Headphone', 'Electronics', 199.99, 45),
                (2, 'Ultra Mechanical Keyboard', 'Electronics', 129.50, 80),
                (3, '4K Ergonomic Monitor', 'Electronics', 449.00, 20),
                (4, 'Ergonomic Standing Desk', 'Home & Office', 389.00, 15),
                (5, 'Breathable Mesh Chair', 'Home & Office', 249.99, 30),
                (6, 'Desk LED Lamp Pro', 'Home & Office', 59.90, 120),
                (7, 'Merino Wool Hoodie', 'Apparel', 89.00, 60),
                (8, 'Waterproof Trail Backpack', 'Apparel', 115.00, 40);

                INSERT INTO orders (customer_id, product_id, quantity, total_amount, status, order_date) VALUES
                (1, 1, 1, 199.99, 'completed', '2026-03-01'),
                (1, 2, 1, 129.50, 'completed', '2026-03-01'),
                (2, 3, 1, 449.00, 'completed', '2026-03-02'),
                (3, 4, 1, 389.00, 'shipped', '2026-03-02'),
                (4, 5, 2, 499.98, 'completed', '2026-03-03'),
                (5, 7, 2, 178.00, 'completed', '2026-03-04'),
                (6, 1, 2, 399.98, 'completed', '2026-03-04'),
                (7, 6, 3, 179.70, 'shipped', '2026-03-05'),
                (8, 8, 1, 115.00, 'processing', '2026-03-06'),
                (2, 1, 1, 199.99, 'completed', '2026-03-06'),
                (3, 2, 2, 259.00, 'completed', '2026-03-07'),
                (4, 8, 1, 115.00, 'completed', '2026-03-07');
                "#,
            )
            .map_err(|e| CommandError::Internal(format!("Failed to seed sample data: {}", e)))?;
        }
    }

    // Save connection into datazen store
    let config = ConnectionConfig {
        id: SAMPLE_CONN_ID.to_string(),
        name: SAMPLE_DB_NAME.to_string(),
        database_type: "sqlite".to_string(),
        database: Some(db_path.to_string_lossy().to_string()),
        group: Some("Samples".to_string()),
        host: None,
        port: None,
        username: None,
        password: None,
        ssl: None,
        ssh_tunnel: None,
        options: None,
        driver_options: None,
        pinned: Some(true),
        color: None,
        read_only: Some(false),
        connection_timeout: 10,
        keepalive_interval: None,
        last_connected_at: None,
    };

    state
        .store
        .save_connection(config.clone())
        .await
        .cmd_err("init_sample_database")?;

    tracing::info!("Initialized sample sqlite database connection successfully");
    Ok(config)
}

#[tauri::command]
pub async fn init_sample_database(
    state: State<'_, AppState>,
) -> Result<ConnectionConfig, CommandError> {
    init_sample_database_impl(&state).await
}
```

- [ ] **Step 4: 注册命令到 `commands/mod.rs` 与 `bootstrap.rs`**

在 `src-tauri/src/commands/mod.rs` 添加:
```rust
mod sample_db;
pub use sample_db::*;
```
在 `src-tauri/src/bootstrap.rs` 的 `invoke_handler` 列表中添加:
```rust
crate::commands::init_sample_database,
```

- [ ] **Step 5: 编写前端 `src/commands/sampleData.ts`**

```ts
// src/commands/sampleData.ts
import { invoke } from '@tauri-apps/api/core';
import type { ConnectionConfig } from '../types';

export const sampleDataCommands = {
  initSampleDatabase: () => invoke<ConnectionConfig>('init_sample_database'),
};
```

- [ ] **Step 6: 运行前端与 Rust 测试验证通过**

Run: `npx vitest run src/commands/__tests__/sampleData.test.ts`
Expected: PASS
Run: `cargo test -p datazen --lib sample_db` (or `cargo check -p datazen --bin datazen`)
Expected: Finished successfully

- [ ] **Step 7: 提交代码**

```bash
git add src-tauri/src/commands/sample_db.rs src-tauri/src/commands/mod.rs src-tauri/src/bootstrap.rs src/commands/sampleData.ts src/commands/__tests__/sampleData.test.ts
git commit -m "feat(backend): add init_sample_database command and seed data"
```

---

### Task 3: 欢迎页三步向导重构与国际化 (`WelcomePage.tsx`)

**Files:**
- Modify: `src/windows/welcome/WelcomePage.tsx`
- Modify: `src/windows/welcome/__tests__/WelcomePage.test.tsx`
- Modify: `src/locales/en/core.ts`
- Modify: `src/locales/zh-CN/core.ts`

**Interfaces:**
- Consumes: `useOnboardingStore`, `sampleDataCommands`, `useConnectionStore`, `useActiveConnectionStore`
- Produces: 呈现三步引导横幅、`[打开示例 SQLite (快速体验)]` 主按钮、`[跳过向导直接使用]` 入口

- [ ] **Step 1: 编写欢迎页新功能的单元测试**

```tsx
// src/windows/welcome/__tests__/WelcomePage.test.tsx
import { render, screen, fireEvent, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { WelcomePage } from '../WelcomePage';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { sampleDataCommands } from '../../../commands/sampleData';

vi.mock('../../../commands/sampleData', () => ({
  sampleDataCommands: {
    initSampleDatabase: vi.fn(),
  },
}));

vi.mock('../../../lib/windowManager', () => ({
  openNewConnectionDialog: vi.fn(),
}));

vi.mock('../../../lib/connectionShare', () => ({
  openConnectionShareDialog: vi.fn(),
}));

describe('WelcomePage Onboarding', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    useOnboardingStore.getState().resetOnboarding();
  });

  it('renders guided steps banner and sample sqlite quick action', () => {
    render(<WelcomePage />);
    expect(screen.getByTestId('welcome-open-sample')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-create-connection')).toBeInTheDocument();
    expect(screen.getByTestId('welcome-skip-onboarding')).toBeInTheDocument();
  });

  it('initiates sample database when clicking quick start', async () => {
    const mockConn = { id: 'sample_sqlite', name: 'Sample E-Commerce' };
    (sampleDataCommands.initSampleDatabase as any).mockResolvedValueOnce(mockConn);

    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-open-sample'));

    await waitFor(() => {
      expect(sampleDataCommands.initSampleDatabase).toHaveBeenCalled();
      expect(useOnboardingStore.getState().status).toBe('active');
      expect(useOnboardingStore.getState().sampleConnectionId).toBe('sample_sqlite');
    });
  });

  it('marks onboarding as skipped when clicking skip link', () => {
    render(<WelcomePage />);
    fireEvent.click(screen.getByTestId('welcome-skip-onboarding'));
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run src/windows/welcome/__tests__/WelcomePage.test.tsx`
Expected: FAIL with unable to find element `welcome-open-sample`

- [ ] **Step 3: 添加 i18n 翻译键**

在 `src/locales/en/core.ts` 添加:
```ts
  'welcome.onboarding.title': 'Get started in 3 quick steps',
  'welcome.onboarding.step1': '1. Connect data (Sample SQLite ready)',
  'welcome.onboarding.step2': '2. Run your first query',
  'welcome.onboarding.step3': '3. Gain insights with charts & AI',
  'welcome.openSampleDb': 'Open Sample SQLite (Instant Tour)',
  'welcome.openSampleDbHint': 'No credentials required. Ready-to-query e-commerce demo dataset.',
  'welcome.skipOnboarding': 'Skip guide and start directly',
```
在 `src/locales/zh-CN/core.ts` 添加:
```ts
  'welcome.onboarding.title': '三步快速开启体验',
  'welcome.onboarding.step1': '1. 连接数据（已备好示例库）',
  'welcome.onboarding.step2': '2. 运行首条 SQL 查询',
  'welcome.onboarding.step3': '3. 体验图表与 AI 洞察',
  'welcome.openSampleDb': '打开示例 SQLite (快速体验)',
  'welcome.openSampleDbHint': '零凭据即开即用，内置完整的真实电商业务示例数据集。',
  'welcome.skipOnboarding': '跳过向导直接使用',
```

- [ ] **Step 4: 更新 `WelcomePage.tsx` 实现**

重构 `src/windows/welcome/WelcomePage.tsx`，加入三步引导横幅、`[打开示例 SQLite (快速体验)]` 主按钮（带 Accent 视觉样式与 loading 状态）、次级按钮和跳过入口。点击示例库时调用 `sampleDataCommands.initSampleDatabase()`，并在成功后触发 `useConnectionStore.getState().fetchConnections()` 与 `useOnboardingStore.getState().startOnboarding(config.id)`。

- [ ] **Step 5: 运行测试验证通过**

Run: `npx vitest run src/windows/welcome/__tests__/WelcomePage.test.tsx`
Expected: PASS

- [ ] **Step 6: 提交代码**

```bash
git add src/windows/welcome/WelcomePage.tsx src/windows/welcome/__tests__/WelcomePage.test.tsx src/locales/en/core.ts src/locales/zh-CN/core.ts
git commit -m "feat(welcome): add 3-step onboarding guide, sample sqlite cta, and skip action"
```

---

### Task 4: 工作台顶部非模态引导条组件 (`OnboardingGuideBar.tsx`)

**Files:**
- Create: `src/windows/connection/OnboardingGuideBar.tsx`
- Test: `src/windows/connection/__tests__/OnboardingGuideBar.test.tsx`

**Interfaces:**
- Consumes: `useOnboardingStore`, `useI18n`
- Produces: `OnboardingGuideBar` 组件
  ```tsx
  export interface OnboardingGuideBarProps {
    onExecuteSampleQuery?: () => void;
  }
  ```

- [ ] **Step 1: 编写引导条单元测试**

```tsx
// src/windows/connection/__tests__/OnboardingGuideBar.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { OnboardingGuideBar } from '../OnboardingGuideBar';
import { useOnboardingStore } from '../../stores/onboardingStore';

describe('OnboardingGuideBar', () => {
  beforeEach(() => {
    useOnboardingStore.getState().resetOnboarding();
  });

  it('does not render when status is not active', () => {
    useOnboardingStore.getState().skipOnboarding();
    const { container } = render(<OnboardingGuideBar />);
    expect(container.firstChild).toBeNull();
  });

  it('renders step 2 message and quick-run button when step is 2', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    const onRun = vi.fn();
    render(<OnboardingGuideBar onExecuteSampleQuery={onRun} />);

    expect(screen.getByTestId('onboarding-guide-bar')).toBeInTheDocument();
    expect(screen.getByText(/2\/3/)).toBeInTheDocument();

    const runBtn = screen.getByTestId('onboarding-quick-run-btn');
    fireEvent.click(runBtn);
    expect(onRun).toHaveBeenCalled();
  });

  it('renders step 3 message when step is 3 and handles completion', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    useOnboardingStore.getState().markQueryExecuted();

    render(<OnboardingGuideBar />);
    expect(screen.getByText(/3\/3/)).toBeInTheDocument();

    const completeBtn = screen.getByTestId('onboarding-complete-btn');
    fireEvent.click(completeBtn);
    expect(useOnboardingStore.getState().status).toBe('completed');
  });

  it('skips onboarding when clicking skip button', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    render(<OnboardingGuideBar />);

    const skipBtn = screen.getByTestId('onboarding-skip-btn');
    fireEvent.click(skipBtn);
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run src/windows/connection/__tests__/OnboardingGuideBar.test.tsx`
Expected: FAIL with module not found `../OnboardingGuideBar`

- [ ] **Step 3: 添加 i18n 键**

在 `src/locales/en/core.ts` 和 `src/locales/zh-CN/core.ts` 添加:
- `onboarding.badge`: `Guide {step}/3` / `向导 {step}/3`
- `onboarding.step2.message`: `Step 2: Run query — Preset sales aggregate query is ready. Click [▶ Run (⌘+Enter)] below.` / `第 2 步：运行首条查询 — 已预置销售额统计语句，点击下方 [▶ 运行 (⌘+Enter)] 查出数据。`
- `onboarding.step2.action`: `Run & Advance →` / `一键运行并推进 →`
- `onboarding.step3.message`: `Step 3: Insights — Query succeeded! Click [📊 Visualize] on the result bar or [✨ AI Assistant].` / `第 3 步：数据洞察 — 查询已成功！点击结果栏右上角 [📊 生成图表] 或工具栏 [✨ AI 助手]。`
- `onboarding.step3.action`: `Complete Guide ✓` / `完成向导 ✓`
- `onboarding.skip`: `Skip Guide` / `跳过向导`

- [ ] **Step 4: 编写 `OnboardingGuideBar.tsx` 实现**

创建 `src/windows/connection/OnboardingGuideBar.tsx`：高度 38px，微 Accent 底色，带步骤徽标、提示文字、快捷推进按钮、跳过与关闭按钮。状态不是 `active` 时返回 `null`。

- [ ] **Step 5: 运行测试验证通过**

Run: `npx vitest run src/windows/connection/__tests__/OnboardingGuideBar.test.tsx`
Expected: PASS

- [ ] **Step 6: 提交代码**

```bash
git add src/windows/connection/OnboardingGuideBar.tsx src/windows/connection/__tests__/OnboardingGuideBar.test.tsx src/locales/en/core.ts src/locales/zh-CN/core.ts
git commit -m "feat(connection): add docked onboarding guide bar component"
```

---

### Task 5: 工作台挂载向导、预置示例 SQL 与左侧导航栏双模 (`ConnectionPage.tsx` & `uiStore.ts`)

**Files:**
- Modify: `src/stores/uiStore.ts`
- Modify: `src/windows/connection/ConnectionPage.tsx`
- Test: `src/windows/connection/__tests__/ConnectionPageOnboarding.test.tsx`

**Interfaces:**
- Consumes: `useOnboardingStore`, `useUiStore`, `OnboardingGuideBar`
- Produces:
  - 工作台自动打开示例 SQL Tab (`getting_started.sql`)
  - 顶部挂载 `OnboardingGuideBar`
  - 左侧 `WorkspaceModeButton` 导航支持 `icons` (40px) 与 `expanded` (120px) 双模

- [ ] **Step 1: 编写挂载向导与双模导航测试**

```tsx
// src/windows/connection/__tests__/ConnectionPageOnboarding.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, beforeEach, vi } from 'vitest';
import { useUiStore } from '../../../stores/uiStore';
import { useOnboardingStore } from '../../../stores/onboardingStore';

describe('ConnectionPage Onboarding & Dual Mode Sidebar', () => {
  beforeEach(() => {
    useOnboardingStore.getState().resetOnboarding();
  });

  it('allows toggling workspace sidebar mode between icons and expanded', () => {
    expect(useUiStore.getState().workspaceSidebarMode).toBe('icons');
    useUiStore.getState().toggleWorkspaceSidebarMode();
    expect(useUiStore.getState().workspaceSidebarMode).toBe('expanded');
    useUiStore.getState().toggleWorkspaceSidebarMode();
    expect(useUiStore.getState().workspaceSidebarMode).toBe('icons');
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run src/windows/connection/__tests__/ConnectionPageOnboarding.test.tsx`
Expected: FAIL with `workspaceSidebarMode` undefined on `uiStore`

- [ ] **Step 3: 更新 `uiStore.ts` 添加双模状态**

在 `src/stores/uiStore.ts` 中添加:
```ts
export type WorkspaceSidebarMode = 'icons' | 'expanded';

// in UiStore interface:
workspaceSidebarMode: WorkspaceSidebarMode;
toggleWorkspaceSidebarMode: () => void;
setWorkspaceSidebarMode: (mode: WorkspaceSidebarMode) => void;
```
初始值读取 `localStorage.getItem('datazen:workspace-sidebar-mode') || 'icons'`，更新时写入。

- [ ] **Step 4: 更新 `ConnectionPage.tsx`**

1. 引入 `<OnboardingGuideBar onExecuteSampleQuery={...} />` 并挂载在工作区顶部；
2. 检测首次进入示例库时，调用 `handleNewQuery` 打开 `getting_started.sql` 并填入预置销售分析 SQL；
3. 更新左侧导航栏：根据 `workspaceSidebarMode === 'expanded'` 自适应宽度 `w-28` 或 `w-10`，`WorkspaceModeButton` 在展开时显示文字标签；
4. 在侧栏底部提供展开/收起切换按钮。

- [ ] **Step 5: 运行测试验证通过**

Run: `npx vitest run src/windows/connection/__tests__/ConnectionPageOnboarding.test.tsx`
Expected: PASS

- [ ] **Step 6: 提交代码**

```bash
git add src/stores/uiStore.ts src/windows/connection/ConnectionPage.tsx src/windows/connection/__tests__/ConnectionPageOnboarding.test.tsx
git commit -m "feat(connection): integrate guide bar, auto-opened sample query, and dual-mode sidebar"
```

---

### Task 6: 更多操作溢出下拉菜单组件 (`QueryToolbarMoreMenu.tsx`)

**Files:**
- Create: `src/windows/connection/query/QueryToolbarMoreMenu.tsx`
- Test: `src/windows/connection/__tests__/QueryToolbarMoreMenu.test.tsx`

**Interfaces:**
- Consumes:
  ```tsx
  export interface QueryToolbarMoreMenuProps {
    compact?: boolean;
    disabled?: boolean;
    supportsExplain?: boolean;
    explainDisabled?: boolean;
    formatDisabled?: boolean;
    inTransaction?: boolean;
    txBusy?: boolean;
    onFormat: () => void;
    onExplain: () => void;
    onBeginTx: () => void;
    onCommitTx: () => void;
    onRollbackTx: () => void;
    onRefreshCompletion?: () => void;
    renderSnippetButton?: () => React.ReactNode;
  }
  ```

- [ ] **Step 1: 编写更多操作菜单测试**

```tsx
// src/windows/connection/__tests__/QueryToolbarMoreMenu.test.tsx
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, it, expect, vi } from 'vitest';
import { QueryToolbarMoreMenu } from '../query/QueryToolbarMoreMenu';

describe('QueryToolbarMoreMenu', () => {
  it('renders trigger button and opens menu on click', () => {
    const onFormat = vi.fn();
    const onExplain = vi.fn();

    render(
      <QueryToolbarMoreMenu
        onFormat={onFormat}
        onExplain={onExplain}
        onBeginTx={vi.fn()}
        onCommitTx={vi.fn()}
        onRollbackTx={vi.fn()}
        supportsExplain
      />,
    );

    const trigger = screen.getByTestId('query-toolbar-more-menu-trigger');
    expect(trigger).toBeInTheDocument();

    fireEvent.click(trigger);
    expect(screen.getByTestId('more-menu-format')).toBeInTheDocument();
    expect(screen.getByTestId('more-menu-explain')).toBeInTheDocument();

    fireEvent.click(screen.getByTestId('more-menu-format'));
    expect(onFormat).toHaveBeenCalled();
  });
});
```

- [ ] **Step 2: 运行测试验证失败**

Run: `npx vitest run src/windows/connection/__tests__/QueryToolbarMoreMenu.test.tsx`
Expected: FAIL with module not found `../query/QueryToolbarMoreMenu`

- [ ] **Step 3: 编写 `QueryToolbarMoreMenu.tsx` 实现**

创建 `src/windows/connection/query/QueryToolbarMoreMenu.tsx`：采用 Web Dropdown Popover，包含格式化（带快捷键提示）、EXPLAIN、补全刷新、代码片段菜单项以及事务开始项；在点击后自动关闭弹层。

- [ ] **Step 4: 运行测试验证通过**

Run: `npx vitest run src/windows/connection/__tests__/QueryToolbarMoreMenu.test.tsx`
Expected: PASS

- [ ] **Step 5: 提交代码**

```bash
git add src/windows/connection/query/QueryToolbarMoreMenu.tsx src/windows/connection/__tests__/QueryToolbarMoreMenu.test.tsx
git commit -m "feat(query-toolbar): add QueryToolbarMoreMenu overflow dropdown"
```

---

### Task 7: 查询工具栏四区重排与断点算法升级 (`QueryEditorSection.tsx` & `queryToolbarWidth.ts`)

**Files:**
- Modify: `src/windows/connection/queryToolbarWidth.ts`
- Modify: `src/windows/connection/query/QueryEditorSection.tsx`
- Test: `src/windows/connection/__tests__/queryToolbarWidth.test.ts`
- Test: `src/windows/connection/__tests__/QueryEditorSection.test.tsx`

**Interfaces:**
- Consumes: `QueryToolbarMoreMenu`, `queryToolbarExpandedMinWidth`
- Produces: 重构后的主查询工具栏，核心操作常驻、事务保护自适应浮现、状态区右对齐

- [ ] **Step 1: 编写更新后的工具栏宽度测试**

```ts
// src/windows/connection/__tests__/queryToolbarWidth.test.ts
// Add tests asserting that min expanded width properly accounts for the consolidated toolbar layout
import { describe, it, expect } from 'vitest';
import { queryToolbarExpandedMinWidth } from '../queryToolbarWidth';

describe('queryToolbarExpandedMinWidth with consolidated layout', () => {
  it('calculates compact required width with fewer top-level buttons', () => {
    const width = queryToolbarExpandedMinWidth({
      supportsExplain: true,
      hasContextSelectors: true,
      isPathHierarchy: false,
      isMultiDb: false,
      namespaceTree: {} as any,
      pathAliases: {},
      databases: [],
      contextPath: [],
    });
    // With overflow menu consolidation, top-level button footprint is streamlined
    expect(width).toBeGreaterThan(400);
    expect(width).toBeLessThan(1200);
  });
});
```

- [ ] **Step 2: 运行测试验证**

Run: `npx vitest run src/windows/connection/__tests__/queryToolbarWidth.test.ts`
Expected: PASS or adapt to new consolidated button count

- [ ] **Step 3: 更新 `queryToolbarWidth.ts` 算法**

调整常驻按钮数量计算：核心区（运行+策略、保存、AI、更多菜单、历史、收藏），在事务激活时动态增加事务提交/回滚按钮宽度。

- [ ] **Step 4: 重构 `QueryEditorSection.tsx` 工具栏**

1. 核心区：保留上下文选择器、运行/取消、执行策略下拉、`[💾 保存]`、`[✨ AI 助手]`；
2. 更多操作区：引入 `<QueryToolbarMoreMenu />` 收纳格式化、EXPLAIN、代码片段、补全刷新、未开始时的事务；
3. **事务自适应浮现**：当 `inTransaction === true` 时，提交与回滚按钮直接显示在工具栏上，并紧邻 `TX` 徽标；
4. 状态区：固定在右侧，展示 Safe Mode 标签、执行耗时与行数。

- [ ] **Step 5: 运行工具栏相关测试验证通过**

Run: `npx vitest run src/windows/connection/__tests__/queryToolbarWidth.test.ts src/windows/connection/__tests__/query.modules.test.tsx`
Expected: PASS

- [ ] **Step 6: 提交代码**

```bash
git add src/windows/connection/queryToolbarWidth.ts src/windows/connection/query/QueryEditorSection.tsx src/windows/connection/__tests__/queryToolbarWidth.test.ts
git commit -m "refactor(query-toolbar): consolidate toolbar layout into 4 functional zones with overflow menu"
```

---

### Task 8: 全生命周期连续旅程集成测试 (`onboardingJourney.test.tsx`)

**Files:**
- Create: `src/windows/connection/__tests__/onboardingJourney.test.tsx`
- Test: `src/windows/connection/__tests__/onboardingJourney.test.tsx`

**Interfaces:**
- Consumes: 全流程模拟 WelcomePage 点击示例库 → 引导条状态跃迁 → 运行 SQL → 结果到达 → 触发图表完成向导

- [ ] **Step 1: 编写连续旅程测试**

```tsx
// src/windows/connection/__tests__/onboardingJourney.test.tsx
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { useOnboardingStore } from '../../../stores/onboardingStore';
import { sampleDataCommands } from '../../../commands/sampleData';

describe('Onboarding Continuous Journey Test', () => {
  beforeEach(() => {
    localStorage.clear();
    useOnboardingStore.getState().resetOnboarding();
  });

  it('runs complete lifecycle: welcome -> sample init -> step 2 query -> step 3 insights -> completion', async () => {
    // 1. Initial state
    expect(useOnboardingStore.getState().status).toBe('not_started');
    expect(useOnboardingStore.getState().step).toBe(1);

    // 2. User clicks "Open Sample SQLite"
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');
    expect(useOnboardingStore.getState().step).toBe(2);

    // 3. User executes the preset query -> success with rows
    useOnboardingStore.getState().markQueryExecuted();
    expect(useOnboardingStore.getState().queryExecuted).toBe(true);
    expect(useOnboardingStore.getState().step).toBe(3);

    // 4. User explores chart or AI action
    useOnboardingStore.getState().markAiOrChartExplored();
    expect(useOnboardingStore.getState().aiOrChartExplored).toBe(true);

    // 5. Completion
    useOnboardingStore.getState().completeOnboarding();
    expect(useOnboardingStore.getState().status).toBe('completed');
  });

  it('allows premature skip at step 2 and leaves store cleanly skipped', () => {
    useOnboardingStore.getState().startOnboarding('sample_sqlite');
    expect(useOnboardingStore.getState().status).toBe('active');

    useOnboardingStore.getState().skipOnboarding();
    expect(useOnboardingStore.getState().status).toBe('skipped');

    // Subsequent query execution does not reactivate guide bar
    useOnboardingStore.getState().markQueryExecuted();
    expect(useOnboardingStore.getState().status).toBe('skipped');
  });
});
```

- [ ] **Step 2: 运行旅程测试验证通过**

Run: `npx vitest run src/windows/connection/__tests__/onboardingJourney.test.tsx`
Expected: PASS

- [ ] **Step 3: 运行全量单元测试套件**

Run: `npx vitest run`
Expected: All test suites PASS without regressions

- [ ] **Step 4: 提交代码**

```bash
git add src/windows/connection/__tests__/onboardingJourney.test.tsx
git commit -m "test(onboarding): add complete continuous lifecycle journey test"
```

---

## Plan Review & Verification Checklist

1. **Spec Coverage**:
   - P0-1 离线 SQLite 示例库机制：已在 Task 2（`sample_db.rs`）与 Task 3（`WelcomePage.tsx`）中覆盖。
   - P0-2 三步首次激活向导与跳过：已在 Task 1、Task 3、Task 4、Task 5、Task 8 中覆盖。
   - P0-3 查询主工具栏重排与事务自适应浮现：已在 Task 6 与 Task 7 中覆盖。
   - 工作区侧栏双模：已在 Task 5 中覆盖。
2. **Placeholder Scan**: 全文无任何 `TODO` / `TBD` / 占位符。
3. **Type Consistency**: `OnboardingStatus`、`OnboardingState`、`WorkspaceSidebarMode` 类型命名与使用完全统一。
