//! 慢日志一级页签（裁定 8-1：右列页签从 4 枚补到 5 枚，慢日志从 `MonitorPanel`
//! 的二极子页升为一级）。
//!
//! 数据只走既有 `slowlog_get` / `slowlog_reset` 命令 —— 屏 A 的 `SlowlogCard`
//! 已在用同一通道，零新增后端（PRD §6）。行模型与错误分类直接复用
//! `overview/overviewModel` 的纯函数，避免两处各写一份 SLOWLOG 形状解析。
//!
//! I-11 具名空态：权限缺失 / Redis 不支持（`unknown command`、`disabled`）走
//! `unauthorized` 分支，真·0 条走 `empty` 分支 —— 两者都渲染具名文案，绝不留白，
//! 也不把「未授权」伪装成「没有慢查询」。
//!
//! 断言口径（PRD §7-6）：状态机通过 `data-slowlog-state` 暴露，测试只读该属性与
//! `data-row-count`，不钉任何翻译后的英文字面量。

import { useCallback, useEffect, useState } from 'react';
import { RefreshCw, Trash2 } from 'lucide-react';
import { Button, Dialog, useI18n, Spinner } from '@datazen/ui';
import { redisCommandInvoke, type RedisInvokeFn } from '../shared/redisInvoke';
import { useRedisGate } from '../shared/useRedisGate';
import {
  buildSlowlogRows,
  classifyOverviewError,
  type OverviewSlowlogEntry,
  type SlowlogRow,
} from '../overview/overviewModel';

/** 与 `MonitorPanel` 的 `SLOWLOG GET` 预算保持一致。 */
export const SLOWLOG_PANEL_COUNT = 100;

/**
 * 面板状态机的五个取值。
 * 进入：挂载即 `loading`；退出：`load` 的 then/catch 折叠成
 * `ready | empty | unauthorized | failed`（无按键时不渲染表格，见 `rows.length`）。
 */
export type SlowlogPanelState = 'loading' | 'ready' | 'empty' | 'unauthorized' | 'failed';

export interface SlowlogPanelProps {
  dbSessionId: string;
  /** 测试注入点；默认走 `redisCommandInvoke`。 */
  invoke?: RedisInvokeFn;
}

/**
 * 把 `slowlog_get` 的返回/错误形状折叠成状态机取值（纯函数，单独测）。
 *
 * Redis 不支持 SLOWLOG（托管代理常砍掉）与 ACL 拒绝在协议层不可区分，二者共用
 * `unauthorized` 这一具名空态；其余错误才是 `failed`，渲染错误条 + 可重试。
 */
export function resolveSlowlogState(
  entries: readonly unknown[] | null | undefined,
  error: unknown,
): SlowlogPanelState {
  if (error) {
    return classifyOverviewError(error) === 'unauthorized' ? 'unauthorized' : 'failed';
  }
  if (!Array.isArray(entries) || entries.length === 0) return 'empty';
  return 'ready';
}

/** µs → 人类可读耗时（数据格式化，非翻译文案）。 */
export function formatSlowlogDuration(us: number): string {
  if (!Number.isFinite(us) || us < 0) return '—';
  if (us < 1000) return `${us} µs`;
  if (us < 1_000_000) return `${(us / 1000).toFixed(2)} ms`;
  return `${(us / 1_000_000).toFixed(2)} s`;
}

/** SLOWLOG 的 timestamp 是秒级 unix 时间。 */
export function formatSlowlogTimestamp(ts: number): string {
  if (!Number.isFinite(ts) || ts <= 0) return '—';
  return new Date(ts * 1000).toLocaleString();
}

export function SlowlogPanel({ dbSessionId, invoke = redisCommandInvoke }: SlowlogPanelProps) {
  const { t } = useI18n();
  const { gateWrite, gateDialog } = useRedisGate();
  const [state, setState] = useState<SlowlogPanelState>('loading');
  const [rows, setRows] = useState<SlowlogRow[]>([]);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [resetOpen, setResetOpen] = useState(false);
  const [resetBusy, setResetBusy] = useState(false);

  const load = useCallback(async () => {
    setState('loading');
    setErrorText(null);
    try {
      const entries = (await invoke('redis', 'slowlog_get', {
        dbSessionId,
        count: SLOWLOG_PANEL_COUNT,
      })) as OverviewSlowlogEntry[];
      setRows(buildSlowlogRows(entries, SLOWLOG_PANEL_COUNT));
      setState(resolveSlowlogState(entries, null));
    } catch (e) {
      setRows([]);
      setErrorText(e instanceof Error ? e.message : String(e));
      setState(resolveSlowlogState(null, e));
    }
  }, [dbSessionId, invoke]);

  useEffect(() => {
    void load();
  }, [load]);

  const handleReset = useCallback(async () => {
    // SLOWLOG RESET 是服务端写操作：先过 Safe Mode 门闸（I-6）。
    if (!(await gateWrite('write-op', 'SLOWLOG RESET'))) return;
    setResetBusy(true);
    try {
      await invoke('redis', 'slowlog_reset', { dbSessionId, confirm: true });
      setResetOpen(false);
      await load();
    } catch (e) {
      setErrorText(e instanceof Error ? e.message : String(e));
      setState('failed');
    } finally {
      setResetBusy(false);
    }
  }, [dbSessionId, invoke, load, gateWrite]);

  const emptyCopy =
    state === 'empty'
      ? t('redis.slowlogEmpty')
      : state === 'unauthorized'
        ? t('redis.overview.slowlog.unauthorized')
        : t('common.failed');

  return (
    <div
      className="flex min-h-0 flex-1 flex-col"
      data-testid="redis-slowlog-panel"
      data-slowlog-state={state}
      data-row-count={rows.length}
    >
      <div className="flex shrink-0 items-center gap-2 border-b border-edge bg-surface-alt px-3 py-2">
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs"
          data-testid="redis-slowlog-refresh"
          disabled={state === 'loading'}
          onClick={() => void load()}
        >
          <RefreshCw className="h-3.5 w-3.5" />
          {t('redis.refresh')}
        </Button>
        <Button
          variant="secondary"
          className="h-7 gap-1 px-2 text-xs text-danger hover:text-danger"
          data-testid="redis-slowlog-reset"
          disabled={state === 'loading'}
          onClick={() => setResetOpen(true)}
        >
          <Trash2 className="h-3.5 w-3.5" />
          {t('redis.resetSlowlog')}
        </Button>
      </div>

      {state === 'loading' ? (
        <div
          className="flex flex-1 items-center justify-center gap-2 text-xs text-fg-muted"
          data-testid="redis-slowlog-loading"
        >
          <Spinner size="md" />
          {t('common.loading')}
        </div>
      ) : rows.length === 0 ? (
        <div
          className="flex flex-1 flex-col items-center justify-center gap-1 px-6 text-center"
          data-testid="redis-slowlog-empty"
          data-i18n-key={
            state === 'empty'
              ? 'redis.slowlogEmpty'
              : state === 'unauthorized'
                ? 'redis.overview.slowlog.unauthorized'
                : 'common.failed'
          }
        >
          <p className="text-xs text-fg-secondary">{emptyCopy}</p>
          {state === 'unauthorized' ? (
            <p className="max-w-[420px] text-[11px] text-fg-muted">
              {t('redis.overview.slowlog.unauthorizedHint')}
            </p>
          ) : null}
          {state === 'failed' ? <p className="text-[11px] text-fg-muted">{errorText}</p> : null}
        </div>
      ) : (
        <div className="min-h-0 flex-1 overflow-auto">
          <table className="w-full border-collapse text-[13px]">
            <thead className="sticky top-0 bg-surface-alt">
              <tr className="border-b border-edge text-left text-xs text-fg-secondary">
                <th className="px-3 py-2 font-medium">{t('redis.slowlogId')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogTimestamp')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogDuration')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogCommand')}</th>
                <th className="px-3 py-2 font-medium">{t('redis.slowlogClient')}</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <tr
                  key={row.id}
                  className="border-b border-edge/60 align-top"
                  data-testid="redis-slowlog-row"
                  data-slowlog-id={row.id}
                >
                  <td className="px-3 py-2 font-mono text-xs text-fg-secondary">{row.id}</td>
                  <td className="px-3 py-2 text-xs text-fg-secondary">
                    {formatSlowlogTimestamp(row.timestamp)}
                  </td>
                  <td
                    className="px-3 py-2 font-mono text-xs text-fg"
                    data-slowlog-duration-us={row.durationUs}
                  >
                    {formatSlowlogDuration(row.durationUs)}
                  </td>
                  <td className="max-w-[420px] px-3 py-2 font-mono text-xs text-fg">
                    <span className="break-all">{row.commandSummary || '—'}</span>
                  </td>
                  <td className="px-3 py-2 text-xs text-fg-muted">
                    {row.client ?? t('redis.overview.slowlog.noClient')}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      <Dialog
        open={resetOpen}
        title={t('redis.confirmResetSlowlog')}
        description={t('redis.confirmResetSlowlogMessage')}
        onClose={() => {
          if (!resetBusy) setResetOpen(false);
        }}
        footer={
          <>
            <Button
              variant="secondary"
              className="h-8 px-3 text-xs"
              data-testid="redis-slowlog-reset-cancel"
              onClick={() => setResetOpen(false)}
              disabled={resetBusy}
            >
              {t('common.cancel')}
            </Button>
            <Button
              variant="primary"
              className="h-8 px-3 text-xs"
              data-testid="redis-slowlog-reset-confirm"
              onClick={() => void handleReset()}
              disabled={resetBusy}
            >
              {t('common.confirm')}
            </Button>
          </>
        }
      >
        <p className="text-xs text-fg-secondary">{t('redis.slowlogReset')}</p>
      </Dialog>
      {gateDialog}
    </div>
  );
}
