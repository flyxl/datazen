import { useEffect } from 'react';
import { listenCrossWindow } from '../lib/crossWindowBus';
import { useSettingsStore } from '../stores/settingsStore';

/**
 * Keep this window's settingsStore in sync when another window saves settings.
 *
 * T11（设计 §6）：`datazen:settings-changed` 只是通知，不携带可信快照——
 * 接收端不信任广播 payload（它可能是发送窗口保存时刻的旧值：发送
 * 窗口的本地基底在保存前未必最新），而是重读权威配置
 * （`loadSettings`：代次校验丢弃过期响应、刷新失败保留最后一份有效
 * 快照）。同窗口从保存的权威响应更新，不依赖广播。通知本身在桌面
 * 经 Tauri 事件（全部窗口）、Web 经 BroadcastChannel（全部标签页）
 * 到达，见 crossWindowBus / 平台 events 端口；本 hook 不新增广播适配。
 */
export function useSettingsSync() {
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;

    void listenCrossWindow('datazen:settings-changed', () => {
      if (cancelled) return;
      // 通知只触发重读；payload 内容不作为新快照。
      void useSettingsStore.getState().loadSettings();
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {
        // Not available (tests / non-Tauri)
      });

    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
