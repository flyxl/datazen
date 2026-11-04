/**
 * 「放弃更改 / 继续编辑」对话框（PRD §4 I-1，本轨 E-5）。
 *
 * 挂在 `StringEditor` 里（脏 ⇒ 编辑面必然在挂载中，这是 I-1 的不变量），
 * 真值源是 `shared/draftGuard` 的 `useSyncExternalStore` 快照。`@datazen/ui`
 * 的 `Dialog` 走 `createPortal(document.body)`，所以 keep-alive 页签被 `hidden`
 * 藏住时对话框依旧可见。
 *
 * 退出跃迁三选一：
 *  - 放弃更改（`redis-draft-discard`）⇒ 先回调 `onDiscard` 回滚编辑面草稿，
 *    再 `settleDraftLeave(true)` 放行动作；
 *  - 继续编辑（`redis-draft-keep`）或右上角 ✕ / Esc ⇒ `settleDraftLeave(false)`，
 *    动作取消，草稿原样保留；
 *  - 组件卸载（编辑器没了 ⇒ 草稿也没了）⇒ 悬起的请求一律按 false 结算，
 *    不让任何调用方的 await 悬死。
 *
 * 断言口径（裁定 8-4）：`data-testid` 定位、文案只断 `data-i18n-key`。
 */
import { useEffect, useSyncExternalStore } from 'react';
import { Button, Dialog, useI18n } from '@datazen/ui';
import { isLeavePending, settleDraftLeave, subscribeDraftLeave } from '../shared/draftGuard';

export interface DraftLeaveDialogProps {
  /** 放弃更改时回滚编辑面（值复位 + 脏信号落 false）；必须在 settle 之前调用。 */
  onDiscard?: () => void;
}

export function DraftLeaveDialog({ onDiscard }: DraftLeaveDialogProps) {
  const { t } = useI18n();
  const pending = useSyncExternalStore(subscribeDraftLeave, isLeavePending);

  useEffect(() => {
    return () => {
      // Editor unmounted ⇒ its draft no longer exists; never strand a caller.
      if (isLeavePending()) settleDraftLeave(false);
    };
  }, []);

  return (
    <Dialog
      open={pending}
      title={t('redis.detail.leave.title')}
      onClose={() => settleDraftLeave(false)}
      testId="redis-draft-leave-dialog"
      footer={
        <>
          <Button
            variant="secondary"
            className="h-8 px-3 text-xs"
            data-testid="redis-draft-keep"
            onClick={() => settleDraftLeave(false)}
          >
            {t('redis.detail.leave.keepEditing')}
          </Button>
          <Button
            variant="danger"
            className="h-8 px-3 text-xs"
            data-testid="redis-draft-discard"
            onClick={() => {
              onDiscard?.();
              settleDraftLeave(true);
            }}
          >
            {t('redis.detail.leave.discard')}
          </Button>
        </>
      }
    >
      <span className="text-xs text-fg-secondary" data-i18n-key="redis.detail.leave.description">
        {t('redis.detail.leave.description')}
      </span>
    </Dialog>
  );
}
