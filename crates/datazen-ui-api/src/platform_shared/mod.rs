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
//! Landed: `mcpClient.ts` → [`mcp_client`] (T092), with its React test
//! pair as the sibling `mcp_client_tests` module and the suite's doubles
//! in [`test_support`]. The barrel itself (`index.ts`, T044) is now this
//! face: every later sibling lands as its own submodule and widens the
//! re-export set below. Today's set is exactly the React barrel's eleven
//! names plus the port's three structural additions ([`McpError`],
//! [`McpRequestInit`], [`WebMcpConnectedServer`] — see the mcp_client
//! module docs for their React justification); the barrel test module
//! pins every name's reachability through this one path.
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

/// T044 pin: every name the React barrel (`index.ts`) exports — plus the
/// port's three documented structural additions — is reachable through
/// this one module path. A dropped re-export breaks this compile.
#[cfg(test)]
mod barrel_surface {
    use super::{
        McpContentBlock, McpError, McpFetch, McpHttpResponse, McpJsonRpcError, McpRequestInit,
        McpToolCallResult, WebMcpClient, WebMcpClientManager, WebMcpConnectedServer,
        WebMcpServerConfig, WebMcpToolInfo, format_call_tool_result, mcp_qualified_name,
    };

    fn the_barrel_surface_stays_complete() {
        // Value exports — the barrel's five functions/constructors.
        let _ = format_call_tool_result;
        let _ = mcp_qualified_name;
        let _ = WebMcpClient::connect
            as fn(&WebMcpServerConfig, &dyn McpFetch) -> Result<WebMcpClient, McpError>;
        let _ = WebMcpClientManager::new as fn(Box<dyn McpFetch>) -> WebMcpClientManager;
        // Type exports — the barrel's six, plus the port's three documented
        // structural additions, all reachable through this one path.
        let _ = std::marker::PhantomData::<McpContentBlock>;
        let _ = std::marker::PhantomData::<McpToolCallResult>;
        let _ = std::marker::PhantomData::<WebMcpToolInfo>;
        let _ = std::marker::PhantomData::<McpHttpResponse>;
        let _ = std::marker::PhantomData::<WebMcpConnectedServer>;
        let _ = std::marker::PhantomData::<McpJsonRpcError>;
        let _ = std::marker::PhantomData::<McpRequestInit>;
        let _ = std::marker::PhantomData::<McpError>;
    }
}
