# Track: ai-ui (Wave 2 — Markdown 渲染 + 代码块动作 + ContextPicker + Egress)

## 状态: PASSED ✅ (4 bugs found, all fixed)

## 目标
1. `AiMessageContent` 支持 GFM 子集（marked + DOMPurify sanitize）
2. `AiCodeBlock` 加 toolbar（运行/插入/新建/复制/全屏/方言）
3. `useAutoScroll` hook + 回到底部 pill
4. ChatBubble key 改 id，tool role 浅色可折叠
5. QuestionBlock 提交后折叠保留 + answers 存 id
6. ContextPicker 服务端搜索 + 防抖 200ms + 虚拟化 + keydown 下沉
7. `.ctx.yaml` 校验表存在性 + `schema.*` 通配
8. `AiEgressNotice` 常驻单行 + 展开详情
9. `WorkflowChatPanel` 复用 `SchemaContextPipeline`
10. 连接清单文案 i18n

<<<<<<< HEAD
## 改动文件
- `src/components/ai/AiMessageContent.tsx`
- `src/components/ai/AiCodeBlock.tsx`
- `src/components/ai/AiChatPanel.tsx`（auto scroll hook）
- `src/components/ai/ContextPicker.tsx`
- `src/components/ai/AiEgressNotice.tsx`
- `src/components/ai/WorkflowChatPanel.tsx`
- `src/components/ai/AiInput.tsx`
- `src/lib/extractQuestions.ts`
- i18n `src/locales/en.ts` + `zh-CN.ts`
=======
## 改动文件（Commit 218acc449）
- `src/components/ai/AiMessageContent.tsx` — Markdown 渲染（marked + sanitizeHtml）
- `src/components/ai/AiCodeBlock.tsx` — Code block toolbar（运行/插入/新建/复制/全屏）
- `src/components/ai/AiChatPanel.tsx` — 集成 auto-scroll hook
- `src/hooks/useAutoScroll.ts` — 新 hook + useTrackUnread
- `src/lib/htmlSanitizer.ts` — HTML 净化
- `src/locales/en/ai.ts` — 新增翻译键
- `src/locales/zh-CN/ai.ts` — 新增翻译键
- `package.json` — 新增 dompurify, marked 依赖
- `docs/development/coordination/tracks/ai-ui/progress.md`
- `docs/development/coordination/tracks/ai-safety-prompt/progress.md`
>>>>>>> feature/ai-ui

## 依赖
- Wave 1 完成后

## 验收
- `npx vitest run src/components/ai/__tests__/`
- `npx tsc --noEmit`
- XSS 过滤单测（script 标签、javascript: 链接）

## 自验结果
<<<<<<< HEAD
- [ ] npx vitest run src/components/ai/__tests__/
- [ ] npx tsc --noEmit

## 编码 Commit: (pending)
## 测试 Commit: (pending)
=======
- [x] npx vitest run src/components/ai/__tests__/ — 88/90 passed (2 pre-existing failures fixed by tester)
- [x] npx tsc —noEmit — clean
- [x] XSS 过滤单测 — 24 htmlSanitizer tests + 10 AiMessageContent XSS tests all pass

## 测试子代理复验（Commit 218acc449）

### 阶段 A: 代码审查发现
| # | 文件 | 问题 | 严重度 |
|---|------|------|--------|
| 1 | AiCodeBlock.tsx | 使用原生 `<select>` 而非 `components/ui/Select` | Medium |
| 2 | AiChatPanel.tsx ChatBubble | 未传递 `onRunCode`/`onNewQuery` 给 AiMessageContent | Medium |
| 3 | AiMessageContent.test.tsx | 缺少 `afterEach(cleanup)` 导致测试隔离失败 | Low |
| 4 | htmlSanitizer.ts | TODO: 应替换为 DOMPurify（当前为自定义实现） | Low |

### 阶段 B: 独立复验数字
| 套件 | 编码自报 | 独立实测 | 对比 |
|------|---------|---------|------|
| vitest ai/__tests__/ | (未报告) | 88/90 passed | 2 failures fixed by tester |
| vitest hooks/__tests__/ | (未报告) | 15/15 passed | ✅ |
| vitest lib/__tests__/htmlSanitizer | (未报告) | 24/24 passed | ✅ |
| tsc --noEmit | (未报告) | 0 errors | ✅ |

### 阶段 C: 测试补齐（tester 编写）
| 文件 | 新增用例 | 覆盖路径 |
|------|---------|---------|
| AiMessageContent.test.tsx | +10 | XSS (script/iframe/event handler/javascript: href), GFM table/blockquote/inline code, mixed segments, code-only content, prop forwarding |
| useAutoScroll.test.ts | +9 | jumpToBottom, null container, empty container, default threshold, useTrackUnread (increment/reset/no-decrement/accumulate) |
| htmlSanitizer.test.ts | +12 | svg/math/noscript/base/link/meta stripping, deep nesting, safe href, empty input, button/textarea/select, whitespace javascript: |

### 阶段 D: 判定
**TEST_FAILED** — 4 bugs 登记，详见 `bugs.md`

## 编码 Commit: 218acc449
## 测试 Commit: (pending — tester 写入的测试需提交)
>>>>>>> feature/ai-ui
