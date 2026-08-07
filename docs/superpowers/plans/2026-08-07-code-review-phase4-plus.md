# Code Review Phase 4+ Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land C6 hard `config_id` rename, P3–P5, S1+ `.key` backup prompt, connection share menu+password IPC, and C2 full translations on `fix/code-review-phase-1-3`.

**Architecture:** Keep ZIP backups free of master keys; local at-rest key moves to OS keyring; cross-machine sharing uses Argon2 passworded connection JSON via dialog-atomic IPC; MCP/API naming matches `config_id` vs `connection_id` docs.

**Tech Stack:** Tauri v2, Rust (`keyring`, `aes-gcm`, Argon2), React/TS, Vitest, `cargo test -p datazen`.

**Spec:** [`docs/superpowers/specs/2026-08-07-code-review-phase4-plus-design.md`](../specs/2026-08-07-code-review-phase4-plus-design.md)

## Global Constraints

- Branch: `fix/code-review-phase-1-3` only.
- C6: hard switch — MCP params that mean config ID must be `config_id`; do **not** accept legacy `connection_id` for those structs.
- App-data ZIP must continue to **exclude** `.key`; never put Keychain secrets in ZIP.
- Path IPC for file IO stays `webdriver`-gated; production uses `*_with_dialog`.
- Connection import v1: overwrite-by-id merge; atomic Rust command; no conflict UI.
- Empty export/import password rejected.
- Process per item: unit tests → independent test agent → fix if fail → commit → update `docs/progress-code-review-fix.md`.
- Respond/commit in project style; no force-push; no unrelated refactors.
- User-facing copy via i18n keys (en + zh-CN minimum before C2 propagates).

---

### Task 0: Progress ledger rows

**Files:**
- Modify: `docs/progress-code-review-fix.md`

- [ ] **Step 1: Add rows for phase 4+**

Add to the summary table (⬜):

| ID | 标题 |
|----|------|
| P4 | SQL/NL 日志降级 debug |
| P5 | splash 等 bootstrap + 错误 i18n |
| C6R | connection_id → config_id 硬切换 |
| S1+ | 导出后提示并另存 `.key` |
| ConnShare | 菜单导出/导入连接 + 口令 |
| P3 | Keychain 主密钥 + 测试 fallback |
| C2F | 10 语系全量真翻译 |

Keep existing S1–E8 rows; note phase 4+ started.

- [ ] **Step 2: Commit**

```bash
git add docs/progress-code-review-fix.md
git commit -m "$(cat <<'EOF'
docs: track phase4+ items in code-review progress ledger

EOF
)"
```

---

### Task 1: P4 — Sensitive logs to debug

**Files:**
- Modify: `src-tauri/src/commands/query.rs`
- Modify: `src-tauri/src/commands/ai.rs` (info sites with `input = %natural_language`, `error = %error_message`, `content_preview`, `response_content`, `args = %args`, tool argument dumps)
- Modify: `src-tauri/src/workflow/workflows.rs` (resolved sql / first_row info logs)
- Test: `src-tauri/src/commands/query.rs` (add `#[cfg(test)]` module) **or** `src-tauri/tests/` grep-style unit in lib

**Interfaces:**
- Consumes: existing `truncate_str` if present
- Produces: info logs without SQL/NL payloads; debug may keep previews

- [ ] **Step 1: Write failing guard test**

Add to `src-tauri/src/commands/query.rs`:

```rust
#[cfg(test)]
mod log_hygiene_tests {
    #[test]
    fn execute_query_source_does_not_info_log_sql_preview() {
        let src = include_str!("query.rs");
        // Crude but effective: the info! macro block for execute_query must not bind sql_preview
        let start = src.find("pub async fn execute_query").expect("fn");
        let chunk = &src[start..start + 800];
        assert!(
            !chunk.contains("tracing::info!(") || !chunk.contains("%sql_preview"),
            "execute_query must not info!-log sql_preview"
        );
        assert!(
            chunk.contains("sql_len") || chunk.contains("tracing::debug!"),
            "expected sql_len and/or debug preview"
        );
    }
}
```

- [ ] **Step 2: Run test — expect FAIL**

```bash
cargo test -p datazen --lib commands::query::log_hygiene_tests -- --nocapture
```

Expected: FAIL (current code still has `%sql_preview` in info).

- [ ] **Step 3: Implement**

In `execute_query`:

```rust
tracing::info!(%connection_id, sql_len = sql.len(), "execute_query");
tracing::debug!(%connection_id, sql_preview = %sql.chars().take(500).collect::<String>(), "execute_query sql");
```

In `ai.rs`: change `input = %natural_language` to `input_len = natural_language.len()` at info; full text at debug. Same for error message bodies, content previews, tool `args` dumps (info: tool name + args_len; debug: args).

In `workflows.rs`: move `[workflow] step ... resolved sql` / `first_row` to `tracing::debug!`.

- [ ] **Step 4: Run tests — expect PASS**

```bash
cargo test -p datazen --lib commands::query::log_hygiene_tests
cargo check -p datazen
```

- [ ] **Step 5: Independent test agent** — ask for grep of remaining `info!` with sql/nl payloads; fix if any.

- [ ] **Step 6: Commit + progress**

```bash
git add src-tauri/src/commands/query.rs src-tauri/src/commands/ai.rs src-tauri/src/workflow/workflows.rs docs/progress-code-review-fix.md
git commit -m "$(cat <<'EOF'
fix(P4): demote SQL/NL payloads from info to debug logs

EOF
)"
```

---

### Task 2: P5 — Splash after bootstrap + i18n errors

**Files:**
- Modify: `src/main.tsx`
- Modify: `src/locales/en.ts`, `src/locales/zh-CN.ts` (new keys as needed)
- Modify: any startup/connection strings found hardcoded in `src/` that are user-visible (scan `MainWindow`, connection store error fallbacks)
- Test: `src/main.tsx` is hard to unit-test; extract splash helper **or** add Vitest for a tiny `hideSplash(el)` module

**Interfaces:**
- Produces: `async function bootstrap()` hides splash in `finally`

- [ ] **Step 1: Failing test for splash helper**

Create `src/lib/splash.ts`:

```ts
export function hideSplash(splash: HTMLElement | null): void {
  if (!splash) return;
  splash.classList.add('hide');
  window.setTimeout(() => splash.remove(), 350);
}
```

Create `src/lib/__tests__/splash.test.ts`:

```ts
import { describe, it, expect, vi } from 'vitest';
import { hideSplash } from '../splash';

describe('hideSplash', () => {
  it('adds hide class and schedules remove', () => {
    vi.useFakeTimers();
    const el = document.createElement('div');
    document.body.appendChild(el);
    hideSplash(el);
    expect(el.classList.contains('hide')).toBe(true);
    vi.advanceTimersByTime(350);
    expect(document.body.contains(el)).toBe(false);
    vi.useRealTimers();
  });
});
```

- [ ] **Step 2: Wire main.tsx**

```ts
async function bootstrap() {
  try {
    // existing settings preload + render
  } finally {
    hideSplash(document.getElementById('splash'));
  }
}
void bootstrap();
// REMOVE the old top-level splash hide that runs before bootstrap finishes
```

- [ ] **Step 3: i18n hardcoded errors**

Search `src/` for user-visible Chinese/English error literals not going through `t()` (connection failures shown in dialogs). Replace with keys under `common.*` / `conn.*`. Add keys to en + zh-CN.

- [ ] **Step 4: Run**

```bash
npx vitest run src/lib/__tests__/splash.test.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Test agent + commit**

```bash
git commit -m "$(cat <<'EOF'
fix(P5): defer splash hide until bootstrap finishes and i18n errors

EOF
)"
```

---

### Task 3: C6R — MCP/API `config_id` hard switch

**Files:**
- Modify: `src-tauri/src/mcp/server.rs` (all tool/prompt structs using config IDs)
- Modify: `src-tauri/src/services/db_tools.rs` (docs + param names where they mean config id; `list_*` helpers that take config)
- Modify: `src-tauri/src/commands/ai.rs` tool arg parsing that reads `args["connection_id"]` for config lookup → `config_id`
- Modify: frontend MCP/docs if any; `docs/architecture/backend/services.md`
- Modify: AGENTS.md MCP tools list if it documents `connection_id`
- Test: update `mcp/server.rs` unit tests; add negative test

**Interfaces:**
- MCP JSON field: `config_id: String` (required) / `Option<String>` for workflow
- `resolve_connection(cm, config_id: &str)` — document that the id is a **config** id (or runtime id still accepted internally by get-then-connect)

- [ ] **Step 1: Failing negative test**

In `src-tauri/src/mcp/server.rs` tests:

```rust
#[test]
fn query_input_rejects_legacy_connection_id_field() {
    let json = r#"{"connection_id":"c1","sql":"SELECT 1"}"#;
    let parsed = serde_json::from_str::<QueryInput>(json);
    assert!(parsed.is_err(), "legacy connection_id must not deserialize");
}

#[test]
fn query_input_accepts_config_id() {
    let json = r#"{"config_id":"c1","sql":"SELECT 1"}"#;
    let parsed: QueryInput = serde_json::from_str(json).unwrap();
    assert_eq!(parsed.config_id, "c1");
}
```

Rename struct fields:

```rust
pub struct QueryInput {
    /// Persistent connection config id (from list_connections)
    pub config_id: String,
    pub sql: String,
    pub limit: Option<u32>,
}
```

Apply to: `ListTablesInput`, `GetSchemaInput`, `ExplainQueryInput`, `DescribeTableInput`, `ListDatabasesInput`, `RunWorkflowInput`, `Nl2SqlArgs`, and any sibling prompt args.

- [ ] **Step 2: Run — FAIL then rename + fix call sites — PASS**

```bash
cargo test -p datazen --lib mcp::server::tests -- --nocapture
```

Update all `input.connection_id` → `input.config_id` in MCP handlers. Update AI chat tool JSON schemas / arg readers that expose `connection_id` to external tools similarly when they mean config id.

- [ ] **Step 3: Docs**

Update `docs/architecture/backend/services.md` MCP note: tools take `config_id`. Update AGENTS.md tools list.

- [ ] **Step 4: Test agent + commit**

```bash
git commit -m "$(cat <<'EOF'
fix(C6): hard-rename MCP config parameters to config_id

EOF
)"
```

---

### Task 4: S1+ — Post-export `.key` backup prompt

**Files:**
- Modify: `src-tauri/src/commands/config.rs` — add `save_encryption_key_with_dialog`
- Modify: `src-tauri/src/lib.rs` — register command
- Modify: `src/commands/backup.ts` (or `connection.ts`) — TS wrapper
- Modify: `src/windows/main/MainWindow.tsx` — after export success, confirm → save key
- Modify: `src/locales/en.ts`, `zh-CN.ts` — new `appData.*` keys
- Test: Rust unit for key bytes write helper; Vitest wiring

**Interfaces:**
- `save_encryption_key_with_dialog(app, state, default_file_name: String) -> Result<bool, CommandError>`
- Reads key from Store (file or, after P3, keyring). Until P3 lands, read `data_dir/.key` contents.

- [ ] **Step 1: Failing Rust test for helper**

Extract:

```rust
pub fn encryption_key_export_bytes(key_b64: &str) -> Vec<u8> {
    key_b64.trim().as_bytes().to_vec()
}
```

Test round-trip write to temp file.

- [ ] **Step 2: Implement dialog command** (mirror `save_text_with_dialog` pattern in `file.rs` / config dialogs)

```rust
#[tauri::command]
pub async fn save_encryption_key_with_dialog(
    app: AppHandle,
    state: State<'_, AppState>,
    default_file_name: String,
) -> Result<bool, CommandError> {
    // read current key material from store data_dir .key OR store API
    // FileDialogBuilder save → write bytes → Ok(true/false)
}
```

- [ ] **Step 3: Frontend**

Keys:

- `appData.exportSuccess` — already warns; strengthen to mention separate key backup
- `appData.backupKeyTitle` / `appData.backupKeyMessage` / `appData.backupKeySaved`

After export success:

```ts
showMessageDialog(t('appData.exportSuccess'), 'success');
const wantKey = await ask(/* title/message from i18n */); // use existing confirm dialog helper
if (wantKey) {
  await backupCommands.saveEncryptionKeyWithDialog('datazen.key');
}
```

Use the same confirm pattern as import app data (`ask` from dialog plugin or existing UI).

- [ ] **Step 4: Tests**

```bash
cargo test -p datazen --lib -- encryption_key
npx vitest run src/commands/__tests__/pathIpcWiring.test.ts  # extend expects
```

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(S1+): prompt to back up encryption key after app-data export

EOF
)"
```

---

### Task 5: ConnShare — Menu + dialog IPC + password

**Files:**
- Modify: `src-tauri/src/lib.rs` (menu items + events + command registration)
- Modify: `src-tauri/resources/menu-labels.json` (+ regenerate if script exists)
- Modify: `src/components/MenuBar.tsx`
- Modify: `src-tauri/src/commands/config.rs` — `export_connections_with_dialog`, `import_connections_with_dialog`
- Modify: `src/commands/connection.ts`
- Create: `src/components/connection/ConnectionShareDialog.tsx` (password + confirm password)
- Modify: `src/windows/main/MainWindow.tsx` — handlers
- Modify: locales en/zh-CN (`menu.exportConnections`, `menu.importConnections`, `connShare.*`)
- Relabel: `menu.exportConfig` / `menu.importConfig` → App Data wording (en/zh-CN already partly done)
- Test: Rust crypto already has tests; add merge unit test; E2E source asserts for menu ids

**Interfaces:**
- `export_connections_with_dialog(password: String, default_file_name: String) -> Result<u32, CommandError>` — reject `password.is_empty()`
- `import_connections_with_dialog(password: String) -> Result<ImportConnectionsResult, CommandError>`
- `ImportConnectionsResult { imported: u32, overwritten: u32, groups_added: u32 }`

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn export_rejects_empty_password() {
    assert!(validate_share_password("").is_err());
}

#[test]
fn merge_connections_overwrites_by_id() {
    // helper pure function: existing + incoming → counts
}
```

- [ ] **Step 2: Implement merge helper + dialog commands**

Reuse `derive_key_from_password` / `encrypt_with_key` / decrypt from existing export. Dialog save/open like app-data. On import: decrypt → for each conn `store.save_connection` (overwrite) → merge groups → return stats.

Gate path-based export/import remain webdriver-only.

- [ ] **Step 3: Menu**

Native + HTML menu:

```rust
let export_connections_item = MenuItemBuilder::new(t("export-connections")).id("export-connections").build(handle)?;
let import_connections_item = MenuItemBuilder::new(t("import-connections")).id("import-connections").build(handle)?;
// tools_menu: app data items keep ids export-config/import-config; add new items
```

Events: `menu:export-connections`, `menu:import-connections`.

- [ ] **Step 4: UI**

`ConnectionShareDialog`: mode `export` | `import`, password fields, validate match on export, call commands, toast results, `loadConnections()` refresh on import.

- [ ] **Step 5: Run**

```bash
cargo test -p datazen --lib commands::config
pnpm exec tsc --noEmit
npx vitest run src/commands/__tests__/pathIpcWiring.test.ts
```

- [ ] **Step 6: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(ConnShare): add menu export/import connections with password dialogs

EOF
)"
```

---

### Task 6: P3 — Keychain master key

**Files:**
- Modify: `src-tauri/Cargo.toml` — add `keyring` dependency
- Create: `src-tauri/src/store/key_store.rs` (or section in `store/mod.rs`)
- Modify: `src-tauri/src/store/mod.rs` — `get_or_create_encryption_key`
- Modify: S1+ key export to read from key API (not only file)
- Env: `DATAZEN_KEYRING=file` forces file backend for CI/tests
- Test: store tests with file backend; migration deletes `.key` after keyring write when keyring available

**Interfaces:**
- `fn load_or_create_master_key(data_dir: &Path) -> Result<[u8; 32], StoreError>`
- Service: `com.tbeasy.datazen`, user: `app-encryption-key`, password: base64 of 32 bytes

- [ ] **Step 1: Failing migration test (file mode)**

```rust
#[tokio::test]
async fn file_backend_creates_and_reloads_key() {
    std::env::set_var("DATAZEN_KEYRING", "file");
    let dir = tempfile::tempdir().unwrap();
    let k1 = Store::get_or_create_encryption_key_for_test(dir.path()).await.unwrap();
    let k2 = Store::get_or_create_encryption_key_for_test(dir.path()).await.unwrap();
    assert_eq!(k1, k2);
}
```

- [ ] **Step 2: Implement**

```rust
enum KeyBackend { Keyring, File }
fn backend() -> KeyBackend {
    if std::env::var("DATAZEN_KEYRING").ok().as_deref() == Some("file") {
        KeyBackend::File
    } else {
        KeyBackend::Keyring
    }
}
```

Order: keyring get → else file `.key` migrate into keyring then delete file → else generate.

On keyring error in production builds: log error and fall back to file with warning (document).

- [ ] **Step 3: Ensure ConnShare / encrypt still work** under file backend in unit tests.

- [ ] **Step 4: Commit**

```bash
git commit -m "$(cat <<'EOF'
feat(P3): store encryption master key in OS keychain with file fallback

EOF
)"
```

---

### Task 7: C2F — Full locale translations

**Files:**
- Modify: `src/locales/de.ts`, `es.ts`, `fr.ts`, `ja.ts`, `ko.ts`, `pt-BR.ts`, `ru.ts`, `zh-TW.ts`
- Modify: `src/locales/index.ts` — `FULLY_TRANSLATED_LOCALES` = all 10; `BETA_LOCALES` empty
- Modify: Settings UI that appends `(Beta)` — remove
- Modify: `src/locales/locales.test.ts` — tighten English-placeholder rate for former beta locales
- Modify: `src-tauri/resources/menu-labels.json` for new menu keys + app-data labels per locale
- Run: `node scripts/generate-menu-labels.mjs` if that is the source of truth

**Interfaces:**
- Every locale: `Record<TranslationKey, string>` with same keys as `en`

- [ ] **Step 1: Tighten failing tests first**

In `locales.test.ts`:

```ts
it('former beta locales are not mostly English copies', () => {
  const enVals = Object.values(en);
  for (const locale of ['de','es','fr','ja','ko','pt-BR','ru'] as const) {
    const dict = getAllTranslations(locale);
    let same = 0;
    for (const [k, v] of Object.entries(dict)) {
      if (v === (en as Record<string,string>)[k]) same++;
    }
    const ratio = same / enVals.length;
    expect(ratio, locale).toBeLessThan(0.35); // allow shared OK/Cancel/DataZen
  }
});

it('marks no locales as beta', () => {
  expect(BETA_LOCALES).toEqual([]);
});
```

Run — expect FAIL.

- [ ] **Step 2: Translate**

For each locale file: translate all values from English (and zh-TW from zh-CN into Traditional). Preserve `{placeholders}`. Include keys added in Tasks 2–5 (`appData.backupKey*`, `connShare.*`, `menu.exportConnections`, …).

Suggested commits: one per locale **or** one `fix(C2): …` if cleaner; update progress checkboxes per locale.

- [ ] **Step 3: Remove Beta UI**

```ts
export const FULLY_TRANSLATED_LOCALES = [...SUPPORTED_LOCALES];
export const BETA_LOCALES = [] as const satisfies readonly SupportedLocale[];
```

Remove `(Beta)` label in Settings language dropdown.

- [ ] **Step 4: Run**

```bash
npx vitest run src/locales/locales.test.ts
pnpm exec tsc --noEmit
```

- [ ] **Step 5: Commit**

```bash
git commit -m "$(cat <<'EOF'
fix(C2): ship full translations for all 10 locales and drop beta marks

EOF
)"
```

---

## Spec coverage checklist

| Spec section | Task |
|--------------|------|
| S1+ key backup prompt | Task 4 |
| ConnShare menu + password | Task 5 |
| C2 full translation | Task 7 |
| P3 Keychain | Task 6 |
| C6 hard switch | Task 3 |
| P4 logs | Task 1 |
| P5 splash/i18n | Task 2 |
| Progress ledger | Task 0 |
| Menu option 1 (4 items) | Task 5 |
| Import overwrite-by-id | Task 5 |
| `DATAZEN_KEYRING=file` | Task 6 |

## Self-review notes

- No TBD placeholders left in task steps.
- ConnShare depends on password dialog before P3; P3 does not block ConnShare crypto (password path independent).
- C2 last so new i18n keys exist.
- Path IPC gates preserved.
