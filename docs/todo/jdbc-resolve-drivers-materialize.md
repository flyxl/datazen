# TODO: 将 resolve-drivers materialize 结果固化进仓库

**分支:** `feature/jdbc-agent`  
**状态:** 待办  
**优先级:** 高（影响本地/CI 无网或首次构建）  
**关联:** JDBC Agent（`packages/drivers/jdbc`、`datazen-jdbc-agent`）

---

## 背景

JDBC 前端接线需要在 `scripts/resolve-drivers.mjs` 中注册：

- `BASIC_PATH_FRONTEND.jdbc`（`jdbcMeta`、`JdbcConnectionFields`、`jdbcValidate`、Settings）
- `DRIVER_LOCALE_CONFIG.jdbc`（`en` / `zh-CN`）

上传大文件时该脚本曾被误写成占位内容，当前入口是**自愈（materialize）**：

1. 从 GitHub raw 下载 commit `0d08398e8012b171c43c5352e0c208e1154b4d1a` 上的完整 `resolve-drivers.mjs`
2. 打上 jdbc 的 frontend / locale 补丁
3. 写出 `scripts/resolve-drivers.impl.mjs` 再 `import`

**问题：** 首次 materialize **依赖网络**；`impl` 若未提交，离线 CI / 无 raw 访问会失败。

---

## 目标

将 materialize 后的**完整脚本**固化为仓库内正式源文件，去掉对 curl / raw 的依赖。

---

## 步骤（本地执行）

```bash
git checkout feature/jdbc-agent
git pull

# 生成带 jdbc 补丁的完整实现
node scripts/resolve-drivers.mjs --rematerialize

# 确认 impl 含 jdbc 注册
grep -n "jdbcMeta\|JdbcConnectionFields\|jdbcLocale" scripts/resolve-drivers.impl.mjs

# 用完整实现覆盖入口（去掉自愈 loader）
cp scripts/resolve-drivers.impl.mjs scripts/resolve-drivers.mjs

# 冒烟：仅 codegen
node scripts/resolve-drivers.mjs --codegen-only --drivers=basic,jdbc
# 或
DATAZEN_DRIVERS=basic,jdbc node scripts/resolve-drivers.mjs --codegen-only

# 确认 generated 产物
grep -n "jdbc\|JdbcConnection" src/plugins/generated.ts src/plugins/generated-locales.ts || true
```

提交建议：

```text
fix(build): restore full resolve-drivers.mjs with jdbc frontend registration

- Replace self-heal curl loader with in-tree script
- Register jdbc meta, connection form, settings, locales
```

可选清理：

- [ ] 删除残留的 `scripts/resolve-drivers.b64.*.txt`（若仍存在）
- [ ] 删除或 gitignore `scripts/resolve-drivers.impl.mjs`（若已合并进主文件）
- [ ] 在 `.gitignore` 中确认不要忽略正式的 `resolve-drivers.mjs`

---

## 验收

- [ ] `scripts/resolve-drivers.mjs` 为完整源码（无 “Self-heal entry” / curl raw 逻辑）
- [ ] 文件中含 `jdbcMeta`、`JdbcConnectionFields`、`jdbcValidate`、`JdbcSettingsSection`、`jdbcLocale`
- [ ] 无网环境下 `node scripts/resolve-drivers.mjs --codegen-only --drivers=basic,jdbc` 成功
- [ ] `DATAZEN_DRIVERS=basic,jdbc` 时连接类型列表出现 **JDBC**，表单为专用字段（URL / jars / driverClass）
- [ ] Settings → Extensions（或插件设置）中可见 **JDBC** 段（java / agent jar / idle timeout）

---

## 补丁语义（供人工对照）

若无法 materialize，可在完整旧版 `resolve-drivers.mjs` 上手动加入：

1. **`BASIC_PATH_FRONTEND`** 在 `vector` 条目后增加 `jdbc`：

```js
jdbc: {
  dbTypes: [{ id: 'jdbc', metaExport: 'jdbcMeta' }],
  metaPath: '../../packages/drivers/jdbc/ui/meta',
  connectionForm: {
    component: 'JdbcConnectionFields',
    path: '../../packages/drivers/jdbc/ui/JdbcConnectionFields',
    formVariant: 'jdbc',
    validator: { export: 'jdbcValidate' },
  },
  settings: {
    pluginId: 'jdbc',
    label: 'JDBC',
    sectionExport: 'JdbcSettingsSection',
    sectionPath: '../../packages/drivers/jdbc/ui/settings',
    schemaExport: 'jdbcSettingsSchema',
    schemaPath: '../../packages/drivers/jdbc/ui/settings',
  },
},
```

2. **`DRIVER_LOCALE_CONFIG`** 增加：

```js
jdbc: {
  path: '../../packages/drivers/jdbc/locales',
  typeExport: 'JdbcTranslationKey',
  importPrefix: 'jdbcLocale',
},
```

相关 UI 源文件（已在分支上）：

- `packages/drivers/jdbc/ui/JdbcConnectionFields.tsx`
- `packages/drivers/jdbc/ui/settings.tsx`
- `packages/drivers/jdbc/ui/meta.ts`
- `packages/drivers/jdbc/locales/en.ts` / `zh-CN.ts`

---

## 完成后

- [ ] 将本 TODO 标为完成或移入归档
- [ ] 更新 `docs/todo/jdbc-agent-implementation-plan.md` 中「Settings / 连接表单」相关未勾项（若已覆盖）
