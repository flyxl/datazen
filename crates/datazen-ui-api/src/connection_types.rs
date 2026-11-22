//! Connection-facing types — ported from React
//! `packages/frontend/src/types/connection.ts` (tasklist T128, 279 lines).
//!
//! That file is the frontend's connection facade: four groups of re-exports
//! from the driver/backend packages plus its own interface declarations, and
//! the app imports the whole vocabulary through it (and, identically, through
//! `types/index` — the React file's own NOTE records that the two copies must
//! not drift; new fields belong to the `./index` pair of each shape). This
//! module is that facade's Rust home: the connection form's
//! [`ConnectionConfig`] with its tunnel vocabulary, the catalog metadata
//! shapes ([`TableInfo`], [`TableSchema`], …), the DB-07 foreign-key
//! resolution contracts, explain plans, query history and the AI context
//! entry. Pure contracts — no IO, no state — so the module compiles for
//! wasm32 exactly like its `chart_types` sibling.
//!
//! ## Dependency inventory (the four re-export groups)
//!
//! | React re-export | Rust side found | decision here |
//! |---|---|---|
//! | `DatabaseType` ← `extensions/generated` | none: `generated.ts` is a gitignored resolve-drivers artifact (a build's driver set as a literal union — tasklist T095 unlanded); driver-api already settled `pub type DatabaseType = String` (`types.rs`), and `connection_views` kept the string | thin mirror: string alias |
//! | `SslMode`, `DatabaseObjectKind` ← `@datazen/extension-points` | driver-api root exports the same five-literal `SslMode`; `schema_objects::ObjectKind` is the superset (adds `table`/`view`) under connection-view's local name — and this crate's `connection_views::ObjectKind` already mirrors the same five literals | thin mirrors under the facade's names |
//! | `SshAuthMethod`, `TunnelKind`, `HttpProxyTunnelConfig`, `WebSocketTunnelConfig`, `SavedTunnel` ← `@datazen/driver-sdk` | driver-api root exports all of them (`tunnel_types.rs`); `SshAuthMethod` exists nowhere in Rust (driver-api's `SshTunnelConfig.authMethod` is a plain `String`) | thin mirrors |
//! | `Value` ← `@datazen/backend-client` | driver-api's `Value` is the richer untagged enum (`Null/Bool/Integer/Float/String/Bytes/Timestamp/Json`); the TS name is exactly its untagged JSON image (`string | number | boolean | null | object | array`) | thin mirror: alias of `serde_json::Value` |
//! | `ColumnInfo`, `QueryResult`, `MultiQueryResult`, `StatementResult`, `QueryStreamEvent` ← `@datazen/backend-client` | driver-api carries close shapes, but the TS set interlocks around `StatementResult`'s provenance/binding legs (`ResultProvenance`, `SessionContext`, `ExecutionTarget`, …) and `QueryStreamEvent`'s artifact legs | deferred — not referenced by any own interface below; the backend-client query domain ports land as their own module and the facade re-exports them then |
//!
//! Everything Rust-side was found but is still mirrored here — not `use`-d —
//! because `datazen-driver-api` is a **native-only** dependency of this crate
//! (`[target.'cfg(not(target_arch = "wasm32"))']`) while the facade itself
//! must compile for wasm32 (`scripts/build-rune-ui-web.sh` checks
//! `-p datazen-ui --target wasm32-unknown-unknown --lib`). Each mirror
//! therefore documents its native twin and keeps the same wire shape, so a
//! native adapter can convert losslessly — and if the mirrors drift, the
//! doc trails are the audit path.
//!
//! `DatabaseType` specifically: React's union is **build-injected** (the
//! resolve-drivers codegen emits `export type DatabaseType = 'postgres' |
//! … | 'never'` per SKU), so no closed Rust enum can be right for every
//! build. The string alias is the honest mirror and the established one
//! (driver-api `types.rs`, `connection_views.rs`); switch to the canonical
//! mirror when T095 lands the generated registry.
//!
//! ## Mirror rules applied
//!
//! - `interface` → struct, camelCase serde (this is a wire-boundary family —
//!   IPC DTOs, like `chart_types`/`ai_sessions`, unlike the `metadata`
//!   family whose identity keys preclude serde).
//! - A string-literal union → Rust enum whose `as_str` pins the exact React
//!   literal and whose serde representation **is** that literal. Inline
//! unions with no exported React name get one: `HttpProxyTunnelConfig.scheme`
//! → [`HttpProxyScheme`], `WebSocketTunnelConfig.mode` →
//! [`WebSocketMode`], `ForeignKeyInfo.deferrability` →
//! [`ForeignKeyDeferrability`], `SavedTunnel.kind`'s
//! `Exclude<TunnelKind, 'none'>` → [`SavedTunnelKind`].
//! - `x?: T` (no `null` in the union) → `Option<T>` +
//!   `skip_serializing_if = "Option::is_none"`: absent stays absent.
//! - `x: T | null` (required) **and** `x?: T | null` (optional *and*
//!   nullable) → `Option<T>` with **no** skip: `None` serializes as an
//!   explicit `null` and the key stays present, and a missing key still
//!   deserializes (serde reads absent `Option` fields as `None`). The
//!   `?:`-plus-`null` fields exist precisely because the backend twin emits
//!   `null` — the React file's own `TableInfo` comment records that `?:`
//!   alone "silently rejects the backend's own payload" — so the null shape
//!   is kept on the wire ([`TableInfo::schema`], [`TableInfo::row_count`],
//!   [`DatabaseObject::schema`], [`PrivilegeGrant::object_schema`],
//!   [`ColumnSchema::default_value`], [`QueryHistoryEntry::schema`], the
//!   request's nullable filters, [`ForeignKeyTargetRef::schema`]).
//! - JS `number`: durations/timeouts/limits/counts/sizes → unsigned widths
//!   (`u16` port, `u32` timeouts and pool sizes, `u64` ms/rows/limits/size);
//!   catalog row counts stay `i64` (the driver-api twin width, and catalog
//!   estimates may be negative); plan `cost` is a double (`f64`).
//! - `unknown` / `Record<string, unknown>` → `serde_json::Value` /
//!   `serde_json::Map` (the crate's `unknown` rule, `chart_types`
//!   `sampleValues` precedent).
//! - No `Default` impls: the React file declares no initial constants — the
//!   form and store ports own the defaults (driver-api's native
//!   `ConnectionConfig` defaults `connectionTimeout`/`maxPoolSize` are its
//!   own layer's truth, not this facade's).
//!
//! ## Dedupe with already-landed ports
//!
//! - `ContextKind`/`ContextItem` are **not redefined here**: the facade
//!   duplicates them from `types/index.ts`, and their Rust home is already
//!   `crate::context_items` (the `lib/contextItems.ts` port documents that
//!   its module is the home for exactly this duplicate). The crate root
//!   exports them from there.
//! - `connection_views::ObjectKind` mirrors the same five literals this
//!   module's [`DatabaseObjectKind`] carries (`connection-view.ts` declared
//!   the union locally under the shorter name). The two stay distinct —
//!   unifying them belongs to the consumer that bridges views to objects,
//!   the move `relation_prediction::from_table_schema` made for its twins.
//! - `SavedTunnelSshConfig` (driver-sdk's `SavedTunnel.ssh` leg) is
//!   field-identical to [`SshTunnelConfig`] — the TS declaration exists for
//!   a package boundary this module does not have — so [`SavedTunnel`]
//!   reuses [`SshTunnelConfig`] directly.
//! - `ColumnSchema`/`IndexInfo`/`ForeignKeyInfo`/`TableSchema` at the crate
//!   root now carry **these** full facade shapes: the previously root-exposed
//!   same-named prediction slices (`relation_prediction`) were narrow
//!   stopgaps with no root-path consumer, and stay reachable inside their
//!   module.
//! - **The index.ts reconciliation (T211)**: `types/index.ts` is the file
//!   the app actually imports, and its own NOTE — mirrored in
//!   `types/connection.ts` — records that the two files' duplicated shapes
//!   "must not drift" and that "new fields belong in `./index` only". Two
//!   pairs had drifted when T211 landed: `DriverCapabilities` (the index
//!   copy carries `supportsOffset`/`hasSchemaLevel`, which the kernel's own
//!   wire struct also serializes) and `DatabaseObject` (the index copy
//!   carries the driver-api twin's `signature`/`targetSchema`/
//!   `targetName`). The single Rust definitions here were **enriched to the
//!   authoritative index.ts shapes** — one definition, re-exported by the
//!   aggregate (`crate::types`), never forked; see each struct's docs.
//!
//! The React file has no test suite of its own (its tests live with the
//! connection commands/stores, other tasks); the pins below are the port's
//! own: every literal table, the null/absent two-state of every optional
//! family, the recursion the tree shapes promise, and the camelCase wire.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Re-export group 1: `DatabaseType` from `extensions/generated`.
// ---------------------------------------------------------------------------

/// Database engine identifier for the current build (React: the
/// build-injected literal union from the gitignored `generated.ts`).
///
/// A closed Rust enum cannot be right for every SKU — the driver set varies
/// per build — so the alias is the mirror, matching driver-api's own
/// `DatabaseType` decision and `connection_views`'. Switch to the canonical
/// mirror when T095 lands the generated registry.
pub type DatabaseType = String;

// ---------------------------------------------------------------------------
// Re-export group 2: `SslMode`, `DatabaseObjectKind`
// from `@datazen/extension-points`.
// ---------------------------------------------------------------------------

/// React `SslMode = 'disable' | 'prefer' | 'require' | 'verifyCa' |
/// 'verifyFull'` — the TLS posture a connection asks for.
///
/// Thin mirror of driver-api's root-exported `SslMode` (same five literals,
/// same camelCase wire); mirrored here because that crate is a native-only
/// dependency and this module compiles for wasm32.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SslMode {
    Disable,
    Prefer,
    Require,
    VerifyCa,
    VerifyFull,
}

impl SslMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Disable => "disable",
            Self::Prefer => "prefer",
            Self::Require => "require",
            Self::VerifyCa => "verifyCa",
            Self::VerifyFull => "verifyFull",
        }
    }
}

/// React `DatabaseObjectKind = 'function' | 'procedure' | 'trigger' |
/// 'sequence' | 'type'` — the non-relation catalog objects a driver can
/// list/open.
///
/// Thin mirror of extension-points' union. Driver-api's
/// `schema_objects::ObjectKind` is the superset (it adds `table`/`view` for
/// relation-scoped work) and this crate's `connection_views::ObjectKind`
/// carries the same five literals under connection-view's local name; the
/// facade's own name and closed set are mirrored here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum DatabaseObjectKind {
    Function,
    Procedure,
    Trigger,
    Sequence,
    Type,
}

impl DatabaseObjectKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Function => "function",
            Self::Procedure => "procedure",
            Self::Trigger => "trigger",
            Self::Sequence => "sequence",
            Self::Type => "type",
        }
    }
}

// ---------------------------------------------------------------------------
// Re-export group 3 (the connection half): `SshAuthMethod` and `Value`.
// The query-result half of this group (`ColumnInfo`, `QueryResult`,
// `MultiQueryResult`, `StatementResult`, `QueryStreamEvent`) is deferred —
// see the module docs' inventory table.
// ---------------------------------------------------------------------------

/// React `SshAuthMethod = 'password' | 'private_key' | 'agent'` — how an SSH
/// tunnel leg authenticates.
///
/// Exists nowhere in Rust yet (driver-api's native `SshTunnelConfig` spells
/// `authMethod` as a plain `String` with a `"password"` serde default), so
/// this mirror is the canonical closed union until that side narrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SshAuthMethod {
    Password,
    PrivateKey,
    Agent,
}

impl SshAuthMethod {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Password => "password",
            Self::PrivateKey => "private_key",
            Self::Agent => "agent",
        }
    }
}

/// React `Value` (`@datazen/backend-client`): `string | number | boolean |
/// null | Record<string, unknown> | unknown[]` — one cell of a result row.
///
/// That union is the untagged JSON image of driver-api's richer `Value`
/// enum once it crosses the wire, so the alias is the exact mirror (the
/// crate's `unknown` rule). Cells may be `null`: rows are
/// `Vec<Vec<Option<Value>>>` at their consumers.
pub type Value = serde_json::Value;

// ---------------------------------------------------------------------------
// Re-export group 4: `TunnelKind`, `HttpProxyTunnelConfig`,
// `WebSocketTunnelConfig`, `SavedTunnel` from `@datazen/driver-sdk`.
// ---------------------------------------------------------------------------

/// React `TunnelKind = 'none' | 'ssh' | 'httpProxy' | 'websocket'` — the
/// preferred tunnel strategy of a connection.
///
/// Thin mirror of driver-api's root-exported `TunnelKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TunnelKind {
    None,
    Ssh,
    HttpProxy,
    // camelCase would spell this `webSocket`; the React literal is flat.
    #[serde(rename = "websocket")]
    WebSocket,
}

impl TunnelKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ssh => "ssh",
            Self::HttpProxy => "httpProxy",
            Self::WebSocket => "websocket",
        }
    }
}

/// React `HttpProxyTunnelConfig.scheme`'s inline union `'http' | 'https'` —
/// transport to the proxy itself. React declares it inline with no exported
/// name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HttpProxyScheme {
    Http,
    Https,
}

impl HttpProxyScheme {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Http => "http",
            Self::Https => "https",
        }
    }
}

/// React `WebSocketTunnelConfig.mode`'s inline union `'datazen_v1' |
/// 'raw_binary'` — the relay protocol. React declares it inline with no
/// exported name; the mirror gives it one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSocketMode {
    DatazenV1,
    RawBinary,
}

impl WebSocketMode {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DatazenV1 => "datazen_v1",
            Self::RawBinary => "raw_binary",
        }
    }
}

/// React `HttpProxyTunnelConfig` — an HTTP CONNECT proxy tunnel (local
/// listener → proxy CONNECT → remote DB). Thin mirror of driver-api's
/// root-exported struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpProxyTunnelConfig {
    pub enabled: bool,
    pub host: String,
    /// Proxy port (driver-api twin width).
    pub port: u16,
    /// Transport to the proxy itself.
    pub scheme: HttpProxyScheme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    /// Extra headers for the CONNECT handshake. `BTreeMap` so two equal
    /// configs serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_secs: Option<u32>,
}

/// React `WebSocketTunnelConfig` — a WebSocket relay tunnel (local listener
/// → WS relay → remote DB). Thin mirror of driver-api's root-exported
/// struct.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebSocketTunnelConfig {
    pub enabled: bool,
    /// Full URL, e.g. `wss://relay.example.com/v1/tunnel`.
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_token: Option<String>,
    /// Extra headers for the upgrade request. `BTreeMap` so two equal
    /// configs serialize byte-identically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub headers: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connect_timeout_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ping_interval_secs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<WebSocketMode>,
}

/// React `SavedTunnel.kind`'s `Exclude<TunnelKind, 'none'>` — a stored
/// tunnel is always one of the three real strategies, never `none`.
/// React spells the narrowing inline; the mirror gives it a name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SavedTunnelKind {
    Ssh,
    HttpProxy,
    // camelCase would spell this `webSocket`; the React literal is flat.
    #[serde(rename = "websocket")]
    WebSocket,
}

impl SavedTunnelKind {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ssh => "ssh",
            Self::HttpProxy => "httpProxy",
            Self::WebSocket => "websocket",
        }
    }
}

/// React `SavedTunnel` — an independently persisted tunnel definition
/// (`tunnels.json`); connections reference it via
/// [`ConnectionConfig::tunnel_id`].
///
/// Thin mirror of driver-api's root-exported struct. The TS `ssh` leg is
/// `SavedTunnelSshConfig`, documented there as field-identical to
/// `SshTunnelConfig` (the split was a package boundary); this module has
/// no boundary, so [`SshTunnelConfig`] is the leg directly.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedTunnel {
    pub id: String,
    pub name: String,
    pub kind: SavedTunnelKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh: Option<SshTunnelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy: Option<HttpProxyTunnelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket: Option<WebSocketTunnelConfig>,
}

// ---------------------------------------------------------------------------
// The file's own interfaces, in the React file's declaration order.
// ---------------------------------------------------------------------------

/// React `SshTunnelConfig` — one SSH tunnel leg with its optional
/// ProxyJump hop.
///
/// Frontend shape: `authMethod` is the closed [`SshAuthMethod`] union (the
/// native driver-api twin spells it a defaulted `String`) and the optional
/// credentials are `?:` (absent stays absent). `jump` recurses.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SshTunnelConfig {
    pub enabled: bool,
    pub host: String,
    /// SSH port (driver-api twin width).
    pub port: u16,
    pub username: String,
    pub auth_method: SshAuthMethod,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub private_key_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub passphrase: Option<String>,
    /// Optional first hop (ProxyJump). Nested `jump` is allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jump: Option<Box<SshTunnelConfig>>,
}

/// React `ConnectionConfig` — the connection form's main shape: what a
/// saved connection is.
///
/// Frontend shape: everything the form does not require is `?:`
/// (absent stays absent). The native driver-api twin is the backend's own
/// DTO — it defaults `connectionTimeout`/`maxPoolSize` and requires
/// `readOnly`/`pinned` — while this facade declares the wire the frontend
/// actually speaks; both parse each other's payloads (serde `Option`
/// accepts absent keys).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub database_type: DatabaseType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// Connection port (driver-api twin width).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub database: Option<String>,
    /// Presto/Trino schema within catalog.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub password: Option<String>,
    pub ssl_mode: SslMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection_timeout: Option<u32>,
    /// Host-injected pool size; not typically set in the connection form.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pool_size: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ssh_tunnel: Option<SshTunnelConfig>,
    /// Preferred tunnel strategy. When absent, inferred from legacy
    /// ssh/http/ws fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_kind: Option<TunnelKind>,
    /// Reference to a [`SavedTunnel`] in `tunnels.json`. When set, the
    /// runtime resolves tunnel fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub http_proxy_tunnel: Option<HttpProxyTunnelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub websocket_tunnel: Option<WebSocketTunnelConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color_tag: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_connected_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_version: Option<String>,
    /// Opaque per-driver connection options (e.g. Redis topology/TLS).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<serde_json::Map<String, Value>>,
    /// When true, the host rejects mutating SQL and row edits.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_only: Option<bool>,
    /// When true, sorted first within the connection group in the
    /// navigator.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pinned: Option<bool>,
}

/// React `ServerInfo` — the server a live session is talking to.
///
/// Thin content twin of driver-api's `ServerInfo`; kept here as the
/// facade's own declaration because the frontend file declares it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerInfo {
    pub server_version: String,
    pub server_type: String,
}

/// React `DriverCapabilities` — capabilities reported by the concrete
/// driver for a live database session.
///
/// T128 ported the four-boolean copy `types/connection.ts` declares;
/// `types/index.ts` (the file the app actually imports, and whose own NOTE
/// — "keep new fields in `./index` only" — makes it the authoritative
/// pair) carries six: `supportsOffset` and `hasSchemaLevel` were added
/// there first. The single Rust definition is the enriched six-field shape
/// (T211's reconciliation — one definition, no fork): the kernel's own wire
/// struct (`driver_registry::DriverCapabilities`, the thing that actually
/// serializes) carries all six plus `hasMultiDatabase`, which the frontend
/// view leaves out and serde ignores on decode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DriverCapabilities {
    pub supports_cancel_query: bool,
    /// True only when the driver implements opaque execution-handle cancel.
    pub supports_query_execution_cancel: bool,
    pub supports_explain: bool,
    pub supports_streaming_results: bool,
    /// Whether the dialect accepts `OFFSET` in pagination (`false` for
    /// Presto/Hive-family engines, which paginate with `LIMIT` only).
    pub supports_offset: bool,
    /// Whether the engine has a real second namespace level (`schema`).
    pub has_schema_level: bool,
}

/// React `TableType = 'table' | 'view' | 'materializedView' |
/// 'systemTable'` — the relation kinds a catalog lists.
///
/// Thin mirror of driver-api's root-exported `TableType` (same four
/// literals, same camelCase wire).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TableType {
    Table,
    View,
    MaterializedView,
    SystemTable,
}

impl TableType {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materializedView",
            Self::SystemTable => "systemTable",
        }
    }
}

/// React `DatabaseObject` — one non-relation catalog object a driver
/// lists/opens.
///
/// Frontend-narrowed twin of driver-api's `DatabaseObject` (which spells
/// `kind` a plain `String`); `schema` is `?: string | null` because the
/// backend twin emits `null` when the engine has no schema level.
///
/// T128 ported the three-field copy `types/connection.ts` declares;
/// `types/index.ts` (the authoritative pair — see [`DriverCapabilities`]'
/// docs) carries the driver-api twin's three extra legs: the PostgreSQL
/// routine `signature` that disambiguates overloads, and a trigger's
/// `targetSchema`/`targetName` relation. The single Rust definition is
/// the enriched shape (T211's reconciliation). The three legs mirror the
/// twin's own `skip_serializing_if` wire — absent stays absent — while
/// `null` payloads decode (serde `Option` accepts both states).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DatabaseObject {
    pub kind: DatabaseObjectKind,
    /// Null on the wire (driver-api twin keeps no skip); absent also
    /// accepted.
    pub schema: Option<String>,
    pub name: String,
    /// PostgreSQL routine identity arguments. This disambiguates
    /// overloaded functions/procedures without putting SQL syntax into
    /// `name`. Absent stays absent (the twin's own skip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
    /// Relation attached to a trigger, when exposed by the driver — its
    /// schema leg. Absent stays absent (the twin's own skip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_schema: Option<String>,
    /// Relation attached to a trigger, when exposed by the driver — its
    /// name leg. Absent stays absent (the twin's own skip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_name: Option<String>,
}

/// React `PrivilegeGrant` — one row of a privilege listing.
///
/// `objectSchema` is `?: string | null` — null on the wire from the
/// driver-api twin's `Option<_>` without skip.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrivilegeGrant {
    pub grantee: String,
    /// Null on the wire (driver-api twin keeps no skip); absent also
    /// accepted.
    pub object_schema: Option<String>,
    pub object_name: String,
    pub privilege: String,
}

/// React `TableInfo` — one relation as the navigator's tree lists it.
///
/// The React comment on `schema`/`rowCount` is the facade's own wire
/// lesson: driver-api declares both as `Option<_>` without
/// `skip_serializing_if`, so serde puts `null` on the wire — `?:` alone
/// would silently reject the backend's own payload. The mirror keeps that
/// truth: both keys stay present, `None` serializes as an explicit `null`,
/// and an absent key still deserializes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TableInfo {
    pub name: String,
    /// Null on the wire (see the struct docs); absent also accepted.
    pub schema: Option<String>,
    pub table_type: TableType,
    /// Catalog row count, often an estimate (`i64`: the driver-api twin
    /// width). Null on the wire; absent also accepted.
    pub row_count: Option<i64>,
}

/// React `ColumnSchema` — one column of a table's schema.
///
/// Frontend-narrowed twin of driver-api's `ColumnSchema` (which requires
/// `comment`/`isPrimaryKey`/`isAutoIncrement` and defaults deferrability
/// concerns elsewhere); `defaultValue` is `?: string | null` — the
/// backend twin emits `null` — while the flags and comment are plain `?:`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ColumnSchema {
    pub name: String,
    pub data_type: String,
    pub nullable: bool,
    /// Null on the wire (driver-api twin keeps no skip); absent also
    /// accepted.
    pub default_value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_primary_key: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_auto_increment: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub comment: Option<String>,
}

/// React `IndexInfo` — one index of a table's schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct IndexInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub is_unique: bool,
    pub is_primary: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index_type: Option<String>,
}

/// React `ForeignKeyInfo.deferrability`'s inline union `'unknown' |
/// 'notDeferrable' | 'deferrableInitiallyImmediate' |
/// 'deferrableInitiallyDeferred'` — the constraint's deferral behavior.
/// React declares it inline with no exported name; the mirror gives it
/// one. Content twin of driver-api's `ForeignKeyDeferrability`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ForeignKeyDeferrability {
    Unknown,
    NotDeferrable,
    DeferrableInitiallyImmediate,
    DeferrableInitiallyDeferred,
}

impl ForeignKeyDeferrability {
    /// The wire spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::NotDeferrable => "notDeferrable",
            Self::DeferrableInitiallyImmediate => "deferrableInitiallyImmediate",
            Self::DeferrableInitiallyDeferred => "deferrableInitiallyDeferred",
        }
    }
}

/// React `ForeignKeyInfo` — one declared foreign key of a table's schema.
///
/// Frontend-narrowed twin of driver-api's `ForeignKeyInfo` (which requires
/// `onUpdate`/`onDelete` as strings and defaults `deferrability`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyInfo {
    pub name: String,
    pub columns: Vec<String>,
    pub referenced_table: String,
    pub referenced_columns: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_update: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub on_delete: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deferrability: Option<ForeignKeyDeferrability>,
}

/// React `TableSchema` — a whole table's schema as the DDL/schema panels
/// consume it.
///
/// Frontend-narrowed twin of driver-api's `TableSchema` (which additionally
/// carries `checkConstraints` and `tableOptions` — the metadata family's
/// facts, never rendered by these panels).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TableSchema {
    pub table_name: String,
    pub columns: Vec<ColumnSchema>,
    pub primary_keys: Vec<String>,
    pub indexes: Vec<IndexInfo>,
    pub foreign_keys: Vec<ForeignKeyInfo>,
}

/// React `ForeignKeyTargetRef` — the referenced side of a foreign key, as
/// the host resolved it (DB-07).
///
/// The host is the only authority on where the target lives: the driver's
/// `referenced_table` differs per driver (PostgreSQL prefixes the schema,
/// MySQL and SQLite do not), so the front end must never split that string
/// itself. Field-for-field twin of backend-client's `ForeignKeyTargetRef`
/// — that port is the cross-shape contract, this is the host-side mirror.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyTargetRef {
    /// Target database.
    pub database: String,
    /// Target namespace; `null` when the driver has no schema level
    /// (required-but-nullable: no skip, key stays present).
    pub schema: Option<String>,
    /// Target table.
    pub table: String,
    /// The referenced columns, in the same order as the local columns.
    pub columns: Vec<String>,
    /// The type names of `columns`, in the same order.
    pub data_types: Vec<String>,
    /// The effective primary key of the target table.
    pub primary_keys: Vec<String>,
    /// The referenced columns are covered by a primary key or a unique
    /// index.
    pub unique: bool,
    /// The candidate limit that was actually applied.
    pub candidate_limit: u64,
}

/// React `ForeignKeyTargetResolution` — the result of resolving a foreign
/// key (DB-07).
///
/// `exists: false` means the target table is readable but the referenced
/// row is gone — a successful answer, not an error.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ForeignKeyTargetResolution {
    pub target: ForeignKeyTargetRef,
    pub exists: bool,
    /// Candidate rows: each cell a [`Value`] or `null`.
    pub candidates: Vec<Vec<Option<Value>>>,
    pub truncated: bool,
}

/// React `ResolveForeignKeyTargetsRequest` — the request of a
/// foreign-key resolution (DB-07). Field-for-field twin of
/// backend-client's `ResolveForeignKeyTargetsRequest`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveForeignKeyTargetsRequest {
    pub db_session_id: String,
    /// `?: string | null`: null on the wire when set to `None`; absent
    /// also accepted.
    pub database: Option<String>,
    /// `?: string | null`: null on the wire when set to `None`; absent
    /// also accepted.
    pub schema: Option<String>,
    pub table: String,
    /// `?: string | null`: null on the wire when set to `None`; absent
    /// also accepted.
    pub qualified_table: Option<String>,
    /// The local columns of the foreign key, in pairing order.
    pub columns: Vec<String>,
    /// Disambiguates two constraints over the same columns.
    /// `?: string | null`: null on the wire when set to `None`; absent
    /// also accepted.
    pub constraint_name: Option<String>,
    /// The local column values, positionally paired with `columns`.
    pub values: Vec<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<u64>,
}

/// React `ExplainPlanDetail` — one `key`/`value` line of a plan node's
/// details.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlanDetail {
    pub key: String,
    pub value: String,
}

/// React `ExplainPlanNode` — one node of an explain plan tree; `children`
/// recurses.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainPlanNode {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<i64>,
    pub details: Vec<ExplainPlanDetail>,
    pub children: Vec<ExplainPlanNode>,
}

/// React `ExplainResult` — one explain run's output: the raw text, the
/// parsed JSON, and the host-normalized tree.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplainResult {
    pub plan_text: String,
    /// Opaque driver JSON (the crate's `unknown` rule).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_json: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_tree: Option<ExplainPlanNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_cost: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_rows: Option<i64>,
}

/// React `QueryHistoryEntry` — one executed statement as the history
/// panel lists it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryHistoryEntry {
    pub id: String,
    pub connection_id: String,
    pub database: String,
    /// Schema namespace when known (PG `search_path` is not
    /// session-tracked yet → usually absent). `?: string | null`: null on
    /// the wire when set to `None`; absent also accepted.
    pub schema: Option<String>,
    pub sql: String,
    pub executed_at: String,
    pub execution_time_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows_affected: Option<u64>,
    pub success: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

/// React `ContextEntry` — one entry of an AI context tree (a file, a
/// directory with children, …); `children` recurses.
///
/// The facade's `ContextKind`/`ContextItem` siblings are not redefined
/// here — `crate::context_items` is their documented Rust home.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<ContextEntry>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    // No React test suite backs this file — these tests are the port's
    // own pins: every literal table, the null/absent two-state of every
    // optional family, the recursion the tree shapes promise, and the
    // camelCase wire. No `Default` pins: the file declares no initial
    // constants (see module docs).

    fn decode<T: serde::de::DeserializeOwned>(wire: serde_json::Value) -> T {
        serde_json::from_value(wire)
            .unwrap_or_else(|error| panic!("wire decode failed: {error}"))
    }

    fn encode<T: serde::Serialize>(value: &T) -> serde_json::Value {
        serde_json::to_value(value)
            .unwrap_or_else(|error| panic!("wire encode failed: {error}"))
    }

    #[test]
    fn ssl_mode_keeps_the_react_literals() {
        let table = [
            (SslMode::Disable, "disable"),
            (SslMode::Prefer, "prefer"),
            (SslMode::Require, "require"),
            (SslMode::VerifyCa, "verifyCa"),
            (SslMode::VerifyFull, "verifyFull"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: SslMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn database_object_kind_keeps_the_react_literals() {
        let table = [
            (DatabaseObjectKind::Function, "function"),
            (DatabaseObjectKind::Procedure, "procedure"),
            (DatabaseObjectKind::Trigger, "trigger"),
            (DatabaseObjectKind::Sequence, "sequence"),
            (DatabaseObjectKind::Type, "type"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: DatabaseObjectKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn ssh_auth_method_keeps_the_react_literals() {
        let table = [
            (SshAuthMethod::Password, "password"),
            (SshAuthMethod::PrivateKey, "private_key"),
            (SshAuthMethod::Agent, "agent"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: SshAuthMethod = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn tunnel_kind_keeps_the_react_literals() {
        let table = [
            (TunnelKind::None, "none"),
            (TunnelKind::Ssh, "ssh"),
            (TunnelKind::HttpProxy, "httpProxy"),
            (TunnelKind::WebSocket, "websocket"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: TunnelKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn tunnel_inline_unions_keep_the_react_literals() {
        let schemes = [
            (HttpProxyScheme::Http, "http"),
            (HttpProxyScheme::Https, "https"),
        ];
        for (value, literal) in schemes {
            assert_eq!(value.as_str(), literal);
            let parsed: HttpProxyScheme = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let modes = [
            (WebSocketMode::DatazenV1, "datazen_v1"),
            (WebSocketMode::RawBinary, "raw_binary"),
        ];
        for (value, literal) in modes {
            assert_eq!(value.as_str(), literal);
            let parsed: WebSocketMode = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
        let kinds = [
            (SavedTunnelKind::Ssh, "ssh"),
            (SavedTunnelKind::HttpProxy, "httpProxy"),
            (SavedTunnelKind::WebSocket, "websocket"),
        ];
        for (value, literal) in kinds {
            assert_eq!(value.as_str(), literal);
            let parsed: SavedTunnelKind = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn table_type_keeps_the_react_literals() {
        let table = [
            (TableType::Table, "table"),
            (TableType::View, "view"),
            (TableType::MaterializedView, "materializedView"),
            (TableType::SystemTable, "systemTable"),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: TableType = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn foreign_key_deferrability_keeps_the_react_literals() {
        let table = [
            (ForeignKeyDeferrability::Unknown, "unknown"),
            (ForeignKeyDeferrability::NotDeferrable, "notDeferrable"),
            (
                ForeignKeyDeferrability::DeferrableInitiallyImmediate,
                "deferrableInitiallyImmediate",
            ),
            (
                ForeignKeyDeferrability::DeferrableInitiallyDeferred,
                "deferrableInitiallyDeferred",
            ),
        ];
        for (value, literal) in table {
            assert_eq!(value.as_str(), literal);
            let parsed: ForeignKeyDeferrability = decode(serde_json::json!(literal));
            assert_eq!(parsed, value);
            assert_eq!(encode(&value), serde_json::json!(literal));
        }
    }

    #[test]
    fn database_type_stays_a_plain_string_until_t095_lands() {
        // generated.ts is a per-build literal union; the alias is the
        // honest mirror (driver-api's own decision), so any engine id is
        // legal — including ones this build's registry never listed.
        let engine: DatabaseType = String::from("postgresql");
        let mut config = minimal_connection_config();
        config.database_type = engine;
        assert_eq!(config.database_type, "postgresql");
        config.database_type = "superset".to_owned();
        let wire = encode(&config);
        assert_eq!(wire["databaseType"], "superset");
    }

    #[test]
    fn value_is_the_json_image_of_the_cell_union() {
        // backend-client's Value is string | number | boolean | null |
        // object | array — exactly serde_json::Value.
        let cells: Vec<Value> = decode(serde_json::json!([
            "text",
            7,
            3.25,
            true,
            { "nested": [1, null] },
            [1, "two", null],
        ]));
        assert_eq!(cells.len(), 6);
        assert_eq!(cells[0], serde_json::json!("text"));
        assert_eq!(cells[1], serde_json::json!(7));
        assert_eq!(cells[2], serde_json::json!(3.25));
        assert_eq!(cells[3], serde_json::json!(true));
        assert_eq!(cells[4]["nested"][1], serde_json::Value::Null);
        assert_eq!(cells[5][2], serde_json::Value::Null);
    }

    fn minimal_connection_config() -> ConnectionConfig {
        ConnectionConfig {
            id: "c1".into(),
            name: "Local Postgres".into(),
            database_type: "postgresql".into(),
            host: None,
            port: None,
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::Disable,
            connection_timeout: None,
            max_pool_size: None,
            ssh_tunnel: None,
            tunnel_kind: None,
            tunnel_id: None,
            http_proxy_tunnel: None,
            websocket_tunnel: None,
            color_tag: None,
            group: None,
            last_connected_at: None,
            server_version: None,
            options: None,
            read_only: None,
            pinned: None,
        }
    }

    #[test]
    fn a_minimal_connection_serializes_to_exactly_its_four_required_keys() {
        // Every `?:` field of the form's shape stays absent — the facade's
        // optionality rule: absent stays absent.
        let wire = encode(&minimal_connection_config());
        assert_eq!(
            wire,
            serde_json::json!({
                "id": "c1",
                "name": "Local Postgres",
                "databaseType": "postgresql",
                "sslMode": "disable",
            })
        );
        let back: ConnectionConfig = decode(wire);
        assert_eq!(back, minimal_connection_config());
    }

    #[test]
    fn a_full_connection_round_trips_the_camel_case_wire() {
        let config = ConnectionConfig {
            host: Some("db.example.com".into()),
            port: Some(5432),
            database: Some("app".into()),
            schema: Some("public".into()),
            username: Some("app".into()),
            password: Some("secret".into()),
            ssl_mode: SslMode::VerifyFull,
            connection_timeout: Some(15),
            max_pool_size: Some(4),
            ssh_tunnel: None,
            tunnel_kind: Some(TunnelKind::HttpProxy),
            tunnel_id: Some("t1".into()),
            http_proxy_tunnel: Some(HttpProxyTunnelConfig {
                enabled: true,
                host: "proxy.example.com".into(),
                port: 3128,
                scheme: HttpProxyScheme::Https,
                username: None,
                password: None,
                headers: None,
                connect_timeout_secs: Some(30),
            }),
            websocket_tunnel: None,
            color_tag: Some("#aabbcc".into()),
            group: Some("Prod".into()),
            last_connected_at: Some("2026-01-02T03:04:05Z".into()),
            server_version: Some("16.2".into()),
            options: None,
            read_only: Some(true),
            pinned: Some(false),
            ..minimal_connection_config()
        };
        let wire = encode(&config);
        assert_eq!(wire["port"], 5432);
        assert_eq!(wire["sslMode"], "verifyFull");
        assert_eq!(wire["connectionTimeout"], 15);
        assert_eq!(wire["maxPoolSize"], 4);
        assert_eq!(wire["tunnelKind"], "httpProxy");
        assert_eq!(wire["tunnelId"], "t1");
        assert_eq!(wire["httpProxyTunnel"]["scheme"], "https");
        assert_eq!(wire["httpProxyTunnel"]["connectTimeoutSecs"], 30);
        assert_eq!(wire["colorTag"], "#aabbcc");
        assert_eq!(wire["group"], "Prod");
        assert_eq!(wire["lastConnectedAt"], "2026-01-02T03:04:05Z");
        assert_eq!(wire["serverVersion"], "16.2");
        assert_eq!(wire["readOnly"], true);
        assert_eq!(wire["pinned"], false);
        let back: ConnectionConfig = decode(wire);
        assert_eq!(back, config);
    }

    #[test]
    fn connection_options_carry_opaque_driver_settings() {
        let mut options = serde_json::Map::new();
        options.insert("topology".into(), serde_json::json!({ "mode": "cluster" }));
        let config = ConnectionConfig { options: Some(options), ..minimal_connection_config() };
        let wire = encode(&config);
        assert_eq!(wire["options"]["topology"]["mode"], "cluster");
        let back: ConnectionConfig = decode(wire);
        assert_eq!(
            back.options.as_ref().and_then(|o| o.get("topology")),
            Some(&serde_json::json!({ "mode": "cluster" }))
        );
        // Absent options stay absent.
        assert!(encode(&minimal_connection_config()).get("options").is_none());
    }

    fn ssh_tunnel(jump: Option<Box<SshTunnelConfig>>) -> SshTunnelConfig {
        SshTunnelConfig {
            enabled: true,
            host: "bastion.example.com".into(),
            port: 22,
            username: "deploy".into(),
            auth_method: SshAuthMethod::PrivateKey,
            password: None,
            private_key_path: Some("~/.ssh/id_ed25519".into()),
            passphrase: None,
            jump,
        }
    }

    #[test]
    fn ssh_tunnel_config_round_trips_with_a_recursive_jump_hop() {
        let tunnel = ssh_tunnel(Some(Box::new(ssh_tunnel(None))));
        let wire = encode(&tunnel);
        assert_eq!(wire["authMethod"], "private_key");
        assert_eq!(wire["privateKeyPath"], "~/.ssh/id_ed25519");
        assert_eq!(wire["jump"]["host"], "bastion.example.com");
        // `?:` credentials stay absent when unset.
        assert!(wire.get("password").is_none());
        assert!(wire.get("passphrase").is_none());
        assert!(wire["jump"].get("jump").is_none());
        let back: SshTunnelConfig = decode(wire);
        assert_eq!(back, tunnel);
        // The password union member still round-trips when present.
        let with_password =
            SshTunnelConfig { password: Some("hunter2".into()), ..ssh_tunnel(None) };
        assert_eq!(encode(&with_password)["password"], "hunter2");
    }

    #[test]
    fn saved_tunnels_reference_one_of_the_three_real_strategies() {
        let tunnel = SavedTunnel {
            id: "t1".into(),
            name: "Prod Bastion".into(),
            kind: SavedTunnelKind::Ssh,
            ssh: Some(ssh_tunnel(None)),
            http_proxy: None,
            websocket: None,
        };
        let wire = encode(&tunnel);
        assert_eq!(wire["kind"], "ssh");
        assert_eq!(wire["ssh"]["authMethod"], "private_key");
        // Only one leg is populated; the others stay absent.
        assert!(wire.get("httpProxy").is_none());
        assert!(wire.get("websocket").is_none());
        let back: SavedTunnel = decode(wire);
        assert_eq!(back, tunnel);
    }

    #[test]
    fn server_info_and_driver_capabilities_pin_their_fields() {
        let info = ServerInfo { server_version: "16.2".into(), server_type: "PostgreSQL".into() };
        assert_eq!(
            encode(&info),
            serde_json::json!({ "serverVersion": "16.2", "serverType": "PostgreSQL" })
        );

        // Six required booleans — no optionality anywhere on the surface
        // (the four-boolean shape was `types/connection.ts`'s lagging copy;
        // `types/index.ts`, the authoritative pair, carries all six).
        let caps = DriverCapabilities {
            supports_cancel_query: true,
            supports_query_execution_cancel: false,
            supports_explain: true,
            supports_streaming_results: false,
            supports_offset: true,
            has_schema_level: false,
        };
        assert_eq!(
            encode(&caps),
            serde_json::json!({
                "supportsCancelQuery": true,
                "supportsQueryExecutionCancel": false,
                "supportsExplain": true,
                "supportsStreamingResults": false,
                "supportsOffset": true,
                "hasSchemaLevel": false,
            })
        );
        // The kernel's own wire struct carries a seventh boolean
        // (`hasMultiDatabase`) the frontend view leaves out — decoding it
        // is a noop, not an error.
        let from_kernel: DriverCapabilities = decode(serde_json::json!({
            "supportsCancelQuery": true,
            "supportsQueryExecutionCancel": false,
            "supportsExplain": true,
            "supportsStreamingResults": false,
            "supportsOffset": true,
            "hasSchemaLevel": true,
            "hasMultiDatabase": false,
        }));
        assert_eq!(from_kernel.has_schema_level, true);
    }

    #[test]
    fn table_info_keeps_driver_api_null_on_the_wire() {
        // The React comment's own lesson: the backend twin declares
        // `Option<_>` without skip, so the wire carries `null` — not an
        // absent key. None → explicit nulls, keys present.
        let info = TableInfo {
            name: "users".into(),
            schema: None,
            table_type: TableType::View,
            row_count: None,
        };
        let wire = encode(&info);
        assert_eq!(
            wire,
            serde_json::json!({
                "name": "users",
                "schema": null,
                "tableType": "view",
                "rowCount": null,
            })
        );
        // The backend's own payload — nulls — decodes…
        let from_nulls: TableInfo = decode(serde_json::json!({
            "name": "users",
            "schema": null,
            "tableType": "view",
            "rowCount": null,
        }));
        assert_eq!(from_nulls, info);
        // …and so does a payload with the keys present.
        let full: TableInfo = decode(serde_json::json!({
            "name": "users",
            "schema": "public",
            "tableType": "materializedView",
            "rowCount": 1_234_567,
        }));
        assert_eq!(full.schema.as_deref(), Some("public"));
        assert_eq!(full.row_count, Some(1_234_567));
        assert_eq!(full.table_type, TableType::MaterializedView);
        // An absent key deserializes too (serde Option default).
        let sparse: TableInfo =
            decode(serde_json::json!({ "name": "t", "tableType": "table" }));
        assert_eq!(sparse.schema, None);
        assert_eq!(sparse.row_count, None);
    }

    #[test]
    fn database_objects_and_privilege_grants_keep_nullable_schemas() {
        let object = DatabaseObject {
            kind: DatabaseObjectKind::Function,
            schema: None,
            name: "now".into(),
            signature: None,
            target_schema: None,
            target_name: None,
        };
        let wire = encode(&object);
        // `schema` keeps its explicit null; the three legs the index.ts copy
        // added keep the twin's own skip — absent stays absent.
        assert_eq!(wire, serde_json::json!({ "kind": "function", "schema": null, "name": "now" }));
        let back: DatabaseObject = decode(wire);
        assert_eq!(back, object);

        // The routine/trigger legs ride when the driver fills them: a
        // PostgreSQL overloaded function disambiguated by its signature,
        // and a trigger carrying its relation.
        let routine = DatabaseObject {
            schema: Some("public".into()),
            signature: Some("(integer, text)".into()),
            target_schema: None,
            target_name: None,
            ..object
        };
        let wire = encode(&routine);
        assert_eq!(wire["schema"], "public");
        assert_eq!(wire["signature"], "(integer, text)");
        assert!(wire.get("targetSchema").is_none());
        let back: DatabaseObject = decode(wire);
        assert_eq!(back, routine);
        let trigger = DatabaseObject {
            kind: DatabaseObjectKind::Trigger,
            target_schema: Some("public".into()),
            target_name: Some("users".into()),
            ..routine
        };
        let wire = encode(&trigger);
        assert_eq!(wire["targetSchema"], "public");
        assert_eq!(wire["targetName"], "users");
        let back: DatabaseObject = decode(wire);
        assert_eq!(back, trigger);

        let grant = PrivilegeGrant {
            grantee: "app".into(),
            object_schema: None,
            object_name: "users".into(),
            privilege: "SELECT".into(),
        };
        let wire = encode(&grant);
        assert_eq!(
            wire,
            serde_json::json!({
                "grantee": "app",
                "objectSchema": null,
                "objectName": "users",
                "privilege": "SELECT",
            })
        );
        let back: PrivilegeGrant = decode(wire);
        assert_eq!(back, grant);
    }

    #[test]
    fn column_schema_mixes_the_two_optionality_families() {
        // defaultValue is `?: string | null` → null stays on the wire;
        // the flags and comment are plain `?:` → absent stays absent.
        let column = ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            is_primary_key: None,
            is_auto_increment: None,
            comment: None,
        };
        let wire = encode(&column);
        assert_eq!(
            wire,
            serde_json::json!({
                "name": "id",
                "dataType": "integer",
                "nullable": false,
                "defaultValue": null,
            })
        );
        let full = ColumnSchema {
            default_value: Some("nextval('s')".into()),
            is_primary_key: Some(true),
            is_auto_increment: Some(false),
            comment: Some("the key".into()),
            ..column
        };
        let wire = encode(&full);
        assert_eq!(wire["defaultValue"], "nextval('s')");
        assert_eq!(wire["isPrimaryKey"], true);
        assert_eq!(wire["isAutoIncrement"], false);
        assert_eq!(wire["comment"], "the key");
        let back: ColumnSchema = decode(wire);
        assert_eq!(back, full);
    }

    #[test]
    fn index_info_round_trips_with_an_optional_index_type() {
        let index = IndexInfo {
            name: "users_pkey".into(),
            columns: vec!["id".into()],
            is_unique: true,
            is_primary: true,
            index_type: None,
        };
        let wire = encode(&index);
        assert_eq!(
            wire,
            serde_json::json!({
                "name": "users_pkey",
                "columns": ["id"],
                "isUnique": true,
                "isPrimary": true,
            })
        );
        let typed = IndexInfo { index_type: Some("btree".into()), ..index };
        assert_eq!(encode(&typed)["indexType"], "btree");
        let back: IndexInfo = decode(encode(&typed));
        assert_eq!(back, typed);
    }

    #[test]
    fn plain_optional_fields_still_accept_explicit_nulls_on_input() {
        // The `?:` (skip) family writes absence, but a peer that sends
        // `"comment": null` (or any `null`-valued `?:` key) must still
        // decode: `Option` accepts both states on the way in — only the
        // re-emission differs from the `?: | null` family.
        let column: ColumnSchema = decode(serde_json::json!({
            "name": "id",
            "dataType": "integer",
            "nullable": false,
            "defaultValue": null,
            "isPrimaryKey": null,
            "isAutoIncrement": null,
            "comment": null,
        }));
        assert_eq!(column, ColumnSchema {
            name: "id".into(),
            data_type: "integer".into(),
            nullable: false,
            default_value: None,
            is_primary_key: None,
            is_auto_increment: None,
            comment: None,
        });
        // And the wire the facade itself re-emits is the absent-key one.
        assert!(encode(&column).get("comment").is_none());
        assert!(encode(&column).get("isPrimaryKey").is_none());
    }

    #[test]
    fn foreign_key_info_and_table_schema_round_trip() {
        let fk = ForeignKeyInfo {
            name: "orders_user_fk".into(),
            columns: vec!["user_id".into()],
            referenced_table: "users".into(),
            referenced_columns: vec!["id".into()],
            on_update: None,
            on_delete: Some("CASCADE".into()),
            deferrability: Some(ForeignKeyDeferrability::DeferrableInitiallyDeferred),
        };
        let wire = encode(&fk);
        assert_eq!(wire["referencedTable"], "users");
        assert_eq!(wire["onDelete"], "CASCADE");
        assert_eq!(wire["deferrability"], "deferrableInitiallyDeferred");
        // Plain `?:` actions stay absent.
        assert!(wire.get("onUpdate").is_none());
        let back: ForeignKeyInfo = decode(wire);
        assert_eq!(back, fk);

        let schema = TableSchema {
            table_name: "orders".into(),
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                is_primary_key: Some(true),
                is_auto_increment: None,
                comment: None,
            }],
            primary_keys: vec!["id".into()],
            indexes: Vec::new(),
            foreign_keys: vec![fk],
        };
        let wire = encode(&schema);
        assert_eq!(wire["tableName"], "orders");
        assert_eq!(wire["primaryKeys"], serde_json::json!(["id"]));
        assert_eq!(wire["columns"][0]["isPrimaryKey"], true);
        let back: TableSchema = decode(wire);
        assert_eq!(back, schema);
    }

    #[test]
    fn foreign_key_target_resolution_carries_values_and_nulls() {
        let resolution = ForeignKeyTargetResolution {
            target: ForeignKeyTargetRef {
                database: "app".into(),
                schema: None,
                table: "users".into(),
                columns: vec!["id".into()],
                data_types: vec!["integer".into()],
                primary_keys: vec!["id".into()],
                unique: true,
                candidate_limit: 50,
            },
            exists: true,
            candidates: vec![
                vec![Some(serde_json::json!(7)), None],
                vec![Some(serde_json::json!("seven")), None],
            ],
            truncated: false,
        };
        let wire = encode(&resolution);
        // Required-but-nullable schema → explicit null, key present.
        assert_eq!(wire["target"]["schema"], serde_json::Value::Null);
        assert_eq!(wire["target"]["candidateLimit"], 50);
        assert_eq!(wire["candidates"][0][0], 7);
        assert_eq!(wire["candidates"][0][1], serde_json::Value::Null);
        assert_eq!(wire["candidates"][1][0], "seven");
        assert_eq!(wire["truncated"], false);
        let back: ForeignKeyTargetResolution = decode(wire);
        assert_eq!(back, resolution);
    }

    #[test]
    fn resolve_foreign_key_targets_requests_pin_their_two_state_filters() {
        let request = ResolveForeignKeyTargetsRequest {
            db_session_id: "s1".into(),
            database: None,
            schema: None,
            table: "orders".into(),
            qualified_table: None,
            columns: vec!["user_id".into()],
            constraint_name: None,
            values: vec![serde_json::json!(7), serde_json::json!("seven")],
            limit: None,
        };
        let wire = encode(&request);
        assert_eq!(
            wire,
            serde_json::json!({
                "dbSessionId": "s1",
                "database": null,
                "schema": null,
                "table": "orders",
                "qualifiedTable": null,
                "columns": ["user_id"],
                "constraintName": null,
                "values": [7, "seven"],
            })
        );
        // `limit` is plain `?:` → absent when unset…
        assert!(wire.get("limit").is_none());
        let back: ResolveForeignKeyTargetsRequest = decode(wire);
        assert_eq!(back, request);
        // …and rides along when set.
        let with_limit =
            ResolveForeignKeyTargetsRequest { limit: Some(25), ..request.clone() };
        assert_eq!(encode(&with_limit)["limit"], 25);
    }

    #[test]
    fn explain_plans_recurse_and_keep_their_optional_numbers() {
        let leaf = ExplainPlanNode {
            id: "2".into(),
            label: "Seq Scan".into(),
            cost: None,
            rows: Some(12),
            details: vec![ExplainPlanDetail { key: "rows".into(), value: "12".into() }],
            children: Vec::new(),
        };
        let root = ExplainPlanNode {
            id: "1".into(),
            label: "Aggregate".into(),
            cost: Some(24.5),
            rows: Some(1),
            details: Vec::new(),
            children: vec![leaf],
        };
        let wire = encode(&root);
        assert_eq!(wire["cost"], 24.5);
        assert_eq!(wire["rows"], 1);
        // `?:` numbers stay absent when unset.
        assert!(wire["children"][0].get("cost").is_none());
        assert_eq!(wire["children"][0]["rows"], 12);
        assert_eq!(wire["children"][0]["details"][0]["key"], "rows");
        let back: ExplainPlanNode = decode(wire);
        assert_eq!(back, root);

        let result = ExplainResult {
            plan_text: "Aggregate  (cost=24.50..24.51 rows=1)".into(),
            plan_json: Some(serde_json::json!({ "Plan": { "Node Type": "Aggregate" } })),
            plan_tree: Some(root),
            total_cost: None,
            estimated_rows: None,
        };
        let wire = encode(&result);
        assert_eq!(wire["planText"], "Aggregate  (cost=24.50..24.51 rows=1)");
        assert_eq!(wire["planJson"]["Plan"]["Node Type"], "Aggregate");
        assert_eq!(wire["planTree"]["children"][0]["rows"], 12);
        assert!(wire.get("totalCost").is_none());
        assert!(wire.get("estimatedRows").is_none());
        let back: ExplainResult = decode(wire);
        assert_eq!(back, result);
    }

    #[test]
    fn query_history_entries_pin_their_widths_and_two_states() {
        let entry = QueryHistoryEntry {
            id: "q1".into(),
            connection_id: "c1".into(),
            database: "app".into(),
            schema: None,
            sql: "SELECT 1".into(),
            executed_at: "2026-01-02T03:04:05Z".into(),
            execution_time_ms: 4_000_000_000,
            rows_affected: None,
            success: true,
            error_message: None,
        };
        let wire = encode(&entry);
        // 4_000_000_000 is still *inside* `u32`'s range — the true
        // past-`u32` boundary is pinned in the widths test below.
        let wide_ms = 4_000_000_000u64;
        assert_eq!(
            wire,
            serde_json::json!({
                "id": "q1",
                "connectionId": "c1",
                "database": "app",
                "schema": null,
                "sql": "SELECT 1",
                "executedAt": "2026-01-02T03:04:05Z",
                "executionTimeMs": wide_ms,
                "success": true,
            })
        );
        // Plain `?:` fields stay absent when unset.
        assert!(wire.get("rowsAffected").is_none());
        assert!(wire.get("errorMessage").is_none());
        let back: QueryHistoryEntry = decode(wire);
        assert_eq!(back, entry);
        // And the failure half of the record round-trips.
        let failed = QueryHistoryEntry {
            schema: Some("public".into()),
            rows_affected: Some(0),
            success: false,
            error_message: Some("relation missing".into()),
            ..entry
        };
        let wire = encode(&failed);
        assert_eq!(wire["schema"], "public");
        assert_eq!(wire["rowsAffected"], 0);
        assert_eq!(wire["errorMessage"], "relation missing");
        let back: QueryHistoryEntry = decode(wire);
        assert_eq!(back, failed);
    }

    #[test]
    fn context_entries_recurse_and_keep_their_optional_size() {
        let entry = ContextEntry {
            name: "src".into(),
            path: "/p/src".into(),
            is_dir: true,
            size: None,
            children: Some(vec![ContextEntry {
                name: "main.sql".into(),
                path: "/p/src/main.sql".into(),
                is_dir: false,
                size: Some(128),
                children: None,
            }]),
        };
        let wire = encode(&entry);
        assert_eq!(wire["isDir"], true);
        // `?:` size stays absent on the directory…
        assert!(wire.get("size").is_none());
        // …and rides on the file.
        assert_eq!(wire["children"][0]["size"], 128);
        assert!(wire["children"][0].get("children").is_none());
        let back: ContextEntry = decode(wire);
        assert_eq!(back, entry);
    }

    // ------------------------------------------------------------------
    // Audit pins (T128-audit). The coder's 26 pins were probed with 16
    // mutants: the literal tables, the null/absent two-state and the
    // recursion held (every shuffle/camelCase/serde-rename mutant on a
    // *tested* shape went red), but eleven mutants survived — the closed
    // unions were open, `WebSocketTunnelConfig` was never serialized,
    // `SavedTunnel`'s multi-word leg keys never appeared, required keys
    // could silently grow serde defaults, the documented widths were
    // unpinned, the HttpProxy `?:` skips were unobserved, and the crate-root
    // re-export surface had no consumer to notice a shrink. Each pin below
    // replays one surviving mutant's condition.
    // ------------------------------------------------------------------

    #[test]
    fn the_literal_unions_stay_closed() {
        // TS's closed string-literal unions are compile-time contracts; the
        // runtime mirror is serde refusing a non-member literal. The
        // near-misses below are spellings a widened enum would gladly
        // accept — the audit's surviving mutants were a sixth `TableType`
        // variant and a `none` `SavedTunnelKind`, both invisible to the
        // positive literal tables above.
        fn rejects<T: serde::de::DeserializeOwned>(literal: &str) {
            assert!(
                serde_json::from_value::<T>(serde_json::json!(literal)).is_err(),
                "`{literal}` must not decode into the closed React union"
            );
        }
        rejects::<TableType>("materialized_view");
        rejects::<TableType>("foreignTable");
        rejects::<TableType>("matview");
        rejects::<SslMode>("verify-ca");
        rejects::<SslMode>("verify_full");
        // `table`/`view` are the ObjectKind *superset's* members — this
        // facade's five literals are the non-relation kinds only.
        rejects::<DatabaseObjectKind>("table");
        rejects::<DatabaseObjectKind>("view");
        rejects::<SshAuthMethod>("privateKey");
        rejects::<SshAuthMethod>("private-key");
        // `webSocket` is what camelCase rename would spell — the React
        // literal is flat `websocket`.
        rejects::<TunnelKind>("webSocket");
        rejects::<TunnelKind>("http_proxy");
        rejects::<HttpProxyScheme>("ws");
        rejects::<WebSocketMode>("datazen-v1");
        rejects::<WebSocketMode>("rawBinary");
        // `Exclude<TunnelKind, 'none'>`: a stored tunnel is never `none`.
        rejects::<SavedTunnelKind>("none");
        rejects::<ForeignKeyDeferrability>("deferrable");
        rejects::<ForeignKeyDeferrability>("not_deferrable");
    }

    #[test]
    fn websocket_tunnels_round_trip_their_camel_case_wire() {
        // The one struct of the file the coder's tests never serialized —
        // a mutant that dropped this struct's camelCase rename (or its
        // `?:` skips) survived the whole round untouched.
        let mut headers = BTreeMap::new();
        headers.insert("X-Datazen".to_owned(), "1".to_owned());
        let tunnel = WebSocketTunnelConfig {
            enabled: true,
            url: "wss://relay.example.com/v1/tunnel".to_owned(),
            auth_token: Some("tok".to_owned()),
            headers: Some(headers),
            connect_timeout_secs: Some(30),
            ping_interval_secs: Some(15),
            mode: Some(WebSocketMode::DatazenV1),
        };
        let wire = encode(&tunnel);
        assert_eq!(wire["url"], "wss://relay.example.com/v1/tunnel");
        assert_eq!(wire["authToken"], "tok");
        assert_eq!(wire["headers"]["X-Datazen"], "1");
        assert_eq!(wire["connectTimeoutSecs"], 30);
        assert_eq!(wire["pingIntervalSecs"], 15);
        assert_eq!(wire["mode"], "datazen_v1");
        let back: WebSocketTunnelConfig = decode(wire);
        assert_eq!(back, tunnel);
        // Bare relay: exactly the two required keys — the `?:` legs absent.
        let bare = WebSocketTunnelConfig {
            enabled: true,
            url: "wss://relay.example.com/v1/tunnel".to_owned(),
            auth_token: None,
            headers: None,
            connect_timeout_secs: None,
            ping_interval_secs: None,
            mode: None,
        };
        assert_eq!(
            encode(&bare),
            serde_json::json!({
                "enabled": true,
                "url": "wss://relay.example.com/v1/tunnel",
            })
        );
    }

    #[test]
    fn saved_tunnels_carry_their_tunnel_legs_under_the_facade_keys() {
        // The coder's saved-tunnel pin kept every leg `None`, so a dropped
        // camelCase rename on the struct survived — the single-word keys
        // don't show it; `httpProxy` and the websocket leg's fields do.
        let http = SavedTunnel {
            id: "t2".into(),
            name: "Prod Proxy".into(),
            kind: SavedTunnelKind::HttpProxy,
            ssh: None,
            http_proxy: Some(HttpProxyTunnelConfig {
                enabled: true,
                host: "proxy.example.com".into(),
                port: 3128,
                scheme: HttpProxyScheme::Http,
                username: None,
                password: None,
                headers: None,
                connect_timeout_secs: None,
            }),
            websocket: None,
        };
        let wire = encode(&http);
        assert_eq!(wire["kind"], "httpProxy");
        assert_eq!(wire["httpProxy"]["scheme"], "http");
        assert_eq!(wire["httpProxy"]["port"], 3128);
        assert!(wire.get("ssh").is_none());
        assert!(wire.get("websocket").is_none());
        let back: SavedTunnel = decode(wire);
        assert_eq!(back, http);

        let relay = SavedTunnel {
            id: "t3".into(),
            name: "Relay".into(),
            kind: SavedTunnelKind::WebSocket,
            ssh: None,
            http_proxy: None,
            websocket: Some(WebSocketTunnelConfig {
                enabled: true,
                url: "wss://relay.example.com/v1/tunnel".into(),
                auth_token: None,
                headers: None,
                connect_timeout_secs: None,
                ping_interval_secs: None,
                mode: Some(WebSocketMode::RawBinary),
            }),
        };
        let wire = encode(&relay);
        assert_eq!(wire["kind"], "websocket");
        assert_eq!(wire["websocket"]["url"], "wss://relay.example.com/v1/tunnel");
        assert_eq!(wire["websocket"]["mode"], "raw_binary");
        let back: SavedTunnel = decode(wire);
        assert_eq!(back, relay);
    }

    #[test]
    fn a_bare_http_proxy_tunnel_serializes_to_exactly_its_four_required_keys() {
        // The coder's full-connection pin only asserted specific keys, so
        // dropping the `?:` skips on username/password/headers survived —
        // `None` would have ridden the wire as explicit nulls unnoticed.
        let bare = HttpProxyTunnelConfig {
            enabled: true,
            host: "proxy.example.com".into(),
            port: 3128,
            scheme: HttpProxyScheme::Http,
            username: None,
            password: None,
            headers: None,
            connect_timeout_secs: None,
        };
        assert_eq!(
            encode(&bare),
            serde_json::json!({
                "enabled": true,
                "host": "proxy.example.com",
                "port": 3128,
                "scheme": "http",
            })
        );
        let back: HttpProxyTunnelConfig = decode(encode(&bare));
        assert_eq!(back, bare);
    }

    #[test]
    fn required_keys_stay_required_on_the_wire() {
        // A field React declares without `?` must not grow a serde default:
        // the audit's surviving mutant defaulted `sslMode` (with a Default
        // impl on the enum) and every test stayed green. TS's required
        // marker is a wire contract, not a suggestion.
        let missing_ssl = serde_json::json!({
            "id": "c1",
            "name": "n",
            "databaseType": "postgresql",
        });
        assert!(serde_json::from_value::<ConnectionConfig>(missing_ssl).is_err());
        let missing_type = serde_json::json!({
            "name": "users",
            "schema": null,
            "rowCount": null,
        });
        assert!(serde_json::from_value::<TableInfo>(missing_type).is_err());
        // `children` is required (an empty plan node still carries `[]`) —
        // the recursion's termination is part of the shape.
        let missing_children = serde_json::json!({
            "id": "1",
            "label": "Aggregate",
            "details": [],
        });
        assert!(serde_json::from_value::<ExplainPlanNode>(missing_children).is_err());
        let missing_ms = serde_json::json!({
            "id": "q1",
            "connectionId": "c1",
            "database": "app",
            "schema": null,
            "sql": "SELECT 1",
            "executedAt": "2026-01-02T03:04:05Z",
            "success": true,
        });
        assert!(serde_json::from_value::<QueryHistoryEntry>(missing_ms).is_err());
    }

    #[test]
    fn the_documented_widths_are_pinned_at_their_boundaries() {
        // Every width below is the module's documented mirror rule, not the
        // TS `number` (which is unbounded): mutating any of them slipped
        // through the coder's pins, which never left the narrowed range.

        // Ports are the u16 driver-api twin width: one past the TCP range
        // must be rejected — on the connection, the SSH leg and the proxy.
        let over = serde_json::json!({
            "id": "c1",
            "name": "n",
            "databaseType": "postgresql",
            "sslMode": "disable",
            "port": 65536,
        });
        assert!(serde_json::from_value::<ConnectionConfig>(over).is_err());
        let over = serde_json::json!({
            "enabled": true,
            "host": "bastion.example.com",
            "port": 65536,
            "username": "deploy",
            "authMethod": "password",
        });
        assert!(serde_json::from_value::<SshTunnelConfig>(over).is_err());
        let over = serde_json::json!({
            "enabled": true,
            "host": "proxy.example.com",
            "port": 65536,
            "scheme": "http",
        });
        assert!(serde_json::from_value::<HttpProxyTunnelConfig>(over).is_err());

        // Catalog row counts are i64 because estimates go negative
        // (PG `reltuples` is -1 until ANALYZE); u64 would reject the
        // backend's own payload.
        let info: TableInfo = decode(serde_json::json!({
            "name": "users",
            "schema": null,
            "tableType": "table",
            "rowCount": -1,
        }));
        assert_eq!(info.row_count, Some(-1));
        assert_eq!(encode(&info)["rowCount"], -1);

        // The ms clock is u64: `u32::MAX + 1` is a real 49-day elapsed
        // window — and 4_000_000_000, the coder's pin, is not past u32
        // at all.
        let entry: QueryHistoryEntry = decode(serde_json::json!({
            "id": "q1",
            "connectionId": "c1",
            "database": "app",
            "schema": null,
            "sql": "SELECT 1",
            "executedAt": "2026-01-02T03:04:05Z",
            "executionTimeMs": 4_294_967_296u64,
            "success": true,
        }));
        assert_eq!(entry.execution_time_ms, 4_294_967_296);
        assert_eq!(encode(&entry)["executionTimeMs"], 4_294_967_296u64);

        // File sizes are u64: a 5 GB context file overflows u32.
        let file = ContextEntry {
            name: "big.sql".into(),
            path: "/p/big.sql".into(),
            is_dir: false,
            size: Some(5_000_000_000),
            children: None,
        };
        assert_eq!(encode(&file)["size"], 5_000_000_000u64);
        let back: ContextEntry = decode(encode(&file));
        assert_eq!(back, file);
    }

    #[test]
    fn the_connection_vocabulary_is_reexported_at_the_crate_root() {
        // `crate::` paths only — a dropped root re-export stops this file
        // from compiling instead of silently shrinking the facade (the
        // same pin style as `chart_types`'). The audit's surviving mutant
        // removed two names from `pub use connection_types::{…}` and the
        // whole suite stayed green, because nothing consumes the names
        // through the root yet.
        fn resolves<T>() {}
        resolves::<crate::ConnectionConfig>();
        resolves::<crate::ContextEntry>();
        resolves::<crate::DatabaseObject>();
        resolves::<crate::DatabaseObjectKind>();
        resolves::<crate::DatabaseType>();
        resolves::<crate::DriverCapabilities>();
        resolves::<crate::ExplainPlanDetail>();
        resolves::<crate::ExplainPlanNode>();
        resolves::<crate::ExplainResult>();
        resolves::<crate::ForeignKeyDeferrability>();
        resolves::<crate::ForeignKeyInfo>();
        resolves::<crate::ForeignKeyTargetRef>();
        resolves::<crate::ForeignKeyTargetResolution>();
        resolves::<crate::HttpProxyScheme>();
        resolves::<crate::HttpProxyTunnelConfig>();
        resolves::<crate::IndexInfo>();
        resolves::<crate::PrivilegeGrant>();
        resolves::<crate::QueryHistoryEntry>();
        resolves::<crate::ResolveForeignKeyTargetsRequest>();
        resolves::<crate::SavedTunnel>();
        resolves::<crate::SavedTunnelKind>();
        resolves::<crate::ServerInfo>();
        resolves::<crate::SshAuthMethod>();
        resolves::<crate::SshTunnelConfig>();
        resolves::<crate::SslMode>();
        resolves::<crate::TableInfo>();
        resolves::<crate::TableSchema>();
        resolves::<crate::TableType>();
        resolves::<crate::TunnelKind>();
        resolves::<crate::Value>();
        resolves::<crate::WebSocketMode>();
        resolves::<crate::WebSocketTunnelConfig>();
        // …and one live decode through the root path keeps the pin honest.
        let info: crate::TableInfo = decode(serde_json::json!({
            "name": "users",
            "schema": null,
            "tableType": "view",
            "rowCount": null,
        }));
        assert_eq!(info.table_type, crate::TableType::View);
    }
}
