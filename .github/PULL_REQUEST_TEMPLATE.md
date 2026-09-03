## Summary

<!-- What changed and why (1–3 bullets). -->

-

## Test plan

- [ ] `pnpm test:unit`
- [ ] `cargo test -p datazen --lib`
- [ ] `cargo test -p datazen-ai-api --lib`
- [ ] `node scripts/check-site-seo.mjs`
- [ ] Manual / E2E checks (describe, if applicable):

## Checklist

- [ ] I followed CONTRIBUTING.md
- [ ] No secrets or credentials in the diff
- [ ] Docs / i18n updated if user-facing behavior changed
- [ ] **External contracts**: if this PR touches MCP tools/resources, IPC command shapes, `PROTOCOL_VERSION` / `AI_PROTOCOL_VERSION`, or persisted JSON keys — I read [`docs/development/external-contract-policy.md`](docs/development/external-contract-policy.md), classified breaking vs non-breaking changes, and updated `src-tauri/src/mcp/fixtures/mcp_external_contract.json` when MCP tool names or input keys changed
