# DataZen v0.1.0

**First public release.**

DataZen is a lightweight, open-source, AI-native database client built with Tauri v2 + Rust.

## Highlights

### AI-Native Database Work

- **Natural language → SQL**: Describe what you need in plain language; DataZen generates executable SQL using your current database schema as context.
- **SQL error diagnosis**: When a query fails, AI analyzes the error + schema and proposes a fix.
- **EXPLAIN analysis**: Visualize execution plans and let AI identify bottlenecks and optimization opportunities.
- **Database-aware AI Chat**: AI sidebar understands your schema; SQL from conversations can be inserted into the editor with one click.
- Supports OpenAI, Anthropic, DeepSeek, and compatible custom endpoints.

### Data Visualization

- Turn query results into charts without exporting to Excel.
- Line, bar, pie, scatter, and area charts with aggregation and grouping.
- PNG and SVG export.

### YAML Workflows

- Chain SQL queries, AI steps, conditions, and loops in reusable YAML definitions.
- Cross-database workflows: different steps can use different database connections.
- Run from the UI, AI sidebar, or MCP.
- Generate workflows with AI from natural language descriptions.

### MCP Integration

- **MCP Server**: Expose database queries, schema browsing, EXPLAIN, and workflows to external AI agents. Supports headless stdio mode for automation.
- **MCP Client**: Connect external MCP servers to DataZen AI Chat, bringing additional tools and context into database conversations.

### Extensible Driver Architecture

- Database drivers are compiled into the app at build time (no runtime dynamic-library ABI).
- Each driver can be developed in an independent repository.
- Driver API published as the MIT-licensed `datazen-driver-api` crate on [crates.io](https://crates.io/crates/datazen-driver-api).
- Build-time driver selection: choose exactly which databases to include.

### Supported Databases

| Database | Default / Optional |
|---|---|
| PostgreSQL | Default |
| MySQL / MariaDB | Default |
| SQLite | Default |
| Redis | Default |
| MongoDB | Optional |
| ClickHouse | Optional |
| DuckDB | Optional |
| SQL Server | Optional |

### Security & Privacy

- AES-256-GCM encryption for stored credentials.
- Master key in OS keychain (or local `.key` file for dev builds).
- Local-first: no cloud service, no telemetry.
- SSH tunnel support.

### Cross-Platform

- macOS (Apple Silicon + Intel)
- Windows (x86_64)
- Linux (x86_64: .deb, .rpm, .AppImage)

## Tech Stack

- **Backend**: Rust, Tauri v2
- **Frontend**: React 18, TypeScript, Tailwind CSS 4
- **State**: Zustand
- **Editor**: CodeMirror 6
- **Charts**: Recharts
- **AI**: Multi-provider (OpenAI, Anthropic, DeepSeek, Custom)
- **MCP**: rmcp crate (Server + Client)

## Known Limitations

- AI features require a configured AI provider endpoint (no built-in API key).
- Some advanced database-specific features (stored procedures, triggers browsing) are driver-dependent.
- macOS builds are not notarized yet; you may need to right-click → Open on first launch.

## Links

- **GitHub**: https://github.com/flyxl/datazen
- **Website**: https://flyxl.github.io/datazen/
- **Driver API**: https://crates.io/crates/datazen-driver-api
- **License**: GPLv3 (Driver API: MIT)
