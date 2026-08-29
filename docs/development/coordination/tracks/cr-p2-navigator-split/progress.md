# cr-p2-navigator-split — 进度

**轨 ID：** cr-p2-navigator-split | **状态：** 已完成

## 完成项

1. **抽取 `schemaTreeCategories.ts`** — `BASE_CATEGORIES` / `OBJECT_KIND_CATEGORIES` / `KV_CATEGORIES` / `LEAF_KIND_ICON` / `getCategoriesForDriver` / `getEffectiveCategories`；`UnifiedSchemaTree` 与 `ConnectionNavigatorTree` 共用。
2. **拆分 `ConnectionNavigatorTree.tsx`** — 主文件 660 行（<800）；子模块位于 `src/windows/connection/navigator/`：
   - `types.ts` — `UnifiedRow`、props、常量
   - `utils.ts` — namespace 扁平化、schema 分组、引用工具
   - `buildFlatRows.ts` — 统一 flat row 构建
   - `useNavigatorContextMenus.ts` — 右键菜单
   - `useNavigatorDbState.ts` — 多库缓存与 refresh
   - `NavigatorTreeRow.tsx` — 虚拟列表行渲染
   - `NavigatorToolbar.tsx` / `NavigatorDialogs.tsx` — 工具栏与对话框
3. **测试** — `ConnectionNavigatorTree.test.tsx` + `SchemaTree.test.tsx` 全绿（83 tests）。

## 验证

```bash
npx vitest run src/windows/connection/__tests__/ConnectionNavigatorTree.test.tsx \
  src/windows/connection/schema-tree/__tests__/SchemaTree.test.tsx
```
