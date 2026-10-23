import { formatTimestamp } from './formatters';
import type {
  ResolvedTimestampFormat,
  TimestampFormatEngine,
  TimestampFormatQuery,
  TimestampFormatSetting,
} from '../types/timestamp_format';

/**
 * 时间戳格式化引擎的**默认实现**（GitHub issue #32 的 Wave 11 契约）。
 *
 * ## 这个文件为什么在契约里
 *
 * Wave 11 有四条轨并行，它们都要用「引擎」这个概念：规则引擎轨实现它、
 * 设置轨与列头轨消费它、存储轨只管覆盖数据。如果引擎的实现分散在各轨的
 * 文件里，四条轨就会互相等编译 —— 并行度就没了。
 *
 * ⇒ 所以：**引擎的路径、导出名与接口形状是契约的一部分**，本文件先落地一个
 * **默认实现**，四条轨都从这里接。轨道 A 之后会把它的内部换成真实现
 * （规则引擎 + 三级优先级 + `iso-tz` / `locale`），但**不得改名、不得移动、
 * 不得改导出名** —— 别的轨按路径 import 它。
 *
 * ## 默认实现 = 今天的逐字行为
 *
 * issue 的 Backwards Compatibility 要求 opt-in：既有连接与既有设置**逐字节
 * 不变**。所以在真实现接上之前：
 *
 * - `resolve` 返回全局设置（不做任何匹配）—— 因为没有规则、也没有覆盖；
 * - `format` 委托给 `src/lib/formatters.ts` 的 `formatTimestamp` —— 它就是今天
 *   网格里那一句，包括它对**没有时区标识的墙钟文本原样返回**的语义
 *   （`new Date()` 会把墙钟文本按本地时区解释，于是每个单元格被偏移一个本地
 *   UTC 偏移量，还会拼上一个假的 `Z`）。这条语义是有理由的，**不许**在加
 *   时区支持时被顺手改掉。
 */
export const timestampFormatEngine: TimestampFormatEngine = {
  resolve(query: TimestampFormatQuery): ResolvedTimestampFormat {
    return { setting: query.config.global, source: 'global' };
  },
  format(value: unknown, _setting: TimestampFormatSetting): string {
    return formatTimestamp(value);
  },
};
