/**
 * Host Extension Points Core — 特权扩展点核心基础设施
 *
 * 为 DataZen 提供松耦合、类型安全的进程内特权扩展机制。
 * 核心功能模块（如 SQLEditor Pro）通过定义 ExtensionPoint<T> 声明扩展契约，
 * 宿主消费端始终通过该机制获取实现（未注入时自动降级到默认基础版）。
 *
 * 依据根目录 LICENSE 的 DataZen Plugin and Extension Linking Exception，
 * 通过本接口链接的独立模块不受宿主 GPL-3.0 协议传染。
 */

export interface ExtensionPoint<T> {
  /** 扩展点唯一标识（如 'editor.sql.pro'） */
  readonly id: string;
  /** 扩展点人类可读名称 */
  readonly name: string;
  /** 扩展点详细说明 */
  readonly description?: string;
  /** 获取默认宿主实现（无插件/降级时调用） */
  getDefault(): T;
}

export interface CreateExtensionPointOptions<T> {
  id: string;
  name: string;
  description?: string;
  getDefault: () => T;
}

/**
 * 创建一个特权扩展点定义
 */
export function createExtensionPoint<T>(
  options: CreateExtensionPointOptions<T>,
): ExtensionPoint<T> {
  return Object.freeze({
    id: options.id,
    name: options.name,
    description: options.description,
    getDefault: options.getDefault,
  });
}

/**
 * 特权扩展点注册中心
 */
export class ExtensionRegistry {
  private implementations = new Map<string, unknown>();
  private defaultFallbacks = new Map<string, unknown>();
  private listeners = new Map<string, Set<() => void>>();
  private globalListeners = new Set<() => void>();

  /**
   * 注册扩展点实现
   * @param point 扩展点契约
   * @param impl 增强实现实例
   * @returns 退订/反注册函数
   */
  register<T>(point: ExtensionPoint<T>, impl: T): () => void {
    this.implementations.set(point.id, impl);
    this.notify(point.id);
    return () => {
      this.unregister(point);
    };
  }

  /**
   * 显式反注册扩展点实现
   */
  unregister<T>(point: ExtensionPoint<T>): void {
    if (this.implementations.has(point.id)) {
      this.implementations.delete(point.id);
      this.notify(point.id);
    }
  }

  /**
   * 获取当前生效的扩展实现。若未注册则返回默认基础版（结果具备引用稳定性）。
   */
  get<T>(point: ExtensionPoint<T>): T {
    const impl = this.implementations.get(point.id);
    if (impl !== undefined) {
      return impl as T;
    }
    let fallback = this.defaultFallbacks.get(point.id);
    if (fallback === undefined) {
      fallback = point.getDefault();
      this.defaultFallbacks.set(point.id, fallback);
    }
    return fallback as T;
  }

  /**
   * 检查指定扩展点是否已激活增强实现
   */
  isEnhanced(point: ExtensionPoint<unknown>): boolean {
    return this.implementations.has(point.id);
  }

  /**
   * 订阅指定扩展点的变更事件（注册/反注册）
   */
  subscribe(pointId: string, listener: () => void): () => void {
    let set = this.listeners.get(pointId);
    if (!set) {
      set = new Set();
      this.listeners.set(pointId, set);
    }
    set.add(listener);

    return () => {
      const currentSet = this.listeners.get(pointId);
      if (currentSet) {
        currentSet.delete(listener);
        if (currentSet.size === 0) {
          this.listeners.delete(pointId);
        }
      }
    };
  }

  /**
   * 订阅所有扩展点的注册变更事件
   */
  subscribeAll(listener: () => void): () => void {
    this.globalListeners.add(listener);
    return () => {
      this.globalListeners.delete(listener);
    };
  }

  /**
   * 获取所有已激活增强实现的扩展点 ID 列表
   */
  getAllRegisteredIds(): string[] {
    return Array.from(this.implementations.keys());
  }

  /**
   * 重置注册中心（主要用于单元测试与状态清理）
   */
  reset(): void {
    const keys = Array.from(this.implementations.keys());
    this.implementations.clear();
    this.defaultFallbacks.clear();
    for (const key of keys) {
      this.notify(key);
    }
  }

  private notify(pointId: string): void {
    const specificListeners = this.listeners.get(pointId);
    if (specificListeners) {
      for (const listener of specificListeners) {
        try {
          listener();
        } catch (e) {
          console.error(`[ExtensionRegistry] Error notifying listener for ${pointId}:`, e);
        }
      }
    }

    for (const listener of this.globalListeners) {
      try {
        listener();
      } catch (e) {
        console.error('[ExtensionRegistry] Error notifying global listener:', e);
      }
    }
  }
}

/** 全局单例扩展点注册中心 */
export const extensionRegistry = new ExtensionRegistry();
