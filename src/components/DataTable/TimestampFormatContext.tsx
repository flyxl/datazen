import { createContext, useContext, type ReactNode } from 'react';
import type {
  TimestampFormatContextValue,
  TimestampFormattingConfig,
} from '../../types/timestamp_format';

/**
 * 网格渲染路径上的时间戳格式化上下文（GitHub issue #32 的 Wave 11 契约）。
 *
 * ## 为什么用上下文，而不是一路传 prop
 *
 * `CellRenderer` 在树的第 4 层（`TableView → DataTable → VirtualBody → GridCell →
 * CellRenderer`）。配置与覆盖要到达它，逐层传 prop 就要改 4 个文件的 props
 * 契约 —— 而其中三个文件（`DataTable.tsx` / `VirtualBody.tsx` / `GridCell.tsx`）
 * **同时是 05 册的落点**，多占一个就多一份冲突面。
 *
 * ⇒ 用上下文：渲染侧只读上下文，装配侧（`TableView`）只负责挂 Provider。
 * 两条轨因此**不碰同一个文件**。
 *
 * ## 契约
 *
 * 本文件的**路径、导出名、props 形状**是契约的一部分：任何轨都不得改名、
 * 不得移动、不得改 props 形状。
 */
const TimestampFormatContext = createContext<TimestampFormatContextValue>({
  config: { global: { kind: 'iso', enabled: false }, rules: [] },
  overrides: [],
});

export interface TimestampFormatProviderProps {
  config: TimestampFormattingConfig;
  overrides: TimestampFormatContextValue['overrides'];
  children: ReactNode;
}

/** 提供格式化配置与表级覆盖。 */
export function TimestampFormatProvider({
  config,
  overrides,
  children,
}: TimestampFormatProviderProps) {
  return (
    <TimestampFormatContext.Provider value={{ config, overrides }}>
      {children}
    </TimestampFormatContext.Provider>
  );
}

/** 读取格式化配置与表级覆盖。 */
export function useTimestampFormatContext(): TimestampFormatContextValue {
  return useContext(TimestampFormatContext);
}
