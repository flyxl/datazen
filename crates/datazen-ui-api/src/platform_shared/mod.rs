//! Shared platform layer (form-split) — port of React
//! `packages/frontend/src/platform/shared/`.
//!
//! React's own barrel header (index.ts): implementations that answer
//! shared contracts with browser APIs only — no host runtime
//! (R6-clean). Assemblies (packages/web, packages/desktop) bind these
//! alongside the ports; shared features import through this barrel. The
//! Rust face of the barrel is this module — one sibling submodule per
//! React file, re-exported here.
//!
//! Landed: `mcpClient.ts` → [`mcp_client`] (tasklist T092), with its
//! React test pair as the sibling `mcp_client_tests` module and the suite's
//! doubles in [`test_support`]. The barrel itself (index.ts, tasklist
//! T044) owns this face as further siblings land — the re-export set
//! below is exactly the barrel's export set today, with the port's three
//! structural additions ([`McpError`], [`McpRequestInit`],
//! [`WebMcpConnectedServer`] — see the mcp_client module docs for their
//! React justification).
//!
//! wasm32-safe like every sibling of this crate's platform surface (the
//! only dependencies are serde and serde_json; the seam types carry no
//! IO).

mod mcp_client;
#[cfg(test)]
mod mcp_client_tests;
#[cfg(test)]
mod test_support;

pub use mcp_client::{
    McpContentBlock, McpError, McpFetch, McpHttpResponse, McpJsonRpcError, McpRequestInit,
    McpToolCallResult, WebMcpClient, WebMcpClientManager, WebMcpConnectedServer,
    WebMcpServerConfig, WebMcpToolInfo, format_call_tool_result, mcp_qualified_name,
};
