# 收藏改名弹窗复用 Dialog 图元

Written against: 54110ab85c2cf42d7110119516ffc71ed55e852f

## Evidence chain

- Surface: 连接工作区查询面板收藏改名弹窗——`src/windows/connection/QueryPanel.tsx:529-545` 渲染的 `<FavoriteNameDialog>`，实现位于 `src/windows/connection/query/QueryTransactionModals.tsx:206-258`
- Problem: 同一文件内存在两套互相矛盾的弹窗语言。`QueryTransactionModals:154-169` 的未关闭事务弹窗与 `:171-192` 的中止事务弹窗使用宿主 `Dialog`（`src/components/ui/Dialog.tsx` → `@datazen/ui` `Dialog`），自带 portal、焦点陷阱、Esc 关闭、遮罩与 header/body/footer 结构；而 `FavoriteNameDialog:215-258` 自造 modal：`:215` `if (!open) return null` 手动挂载；`:218` `fixed inset-0 z-50 … bg-black/40` 手写遮罩（与 `Dialog` 的 `bg-black/50` 色值矛盾）；`:219` `w-[400px] rounded-lg border-edge bg-surface` 手写面板（与 `Dialog` 的 `max-w-xl rounded-[12px] bg-surface-alt` 矛盾）；无 `role="dialog"/aria-modal`、无焦点陷阱、无 Esc 处理（仅 Enter 保存 `:231-233`），`autoFocus` 输入与 `Dialog` 的“聚焦首个可聚焦元素”逻辑重复。键盘用户 Esc 无法关闭此弹窗，与同文件其他弹窗行为矛盾
- Design evidence: 宿主权威——`packages/ui/src/Dialog.tsx:101-148` 拥有 portal（`:101` `createPortal(…, document.body)`）、遮罩（`:103` `bg-black/50`）、面板（`:115` `rounded-[12px] border-edge bg-surface-alt`）、header/body/footer（`:119,139,141`）、Esc 关闭（`:58-63`）、焦点陷阱与返回焦点（`:37-56,65-97`）、`role="dialog" aria-modal`（`:106-107`）；薄包装 `src/components/ui/Dialog.tsx:6-9` 注入 `closeLabel`；同文件范例 `QueryTransactionModals:154-169`（`Dialog` + ghost/primary footer）与 `src/components/ui/ConfirmDialog.tsx:66-90`（`Dialog max-w-lg` + `h-8 px-3 text-xs` footer 按钮）、`src/components/ui/ResultMessageDialog.tsx:26-30`（`Dialog max-w-sm`）证明小尺寸对话框应通过 `className="max-w-*"` 收敛而非手写宽度
- Owner: `packages/ui/src/Dialog.tsx`（portal/焦点/Esc/结构拥有者），消费包装为 `src/components/ui/Dialog.tsx`
- Scope and affected surfaces: `FavoriteNameDialog` 定义（`QueryTransactionModals.tsx:1-16` imports + `:197-259`）与其唯一调用点（`QueryPanel.tsx:38,529-545`）及一处单测（`src/windows/connection/__tests__/query.modules.test.tsx:20,722`）。事务弹窗、导出弹窗、加看板弹窗不受影响
- Uncertainty: 无。纠正方向唯一（复用 `Dialog`），footer 沿用同文件既有 ghost + primary 写法，不存在多解

## Design decision

把 `FavoriteNameDialog` 的手写遮罩+面板替换为宿主 `Dialog`（`src/components/ui/Dialog.tsx`），标题用 `title` prop、按钮组进 `footer` prop、标题输入与 SQL 预览进 `children`。此后收藏弹窗与同文件事务弹窗共享同一 portal、遮罩色、圆角、焦点陷阱与 Esc 行为，根除 query 工具栏上唯一的自造 modal 路径；弹窗文案、校验规则（空名禁用保存）、testid、SQL 预览保持不变。

## Reuse

- `Dialog`（`src/components/ui/Dialog.tsx`，底层 `packages/ui/src/Dialog.tsx`）：portal、Esc、焦点陷阱、header/body/footer
- `Button`（`src/components/ui/Button.tsx`）：footer 的 `ghost` 取消 + `primary` 保存，尺寸沿用同文件既有 `className="h-7 px-3 text-xs"`
- Exemplar: 同文件 `QueryTransactionModals.tsx:154-169`（`Dialog` + 双按钮 footer）、`src/components/ui/ConfirmDialog.tsx:66-90`（`max-w-lg` + 小尺寸 footer）、`src/components/ui/ResultMessageDialog.tsx:26-30`（`max-w-sm` 小弹窗）

不新增图元。现有系统已能表达全部决策；手写 `fixed inset-0 … bg-black/40` 是应被收敛的并行路径。

## Changes

1. `src/windows/connection/query/QueryTransactionModals.tsx:1-16,197-259`
   - Change:
     - imports 已含 `Button`（`:3`）、`Dialog`（`:4`）、`useI18n`（`:14`），无需新增；如 `useCallback/useEffect/useState` 仅被 `FavoriteNameDialog` 以外逻辑使用则保留，否则不动（不顺手清理无关 import）
     - `FavoriteNameDialog` 函数体改写为：
       ```tsx
       export function FavoriteNameDialog({ open, favoriteName, favoriteDialogSql, onFavoriteNameChange, onClose, onSave }: FavoriteNameDialogProps) {
         const { t } = useI18n();
         return (
           <Dialog
             open={open}
             title={t('common.addToFavorites')}
             onClose={onClose}
             className="max-w-md"
             footer={
               <>
                 <Button variant="ghost" className="h-7 px-3 text-xs" onClick={onClose}>
                   {t('common.cancel')}
                 </Button>
                 <Button
                   variant="primary"
                   className="h-7 px-3 text-xs"
                   disabled={!favoriteName.trim()}
                   onClick={onSave}
                   data-testid="query-favorite-save"
                 >
                   {t('common.save')}
                 </Button>
               </>
             }
           >
             <div className="mb-2">
               <label className="mb-1 block text-xs text-fg-muted">{t('query.favoriteTitle')}</label>
               <input
                 type="text"
                 data-testid="query-favorite-title-input"
                 value={favoriteName}
                 onChange={(e) => onFavoriteNameChange(e.target.value)}
                 placeholder={t('query.favoriteTitlePlaceholder')}
                 className="h-8 w-full rounded border border-edge bg-surface-alt px-2 text-sm text-fg focus:border-accent focus:outline-none"
                 onKeyDown={(e) => {
                   if (e.key === 'Enter' && favoriteName.trim()) onSave();
                 }}
               />
             </div>
             <div>
               <label className="mb-1 block text-xs text-fg-muted">SQL</label>
               <div className="max-h-[120px] overflow-auto rounded border border-edge bg-surface-alt p-2 font-mono text-xs text-fg-secondary">
                 {favoriteDialogSql}
               </div>
             </div>
           </Dialog>
         );
       }
       ```
     - 关键点：删除 `:215` 手动 `if (!open) return null`（`Dialog` 内部处理）；删除 `:218-219` 手写遮罩+面板；删除 `autoFocus`（`Dialog` 自动聚焦首个可聚焦元素即标题输入）；保留 Enter 保存；`className="max-w-md"`（`448px`，最接近原 `w-[400px]` 的标准刻度，归入 `ResultMessageDialog max-w-sm` / `ConfirmDialog max-w-lg` 家族）
   - Preserve: props 接口（`FavoriteNameDialogProps` 一字不动）、`data-testid="query-favorite-title-input"/"query-favorite-save"`、`t('common.addToFavorites'/'query.favoriteTitle'/'query.favoriteTitlePlaceholder'/'common.cancel'/'common.save')` 文案、空名禁用保存、SQL 预览的 `max-h-[120px]` 与 mono 样式、调用点 `QueryPanel.tsx:529-545` 不动
   - Verify: 打开收藏弹窗时标题输入自动聚焦；Tab 不逃出弹窗；Esc 关闭（触发 `onClose` 即 `setShowFavoriteDialog(false)`）；遮罩为 `bg-black/50`、面板为 `rounded-[12px] bg-surface-alt`，与事务弹窗一致

2. `src/windows/connection/__tests__/query.modules.test.tsx:20,722`（如断言了手写结构则同提交更新，否则不动）
   - Change: 仅当用例查询了 `.fixed.inset-0`、`bg-black/40`、`w-[400px]` 等手写结构时，改为查询 `Dialog` 语义（`role="dialog"` / `query-favorite-title-input` 可见性）；行为断言（空名禁用、保存回调、关闭回调）原样保留
   - Preserve: 测试意图不变，不新增用例
   - Verify: `npx vitest run src/windows/connection/__tests__/query.modules.test.tsx` 通过

## Scope

- Inherit: 未来 query 域一切小表单弹窗（改名、重命名、快速命名）自动继承 `Dialog` 语言
- Verify: 同文件事务弹窗（未关闭/已中止）、`QueryPanel` 的 `ResultMessageDialog`、执行确认弹窗、导出弹窗——确认未被本次改写污染；`Dialog` 全局样式变更（如有）会同时影响本弹窗，属预期继承
- Exclude: 标题输入改用 `Input` 图元（`h-10` 与现有 `h-8` 紧凑表单的取舍需另行裁决，本计划保留原生 `input` 以最小化 diff）；SQL 预览改 `pre` 语义；overlay 点击关闭（`Dialog` 本来就不支持，本计划不加）；收藏的增删查业务逻辑不动

## Validation

- Product: 在查询面板收藏一条 SQL（历史/收藏侧栏 → 收藏按钮），弹窗标题、输入框、SQL 预览、取消/保存按钮与改前文案一致；空标题时保存禁用；有标题时 Enter 与点击保存等效；Esc 关闭且焦点回到触发前元素
- Interface: 路由 `connections → query panel → favorites`；状态：空名/有标题/超长 SQL（`max-h-[120px]` 内滚动）/ `400px` 窄栏；交互：Tab 循环、Shift+Tab、Esc；视口 `1280/800` 确认弹窗居中且不溢出
- System: 确认无第二套 modal 路径——`rg -n 'bg-black/40|w-\[400px\]' src/windows/connection/query/QueryTransactionModals.tsx` → 无命中；`rg -n 'from.*components/ui/Dialog' src/windows/connection/query/QueryTransactionModals.tsx` → `Dialog` 引用覆盖事务弹窗与收藏弹窗
- Repository: `npx vitest run src/windows/connection/__tests__/query.modules.test.tsx` → 通过；`npx tsc --noEmit`（如有）→ 无新增类型错误

## Stop conditions

- Stop if 产品要求收藏弹窗保持 `400px` 精确宽度或 `bg-black/40` 浅遮罩（以用户明确指示为准）——此时 `className` 改为 `max-w-[400px]` 或保留浅遮罩需用户裁决，停下并报告，不擅自发明新变体
- Stop if `Dialog` 的自动聚焦首元素行为与收藏输入冲突（如未来加了前置下拉导致焦点错位）——停下并报告，不在 `FavoriteNameDialog` 内另起焦点逻辑

## Design documentation

- After acceptance and validation: 无需更新设计文档。如仓库日后新增 query 域设计文档，可补记一句「query 域小表单弹窗一律复用宿主 `Dialog`（`max-w-sm/md/lg` 刻度），禁止手写 `fixed inset-0` 遮罩」，目的地为该新文件，否则为 none
