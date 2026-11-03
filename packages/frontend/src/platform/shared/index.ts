/**
 * Shared platform layer (form-split): implementations that answer shared
 * contracts with browser APIs only — no host runtime (R6-clean). Assemblies
 * (apps/web, apps/desktop) bind these alongside the ports; shared features
 * import through this barrel.
 */
export {
  formatCallToolResult,
  mcpQualifiedName,
  McpJsonRpcError,
  WebMcpClient,
  WebMcpClientManager,
} from './mcpClient';
export type {
  McpContentBlock,
  McpFetch,
  McpHttpResponse,
  McpToolCallResult,
  WebMcpServerConfig,
  WebMcpToolInfo,
} from './mcpClient';
