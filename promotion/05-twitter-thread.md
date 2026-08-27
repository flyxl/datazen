# Twitter/X Thread

## Tweet 1 (Hook)

Introducing DataZen — an open-source AI-native database client built with Tauri + Rust.

<15MB. SQL + AI + Charts + Workflows + MCP. All local-first.

GitHub: https://github.com/flyxl/datazen

🧵 Thread ↓

## Tweet 2 (AI Features)

Most DB clients treat AI as a side feature. DataZen puts it at the core:

• Natural language → SQL (uses your schema as context)
• SQL error diagnosis — paste error, get a fix
• EXPLAIN plan analysis with AI bottleneck detection
• Database-aware AI Chat

Supports OpenAI, Anthropic, DeepSeek, custom endpoints.

## Tweet 3 (Workflows)

DataZen Workflows: YAML-driven automation for database work.

Chain SQL queries, AI steps, conditions, and loops — across different databases.

Example: Query Postgres for orders → Query MySQL for logistics → AI summarizes the result.

One workflow, two databases, zero copy-paste.

## Tweet 4 (MCP)

DataZen works as both MCP Server AND MCP Client.

Server: Expose your database to AI agents (headless stdio mode supported)
Client: Bring external tools (file search, web search) into your DB chat

Your database, connected to the AI ecosystem.

## Tweet 5 (Architecture)

The driver architecture is different from other DB clients:

• Compile-time integration (no runtime dynamic libs)
• Each driver can live in its own repo
• Driver API published as MIT crate on crates.io
• Drivers can include both Rust backend AND frontend UI

Default: Postgres, MySQL, SQLite, Redis
Optional: MongoDB, ClickHouse, DuckDB, SQL Server

## Tweet 6 (Call to Action)

v0.1.0 is out. It's early, but functional.

Try it: https://github.com/flyxl/datazen
Build from source: pnpm install && pnpm tauri dev

Feedback on the architecture, driver API, or anything else is very welcome.

#Rust #Tauri #Database #AI #OpenSource
