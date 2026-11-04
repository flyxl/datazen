/**
 * I-1 草稿守卫（PRD §4 I-1，本轨 E-5）：模块级单一真值源。
 *
 * `StringEditor` 的 `jsonDirty` 经 `publishDraftDirty` 推到这里；所有会毁掉
 * 草稿的导航动作（刷新 / 切键 / 切页签 / 切 db / 搜索 / 关面板 / 自动刷新拍）
 * 先 `await requestDraftLeave()`：
 *
 * - 进入条件：`dirty === true` ⇒ 打开「放弃更改 / 继续编辑」对话框，
 *   Promise 悬起，调用方动作**尚未执行**（不是执行完再问）。
 * - 状态内行为：并发请求合并到同一个悬起 Promise；对话框由挂在编辑面里的
 *   `DraftLeaveDialog` 渲染（Dialog  portal 到 body，隐藏页签下照样可见）。
 * - 退出跃迁：`settleDraftLeave(true)`（放弃 ⇒ dirty 落 false，动作继续）或
 *   `settleDraftLeave(false)` / 编辑器卸载（继续编辑 ⇒ 动作取消）。
 *
 * dirty 为 false 时 `requestDraftLeave()` 立即 `true` —— 干净态导航零开销。
 */
type Listener = () => void;

let dirty = false;
let leavePending = false;
let leavePromise: Promise<boolean> | null = null;
let resolveLeave: ((proceed: boolean) => void) | null = null;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of [...listeners]) listener();
}

/** 推入最新草稿状态（`StringEditor` 唯一写入口，保存/放弃/卸载都会落 false）。 */
export function publishDraftDirty(next: boolean): void {
  if (dirty === next) return;
  dirty = next;
  emit();
}

export function isDraftDirty(): boolean {
  return dirty;
}

export function isLeavePending(): boolean {
  return leavePending;
}

export function subscribeDraftLeave(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 请求离开许可。干净 ⇒ 立即 true；脏 ⇒ 打开对话框并悬起，直到
 * `settleDraftLeave` 用 `true`（放弃并继续）或 `false`（继续编辑，取消动作）回答。
 * 悬起期间的重复请求合并到同一个 Promise。
 */
export function requestDraftLeave(): Promise<boolean> {
  if (!dirty) return Promise.resolve(true);
  if (leavePending && leavePromise) return leavePromise;
  leavePending = true;
  leavePromise = new Promise<boolean>((resolve) => {
    resolveLeave = resolve;
  });
  emit();
  return leavePromise;
}

/** 结束一次悬起的离开请求。`proceed=true` 表示用户放弃更改 ⇒ 草稿即刻失效。 */
export function settleDraftLeave(proceed: boolean): void {
  if (!leavePending) return;
  leavePending = false;
  const resolve = resolveLeave;
  resolveLeave = null;
  leavePromise = null;
  if (proceed) dirty = false;
  emit();
  resolve?.(proceed);
}

/** 测试专用：跨用例复位模块单例（生产路径只经 publish/settle 自然流转）。 */
export function __resetDraftGuard(): void {
  dirty = false;
  leavePending = false;
  leavePromise = null;
  resolveLeave = null;
  for (const listener of [...listeners]) listener();
  listeners.clear();
}
