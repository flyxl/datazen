# Reddit — r/rust Post

**Title:** DataZen – Open-source AI database client built with Tauri v2 + Rust (<15MB)

**Body:**

Hey r/rust! I've been working on DataZen, an open-source desktop database client built on Tauri v2.

**Key features:**
- **AI-native**: Natural language → SQL, error diagnosis, EXPLAIN analysis — all using your database schema as context (supports OpenAI, Anthropic, DeepSeek, custom endpoints)
- **Lightweight**: <15MB installed, fast startup thanks to Tauri + Rust
- **Visual**: Query results → charts (line, bar, pie, scatter, area) without leaving the app
- **Automatable**: YAML Workflows that chain SQL, AI, conditions, and loops — including cross-database operations
- **MCP integration**: Both MCP Server (expose DB to AI agents) and MCP Client (bring external tools into DB chat), with headless stdio mode
- **Compile-time drivers**: No runtime dynamic library loading — drivers are compiled into the app. Each driver can live in its own repo using the MIT-licensed `datazen-driver-api` crate

**Supported databases:**
- Default: PostgreSQL, MySQL/MariaDB, SQLite, Redis
- Optional (compile-time): MongoDB, ClickHouse, DuckDB, SQL Server
- Extensible: Any database via the Driver API

**Security:**
- AES-256-GCM encryption for credentials
- Local-first: no cloud service, no telemetry
- SSH tunnel support built-in

**Links:**
- GitHub: https://github.com/flyxl/datazen
- Driver API crate: https://crates.io/crates/datazen-driver-api
- Docs: https://flyxl.github.io/datazen/

Early stage (v0.1.0), so feedback on the Rust/Tauri architecture or driver API design would be really appreciated!
