/**
 * valuePresentation 集合注册中心（issue-32 设计 §3）。
 *
 * 现有 ExtensionRegistry 每个扩展点 ID 只保存一个实现（后注册覆盖），
 * 不能满足多展示器并存；本注册中心按贡献 ID 存储集合，与单实现注册
 * 互不为对方的真源。语义要求：
 *
 * - 重复贡献 ID 显式拒绝（抛错，不覆盖已有实现）；
 * - 卸载句柄绑定单次注册：旧句柄不能误删重新注册后的新实例；
 * - 注册、卸载均触发订阅，listener 异常被隔离；
 * - revision 在每次成功 register/unregister 后递增，宿主以之令展示
 *   计划缓存失效；未发生变更时读取值稳定。
 */

import type { ValuePresentationContribution } from './contracts';

/** 字段展示贡献集合注册中心（由宿主持有，frontend 消费）。 */
export interface ValuePresentationRegistry {
  /**
   * 注册一个展示贡献；id 已存在时抛错（不覆盖）。
   * @returns 卸载句柄：只移除本次注册的实例，重复调用为 no-op。
   */
  register(contribution: ValuePresentationContribution): () => void;
  /** 按 id 显式反注册；未知 id 为 no-op（不通知、不递增 revision）。 */
  unregister(id: string): void;
  /** 获取指定 id 的贡献；不存在时返回 undefined。 */
  get(id: string): ValuePresentationContribution | undefined;
  /** 当前全部贡献的快照数组（与内部状态隔离，按注册顺序）。 */
  list(): readonly ValuePresentationContribution[];
  /** 订阅注册/卸载变更；@returns 退订函数。 */
  subscribe(listener: () => void): () => void;
  /** 贡献修订号：每次成功 register/unregister 递增。 */
  readonly revision: number;
}

/**
 * 单次注册的内部记录。
 *
 * 句柄按此对象身份比对而不是按 contribution 引用：即使同一个
 * contribution 对象被反注册后再次注册，旧句柄也不会误删新注册。
 */
interface ValuePresentationRegistration {
  readonly contribution: ValuePresentationContribution;
}

/** 集合注册中心实现。 */
export class ValuePresentationRegistryImpl implements ValuePresentationRegistry {
  private registrations = new Map<string, ValuePresentationRegistration>();
  private listeners = new Set<() => void>();
  private revisionCounter = 0;

  get revision(): number {
    return this.revisionCounter;
  }

  register(contribution: ValuePresentationContribution): () => void {
    if (this.registrations.has(contribution.id)) {
      throw new Error(
        `[ValuePresentationRegistry] duplicate contribution id '${contribution.id}': ` +
          'a value presentation with this id is already registered; ' +
          'unregister it first — registration never overwrites',
      );
    }
    const registration: ValuePresentationRegistration = { contribution };
    this.registrations.set(contribution.id, registration);
    this.changed();
    return () => {
      // 句柄只认自己这一次注册：id 当前映射的仍是本句柄的记录时才移除，
      // 否则（已卸载或已被重新注册）静默 no-op。
      if (this.registrations.get(contribution.id) === registration) {
        this.registrations.delete(contribution.id);
        this.changed();
      }
    };
  }

  unregister(id: string): void {
    if (!this.registrations.has(id)) {
      return;
    }
    this.registrations.delete(id);
    this.changed();
  }

  get(id: string): ValuePresentationContribution | undefined {
    return this.registrations.get(id)?.contribution;
  }

  list(): readonly ValuePresentationContribution[] {
    return Array.from(this.registrations.values(), (entry) => entry.contribution);
  }

  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  /** 重置注册中心（测试与进程内状态清理用）。 */
  reset(): void {
    const hadContributions = this.registrations.size > 0;
    this.registrations.clear();
    if (hadContributions) {
      this.changed();
    }
  }

  /** 唯一的变更入口：递增修订号并通知全部订阅者。 */
  private changed(): void {
    this.revisionCounter += 1;
    this.notify();
  }

  private notify(): void {
    // 遍历快照：listener 中再订阅/退订不会改变本轮通知集合。
    // 单个 listener 抛错只记录，不中断其它 listener，也不反向影响注册流程
    // （写法对齐 ExtensionRegistry.notify）。
    for (const listener of [...this.listeners]) {
      try {
        listener();
      } catch (e) {
        console.error('[ValuePresentationRegistry] Error notifying listener:', e);
      }
    }
  }
}

/** 全局单例注册中心（宿主进程内唯一，由 frontend 消费）。 */
export const valuePresentationRegistry = new ValuePresentationRegistryImpl();
