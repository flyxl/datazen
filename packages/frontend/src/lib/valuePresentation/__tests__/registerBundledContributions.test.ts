/**
 * registerBundledValuePresentationContributions（issue-32 T06）：核心内置
 * 展示贡献的启动注册。
 *
 * 覆盖：注册一次；两次调用幂等（不抛错、不重复注册、revision 只递增一次）；
 * 注销后可重注册；无参调用注册到全局单例。
 */
import { describe, expect, it } from 'vitest';
import {
  ValuePresentationRegistryImpl,
  valuePresentationRegistry,
} from '@datazen/extension-points';
import { registerBundledValuePresentationContributions } from '../registerBundledContributions';
import { TIMESTAMP_CONTRIBUTION_ID, timestampContribution } from '../timestampContribution';

describe('registerBundledValuePresentationContributions', () => {
  it('registers the timestamp contribution once', () => {
    const registry = new ValuePresentationRegistryImpl();
    registerBundledValuePresentationContributions(registry);
    expect(registry.get(TIMESTAMP_CONTRIBUTION_ID)).toBe(timestampContribution);
    expect(registry.list()).toHaveLength(1);
  });

  it('is idempotent: a second call neither throws nor duplicates', () => {
    const registry = new ValuePresentationRegistryImpl();
    registerBundledValuePresentationContributions(registry);
    const revisionAfterFirst = registry.revision;
    expect(() => registerBundledValuePresentationContributions(registry)).not.toThrow();
    expect(registry.list()).toHaveLength(1);
    expect(registry.get(TIMESTAMP_CONTRIBUTION_ID)).toBe(timestampContribution);
    expect(registry.revision).toBe(revisionAfterFirst);
  });

  it('can register again after the contribution is unregistered', () => {
    const registry = new ValuePresentationRegistryImpl();
    registerBundledValuePresentationContributions(registry);
    registry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    expect(registry.get(TIMESTAMP_CONTRIBUTION_ID)).toBeUndefined();
    registerBundledValuePresentationContributions(registry);
    expect(registry.get(TIMESTAMP_CONTRIBUTION_ID)).toBe(timestampContribution);
    expect(registry.list()).toHaveLength(1);
  });

  it('registers into the global singleton when no registry is given', () => {
    try {
      registerBundledValuePresentationContributions();
      expect(valuePresentationRegistry.get(TIMESTAMP_CONTRIBUTION_ID)).toBe(timestampContribution);
      // 幂等：无参再调一次也不抛错、不重复。
      expect(() => registerBundledValuePresentationContributions()).not.toThrow();
      expect(valuePresentationRegistry.list()).toHaveLength(1);
    } finally {
      valuePresentationRegistry.unregister(TIMESTAMP_CONTRIBUTION_ID);
    }
  });
});
