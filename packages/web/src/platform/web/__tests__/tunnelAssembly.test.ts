import { describe, expect, it } from 'vitest';

import type { SecurityClient } from '@datazen/backend-client';

import {
  WEB_TUNNEL_LOOPBACK_HOST,
  WEB_TUNNEL_SECRET_RESOLVER_GAP,
  WEB_TUNNEL_STORAGE_ROOT_SEMANTICS,
  createWebTunnelBindings,
  parseTunnelSecretRef,
} from '../tunnelAssembly';

/**
 * WR-F5 桌面/Web 同隧道语义测试（web 半边）。
 *
 * 桌面同语义锚点：
 * - `crates/datazen-desktop/src/tunnel/mod.rs`（`resolve_kind_*`、
 *   `start_for_connection` 的注入面）与 `crates/datazen/src/tunnel/web.rs` 的
 *   Rust 注入点测试（两档 + upsert + 凭据缺口）；
 * - 本文件钉 web 宿主（浏览器半边）的**注入来源**三件套：存储根=内存、
 *   凭据=SecretRef+命名缺口、监听点=服务端回环。机制零份（共用 modules）。
 */

describe('web tunnel injection sources (WR-F5)', () => {
  it('storage root keeps the desktop contract: memory semantics, upsert, two tiers without plaintext credentials', () => {
    const bindings = createWebTunnelBindings();

    // 裁决常量面：内存语义（非 OPFS —— 服务端是系统记录，见模块文档）。
    expect(bindings.storageRoot.semantics).toBe(WEB_TUNNEL_STORAGE_ROOT_SEMANTICS);
    expect(bindings.storageRoot.semantics).toBe('memory');

    // upsert：同 id 第二次保存覆盖，不产生第二条。
    bindings.storageRoot.save({
      id: 't1',
      name: 'Prod Bastion',
      kind: 'ssh',
      credential: { domain: 'tunnel', key: 'bastion-pass' },
    });
    bindings.storageRoot.save({
      id: 't1',
      name: 'Renamed',
      kind: 'ssh',
      credential: { domain: 'tunnel', key: 'bastion-pass' },
    });
    expect(bindings.storageRoot.list()).toHaveLength(1);
    expect(bindings.storageRoot.get('t1')?.name).toBe('Renamed');

    // 名单档最小暴露：存储形状只带凭据**引用**，无明文字段。
    const rendered = JSON.stringify(bindings.storageRoot.list());
    expect(rendered).toContain('"domain":"tunnel"');
    expect(rendered).not.toContain('"password"');

    // delete 幂等落空：删除后 get 回 None。
    bindings.storageRoot.delete('t1');
    expect(bindings.storageRoot.get('t1')).toBeUndefined();
    expect(bindings.storageRoot.list()).toHaveLength(0);
  });

  it('credentials resolve as SecretRef with the registered gap; the listen point is always the server loopback', async () => {
    const bindings = createWebTunnelBindings();

    // 引用半边：坏引用在装配边界被拒。
    expect(parseTunnelSecretRef({ domain: 'tunnel', key: 'bastion-pass' })).toEqual({
      domain: 'tunnel',
      key: 'bastion-pass',
    });
    expect(() => parseTunnelSecretRef({ domain: 'tunnel', key: '' })).toThrowError(
      /references, never as values/,
    );
    expect(() => parseTunnelSecretRef({ domain: 'ai-provider', key: 'k' })).toThrowError();
    expect(() => parseTunnelSecretRef(null)).toThrowError();

    // 取值半边（未注入 SecurityClient 的装配）：注入缺失 = 命名缺口
    // （WR-F5#1，沿 WR-F1#2 的闭合语义）—— 如实失败，绝不静默空答；
    // 错误只带引用坐标，不带密钥坐标以外的内容。
    await expect(
      bindings.credentials.resolve({ domain: 'tunnel', key: 'bastion-pass' }),
    ).rejects.toThrowError(WEB_TUNNEL_SECRET_RESOLVER_GAP);
    await expect(
      bindings.credentials.resolve({ domain: 'tunnel', key: 'bastion-pass' }),
    ).rejects.toThrowError(/tunnel\/bastion-pass/);

    // 监听点：唯一形态是服务端回环 —— 浏览器不开本地套接字。
    expect(bindings.listenPoint.host).toBe(WEB_TUNNEL_LOOPBACK_HOST);
    expect(bindings.listenPoint.bindTarget(15432)).toEqual({
      host: '127.0.0.1',
      port: 15432,
    });
  });

  it('credentials resolve through the injected security client (WR-F5#1 closed along WR-F1#2)', async () => {
    // 假 SecurityClient：记下请求体，按冻结契约回 { value }。
    const seen: { domain: string; key: string }[] = [];
    const security: SecurityClient = {
      async resolveSecret(request) {
        seen.push({ domain: request.domain, key: request.key });
        return { value: 'plaintext-only-to-the-principal' };
      },
      async putSecret() {
        throw new Error('fake security client: putSecret is unreachable in the tunnel tests');
      },
    };

    const bindings = createWebTunnelBindings(security);
    await expect(
      bindings.credentials.resolve({ domain: 'tunnel', key: 'bastion-pass' }),
    ).resolves.toBe('plaintext-only-to-the-principal');

    // 冻结契约的引用坐标逐字入体：{ domain, key }，无其它字段。
    expect(seen).toEqual([{ domain: 'tunnel', key: 'bastion-pass' }]);

    // 坏引用依旧在装配边界被拒（不出站）。
    await expect(
      bindings.credentials.resolve({ domain: 'ai-provider', key: 'k' } as never),
    ).rejects.toThrowError(/references, never as values/);
    expect(seen).toHaveLength(1);
  });
});
