/**
 * Desktop adapter for the `@datazen/backend-client` ports (form-split F2: moved
 * from `src/platform/tauriBackendTransport.ts`; the dialog/clipboard part of the
 * port implementation now lives in `platformServices.ts`). The thin re-export
 * shim at the original path keeps every importer unchanged until F6.
 *
 * This file, and only this file, is where `@tauri-apps/api/core` meets the
 * backend boundary. `packages/backend-client` is transport-agnostic by design
 * (architecture guard F-07), so the `invoke` calls, the `Channel` wiring and
 * the snake_case command names all live on the host side of the port.
 *
 * Nothing here decides anything. Each `MethodMap` key maps to a Tauri command
 * by name; payload and response shapes come from `MethodMap` in
 * `packages/backend-client/src/transport.ts`. That is the whole point — a
 * second backend (HTTP, in-memory) replaces this file and nothing above it.
 */

import { invoke, Channel } from '@tauri-apps/api/core';
import {
  createBackendClient,
  setBackendClient,
  type BackendClient,
  type EventEnvelope,
  type ConnectionEvent,
  type ApiErrorPayload,
  deserializeApiError,
  type BackendTransport,
  type MethodMap,
  type UnsupportedMethodKind,
} from '@datazen/backend-client';

/**
 * The `backendId` every desktop build reports.
 *
 * It is a label on the facade, not a routing argument: `useBackendClient()`
 * resolves the same instance whichever backend was selected, and a browser
 * backend would register under its own id without changing a single call site.
 */
export const DESKTOP_BACKEND_ID = 'desktop';

/**
 * Map a `MethodMap` key to its Tauri command name.
 *
 * Keys are written camelCase for the kernel surface and snake_case for the
 * legacy driver-command gateway, so only the former is converted. Tauri maps
 * the names in both directions, so this is a naming convention and not a
 * lookup table.
 *
 * Form-split F4 (ruling #2, step 4): the override table is GONE — the kernel
 * track and the host command names are 1:1. Every rename the table used to
 * carry (`get_shared_settings`, `platform_list_profiles`,
 * `read_artifact_chunk`, `get_table_data_at_target`, ...) was resolved on the
 * Rust side. W8 domain convergence kept that property for all but the
 * query-assets verbs and the workflow family, whose host commands predate
 * the contract's vocabulary — see `COMMAND_ALIASES` below.
 */
/**
 * W8 域收敛后方法名与既有 IPC 命令名不一致的少数几条（其余收敛方法的
 * camelCase → snake_case 纯约定转换恰好等于既有命令名——桌面命令先在，
 * 域方法名随契约冻结，此处登记的是仅剩的词序差）。
 *
 * workflow 域的十一条全部在此：桌面命令名是 `workflow_*` 前缀形
 * （`workflow_list`），契约方法名是动词前置形（`listWorkflows`）——
 * 词序差是历史命名（前缀聚类）与契约命名（动词先行）两代约定的分界，
 * 不是收敛时的疏忽。
 */
const COMMAND_ALIASES: Partial<Record<keyof MethodMap, string>> = {
  listQueryFavorites: 'get_favorite_queries',
  createQueryFavorite: 'add_favorite_query',
  deleteQueryFavorite: 'delete_favorite_query',
  listWorkflows: 'workflow_list',
  getWorkflow: 'workflow_get',
  getWorkflowYaml: 'workflow_get_yaml',
  saveWorkflow: 'workflow_save',
  saveWorkflowYaml: 'workflow_save_yaml',
  deleteWorkflow: 'workflow_delete',
  executeWorkflow: 'workflow_execute',
  reloadWorkflows: 'workflow_reload',
  listWorkflowRuns: 'workflow_history_list',
  getWorkflowRun: 'workflow_history_get',
  clearWorkflowRuns: 'workflow_history_clear',
};

function toCommandName(method: keyof MethodMap): string {
  if (method.includes('_')) return method;
  return (
    COMMAND_ALIASES[method] ?? method.replace(/[A-Z]/g, (letter) => `_${letter.toLowerCase()}`)
  );
}

/**
 * W8 刀3a/刀3b 收口域（queryAssets + schemaDiff）：payload 是业务字段 +
 * `submissionToken` 的平铺袋，IPC 轨不需要幂等回执（进程内信任），剥掉
 * 令牌后按命名参数袋转发——与 FLAT_ARGUMENT_METHODS 同一 flat 语义，
 * 单独成集是因为它们还多一道剥令牌。
 */
const CONVERGED_DOMAIN_FLAT_METHODS: ReadonlySet<keyof MethodMap> = new Set<keyof MethodMap>([
  'listQueryFavorites',
  'createQueryFavorite',
  'deleteQueryFavorite',
  'getQueryHistoryPage',
  'deleteQueryHistory',
  'clearQueryHistory',
  'purgeHistory',
  'listSchemaDiffProfiles',
  'saveSchemaDiffProfile',
  'deleteSchemaDiffProfile',
  'compareTableSchemas',
  'prepareSchemaDiffPlan',
  'prepareSchemaUnifiedPlan',
  'executeSchemaDiffDeploy',
  'listSchemaDiffJobs',
  'getSchemaDiffJobDetails',
  'cancelSchemaDiffDeploy',
  'verifySchemaDiffJobRecovery',
  // W8 刀3c/3d/3e web 收口（workflow / dashboard / dataSync）。saveWorkflow /
  // saveDashboard / saveSyncProfile 的 `{ workflow }` / `{ dashboard }` /
  // `{ profile }` 袋剥令牌后与 IPC 命名参数同形；createWidgetFrom*
  // 顶层 dashboardId 是 web 路径插值的副本，Tauri 只取声明的 params 键。
  'listWorkflows',
  'getWorkflow',
  'getWorkflowYaml',
  'saveWorkflow',
  'saveWorkflowYaml',
  'deleteWorkflow',
  'executeWorkflow',
  'reloadWorkflows',
  'listWorkflowRuns',
  'getWorkflowRun',
  'clearWorkflowRuns',
  'listDashboards',
  'getDashboard',
  'saveDashboard',
  'deleteDashboard',
  'setDashboardRefreshPaused',
  'findDashboardWorkflowRefs',
  'createWidgetFromSql',
  'createWidgetFromWorkflow',
  'updateHiddenWidgetSql',
  'runDashboardWidget',
  'listWidgetRuns',
  'getWidgetRun',
  'getMonitorScheduleGrant',
  'authorizeMonitorSchedule',
  'revokeMonitorSchedule',
  'getSyncTasks',
  'deleteSyncTask',
  'getSyncProfiles',
  'saveSyncProfile',
  'deleteSyncProfile',
  'classifyDataSyncPair',
  'listMigrationRuns',
  'getMigrationRun',
  'listDataSyncJobs',
  'getDataSyncJob',
  'getDataSyncJobDetails',
  'getDataSyncJobPreview',
  'cancelDataSync',
  'inspectDataSync',
  'revalidateDataSync',
  'compareDataSync',
  'startDataSyncPrepareJob',
  'startDataSyncApplyJob',
  'executeDataSync',
  'generateDataSyncSql',
  'checkSyncConflicts',
  'verifyDataSyncRecovery',
]);

/**
 * 剥掉 HTTP 写面的幂等令牌：令牌是传输层词汇（HTTP 面 CSRF/幂等回执的
 * 载体），不该泄进宿主参数袋——多余字段对命令无害，但令牌不是业务事实。
 */
function stripSubmissionToken(payload: unknown): unknown {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) {
    return payload;
  }
  const { submissionToken: _ipcRailNeedsNoReceipt, ...rest } = payload as Record<string, unknown>;
  return rest;
}

/**
 * Whether a `MethodMap` key names a legacy host command (snake_case rail).
 *
 * The legacy rail's names are lowercase — `connect`, `get_connections` — while
 * every kernel name is camelCase (`listConnections`, `getJob`). The earlier
 * test was "the name contains an underscore", which classified the two
 * single-word legacy commands (`connect`, `disconnect`) as kernel methods: their
 * payload was wrapped in `{ request }`, the host reported
 * `command connect missing required key connectionId`, and opening a connection
 * from the navigator failed with a red error banner while the IPC path in the
 * specs kept working. Casing is the discriminator the two rails actually agree
 * on, so use it.
 */
function isLegacyHostCommand(method: keyof MethodMap): boolean {
  return !/[A-Z]/.test(method);
}

/**
 * Methods whose payload *is* the command's argument bag.
 *
 * The kernel-surface commands take a single `request` argument, so their
 * payload is nested under `request`. The commands below take **named**
 * arguments instead (`context`, `changes`, `plan`, `scope`, ...), so their
 * payload is the bag itself and nesting it would make the Rust side look for
 * a `request` field that does not exist.
 */
const FLAT_ARGUMENT_METHODS: ReadonlySet<keyof MethodMap> = new Set<keyof MethodMap>([
  'previewPendingChanges',
  'commitPendingChanges',
  'estimateTableRows',
  'listNamedViews',
  'saveNamedView',
  'deleteNamedView',
]);

/**
 * The table-data methods the desktop backend does not serve.
 *
 * The port registers every method the product has, so a method landing later
 * cannot slip past it unnoticed. This list is the other half of that
 * arrangement: it says which of them the *desktop* cannot serve today, so a
 * caller gets an explicit `CapabilityUnsupported` — with the method name and
 * the backend — instead of Tauri's bare "command not found".
 *
 * It is **empty**: the desktop registers every table-data command the port
 * declares, the three named-view commands of 08 included. A browser build has
 * no host store and therefore declares the named views itself, as a
 * `capability` gap — "this form factor can never serve them" is a different
 * fact from "the desktop has not implemented them yet", and the two must not
 * be reported as one.
 *
 * `src/platform/__tests__/tauriBackendTransport.tableData.test.ts`
 * cross-checks this list against the host's registration, so the day a
 * command is registered, the test fails and asks for this entry to go — which
 * is what happened to `resolve_foreign_key_targets` when 07 landed, to
 * `estimate_table_rows` when 09 landed, and to the three named-view commands
 * when 08 landed.
 */
const DESKTOP_UNSUPPORTED_METHODS: Partial<Record<keyof MethodMap, UnsupportedMethodKind>> = {};

/**
 * Build the desktop transport.
 *
 * The request object *is* the Tauri argument bag: its own fields are passed
 * through as the command's named arguments. That is what keeps every migrated
 * call byte-identical to the `invoke` it replaced — `call('execute_driver_command',
 * { request })` reaches Rust as `{ request }`, not as `{ payload: { request } }`,
 * because Tauri deserializes argument names, not an envelope.
 *
 * Channel deliveries are adapted to an iterator with independent subscription cleanup.
 */
export function createDesktopBackendTransport(): BackendTransport {
  return {
    unsupportedMethods: DESKTOP_UNSUPPORTED_METHODS,
    async call<K extends keyof MethodMap>(
      method: K,
      payload: MethodMap[K]['request'],
    ): Promise<MethodMap[K]['response']> {
      const command = toCommandName(method);
      const args =
        method === 'getPlatformIdentity'
          ? {}
          : CONVERGED_DOMAIN_FLAT_METHODS.has(method)
            ? stripSubmissionToken(payload)
            : isLegacyHostCommand(method) ||
                method === 'getJob' ||
                method === 'listJobs' ||
                FLAT_ARGUMENT_METHODS.has(method)
              ? payload
              : { request: payload ?? {} };
      // The request's own fields are the command's named arguments, so this
      // cast is a widening to Tauri's argument bag, not a shape change: the
      // object handed to `invoke` is the same object `call` received.
      //
      // A command that fails with a **plain string** — the shape
      // `CommandError::Serialize` produces — keeps its message here: the
      // backend client turns an unreadable payload into a generic sentence,
      // and the `grid.*` classification reads the prefix of that message. The
      // host is the only place where the raw value still exists.
      let response: MethodMap[K]['response'];
      try {
        response = (await invoke(
          command,
          args as Record<string, unknown>,
        )) as MethodMap[K]['response'];
      } catch (error) {
        throw typeof error === 'string' ? new Error(error) : error;
      }
      if (method === 'readArtifact' && response && typeof response === 'object') {
        const chunk = response as { bytes?: number[] | Uint8Array };
        if (Array.isArray(chunk.bytes)) chunk.bytes = new Uint8Array(chunk.bytes);
      }
      return response;
    },
    subscribe(_method, request) {
      return {
        [Symbol.asyncIterator]() {
          const subscriptionId = crypto.randomUUID();
          type Message =
            | { kind: 'event'; event: EventEnvelope<ConnectionEvent> }
            | { kind: 'closed' }
            | { kind: 'error'; error: ApiErrorPayload };
          const channel = new Channel<Message>();
          const queue: EventEnvelope<ConnectionEvent>[] = [];
          let finished = false;
          let failure: unknown = null;
          let wake: (() => void) | null = null;
          const signal = () => {
            wake?.();
            wake = null;
          };
          channel.onmessage = (message) => {
            if (finished) return;
            if (message.kind === 'event') {
              if (queue.length >= 2048) {
                failure = new Error('Event buffer overflow; restore execution and reconnect.');
                finished = true;
              } else queue.push(message.event);
            } else {
              finished = true;
              if (message.kind === 'error') failure = deserializeApiError(message.error);
            }
            signal();
          };
          const started = invoke<void>('subscribe_events', {
            request,
            subscriptionId,
            onEvent: channel,
          }).catch((error: unknown) => {
            failure = error;
            finished = true;
            signal();
          });
          let stopped = false;
          return {
            async next(): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> {
              while (!queue.length && !finished)
                await new Promise<void>((resolve) => {
                  wake = resolve;
                });
              if (failure) throw failure;
              const value = queue.shift();
              return value ? { value, done: false } : { value: undefined, done: true };
            },
            async return(): Promise<IteratorResult<EventEnvelope<ConnectionEvent>>> {
              finished = true;
              queue.length = 0;
              signal();
              if (stopped) return { value: undefined, done: true };
              stopped = true;
              await started;
              await invoke<void>('stop_event_subscription', { subscriptionId });
              return { value: undefined, done: true };
            },
          };
        },
      };
    },
  };
}

/**
 * Bind the desktop backend and select it.
 *
 * Called by the app entry before first paint, and lazily by the
 * transitional driver-sdk path. Idempotent by construction: a second call
 * replaces the binding, so tests can rebind freely.
 */
export function bindDesktopBackend(backendId: string = DESKTOP_BACKEND_ID): BackendClient {
  const client = createBackendClient(backendId, createDesktopBackendTransport());
  setBackendClient(backendId, client);
  return client;
}
