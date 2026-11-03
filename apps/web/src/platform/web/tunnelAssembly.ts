/**
 * web 宿主的 tunnel 注入面（WR-F5，TS 半边）。
 *
 * 隧道机制已在 modules 的 tunnel feature 共用（桌面/Web 同一份实现）；web
 * 宿主只注入三样**来源**，与 `modules/src/tunnel/web.rs` 的 Rust 注入点同形：
 *
 * - **存储根**：会话内存。裁决（与 Rust 半边一致）：web 形态隧道定义的
 *   系统记录在服务端（对应桌面 `{appData}/tunnels.json` 的服务端半边）；
 *   浏览器侧不用 OPFS —— 隧道域不在 web HTTP 总账上（`get_tunnels` /
 *   `save_tunnel` 等是登记在案的 unsupported 方法），内存根只是端口
 *   「可装配、可同语义测试」的会话面，落盘归服务端，绝不造第二份真相。
 * - **凭据**：SecretRef（`{domain, key}` 引用，永不传值）。解析半边在装配
 *   边界拒绝坏引用；取值半边在安全域客户端接线前如实抛命名缺口
 *   （沿 WR-F1#2 的登记语义，`WR-F5#1`）。
 * - **监听点**：恒为服务端回环 `127.0.0.1:<port>`（浏览器不开本地套接字；
 *   与桌面「驱动永远连 127.0.0.1:<local_port>」不变式同形）。
 *
 * 仅注入来源：本文件零隧道机制、零形态探测（编译期同形纪律照旧）。
 */

/** WR-F5 命名缺口（TS 半边）：web 的隧道凭据解析器走服务端安全域，未接线。 */
export const WEB_TUNNEL_SECRET_RESOLVER_GAP =
  'WebTunnelCredentialSource has no web resolver yet: the security-domain route for ' +
  'server-side secret management is not wired. Wiring gap WR-F5#1 (沿 WR-F1#2 登记语义).';

/** 监听点裁决的常量面：本地腿只属于服务端回环。 */
export const WEB_TUNNEL_LOOPBACK_HOST = '127.0.0.1';

/** 存储根裁决的常量面：web 注入点默认内存语义（见模块文档，非 OPFS）。 */
export const WEB_TUNNEL_STORAGE_ROOT_SEMANTICS = 'memory' as const;

/** 隧道凭据引用：域 + 键。坏引用在装配边界被拒，绝不变成一次出站请求。 */
export interface TunnelSecretRef {
  domain: 'tunnel';
  key: string;
}

/**
 * SecretRef 解析半边：只收 `tunnel` 域的非空键。
 * 与 `parseSecretRef`（AI 域 stub）同构，域面换成隧道。
 */
export function parseTunnelSecretRef(input: unknown): TunnelSecretRef {
  const candidate = input as Partial<TunnelSecretRef> | null;
  if (
    candidate !== null &&
    typeof candidate === 'object' &&
    candidate.domain === 'tunnel' &&
    typeof candidate.key === 'string' &&
    candidate.key.length > 0
  ) {
    return { domain: 'tunnel', key: candidate.key };
  }
  throw new Error(
    'parseTunnelSecretRef: expected { domain: "tunnel", key: string } — credentials ' +
      'cross the boundary as references, never as values.',
  );
}

/** 隧道定义的存储形状（浏览器会话面；字段与 `SavedTunnel` 的非凭据投影对齐）。 */
export interface SessionTunnelDefinition {
  readonly id: string;
  readonly name: string;
  readonly kind: 'ssh' | 'httpProxy' | 'websocket';
  /** 凭据以引用进存储根 —— 明文永不落浏览器（端口两档语义：名单不带密钥）。 */
  readonly credential: TunnelSecretRef;
}

/** web 宿主注入束（浏览器半边）：三样来源一次铸齐。 */
export interface WebTunnelBindings {
  /** 存储根（内存语义）；`list` 只给摘要，`get` 给全量 —— 两档与桌面同契约。 */
  readonly storageRoot: {
    readonly semantics: typeof WEB_TUNNEL_STORAGE_ROOT_SEMANTICS;
    list(): readonly SessionTunnelDefinition[];
    get(tunnelId: string): SessionTunnelDefinition | undefined;
    save(tunnel: SessionTunnelDefinition): void;
    delete(tunnelId: string): void;
  };
  /** 凭据：按引用取值；安全域接线前如实抛命名缺口。 */
  readonly credentials: {
    resolve(ref: TunnelSecretRef): Promise<string>;
  };
  /** 监听点：恒为服务端回环。 */
  readonly listenPoint: {
    readonly host: typeof WEB_TUNNEL_LOOPBACK_HOST;
    bindTarget(port: number): { host: string; port: number };
  };
}

/** 装配入口：铸 web 宿主的隧道注入三件套（每次装配一个会话根）。 */
export function createWebTunnelBindings(): WebTunnelBindings {
  // 存储根 = 内存（裁决见模块文档）：按 id upsert、名单档即存储本身（该档
  // 形状本就不含明文凭据，最小暴露由类型保证）。
  const definitions = new Map<string, SessionTunnelDefinition>();
  return {
    storageRoot: {
      semantics: WEB_TUNNEL_STORAGE_ROOT_SEMANTICS,
      list: () => [...definitions.values()],
      get: (tunnelId) => definitions.get(tunnelId),
      save: (tunnel) => {
        definitions.set(tunnel.id, tunnel);
      },
      delete: (tunnelId) => {
        definitions.delete(tunnelId);
      },
    },
    credentials: {
      async resolve(ref) {
        parseTunnelSecretRef(ref);
        throw new Error(
          `${WEB_TUNNEL_SECRET_RESOLVER_GAP} (ref: ${ref.domain}/${ref.key})`,
        );
      },
    },
    listenPoint: {
      host: WEB_TUNNEL_LOOPBACK_HOST,
      bindTarget: (port) => ({ host: WEB_TUNNEL_LOOPBACK_HOST, port }),
    },
  };
}
