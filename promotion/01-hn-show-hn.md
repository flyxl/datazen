# Hacker News — Show HN Post

**Title:** Show HN: DataZen – Lightweight AI-native database client built with Tauri + Rust

**URL:** https://github.com/flyxl/datazen

**Body:**

Hi HN! I'm building DataZen, an open-source desktop database client that puts AI next to your database instead of making you copy schema and errors into ChatGPT.

**What it does:**
- Write and run SQL in a modern editor (CodeMirror 6)
- Natural language → SQL using your current database schema as context
- SQL error diagnosis — paste the error + schema, get a fix
- EXPLAIN plan visualization + AI-powered bottleneck analysis
- Turn query results into charts (line, bar, pie, scatter, area) without exporting to Excel
- YAML Workflows that chain SQL, AI, conditions, and loops — including cross-database (e.g., query Postgres + MySQL in one workflow)
- MCP Server + Client — expose your database to AI agents, or bring external tools into your DB chat

**Why it's different:**
- **<15MB** — Tauri + Rust, no Electron bloat
- **AI-native** — not a ChatGPT wrapper, AI is deeply integrated with schema context
- **Compile-time drivers** — no unstable dynamic-library ABI; drivers are compiled into the app, each can have its own repo
- **MCP first** — works as both MCP Server and Client, with headless stdio mode for agent automation
- **Local-first** — AES-256-GCM encryption, no cloud service, no telemetry

**Supported databases:** PostgreSQL, MySQL/MariaDB, SQLite, Redis (default). Optional: MongoDB, ClickHouse, DuckDB, SQL Server. Extensible via the MIT-licensed Driver API crate.

**Tech stack:** Tauri v2, Rust, React 18, TypeScript, Tailwind CSS 4, Zustand, CodeMirror 6, Recharts.

It's early (v0.1.0), so there will be rough edges. I'd love feedback on the architecture, the driver API design, or anything else.

GitHub: https://github.com/flyxl/datazen

Thanks for checking it out!
