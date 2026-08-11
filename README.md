<div align="center">

<img src="site/assets/logo.png" width="96" alt="DataZen" />

# DataZen

### The lightweight, open-source AI database client for developers

Natural-language SQL · Query analysis · Charts · Workflows · MCP · Extensible drivers

[![Release](https://img.shields.io/github/v/release/flyxl/datazen?style=flat-square)](https://github.com/flyxl/datazen/releases)
[![License](https://img.shields.io/badge/license-GPLv3-blue?style=flat-square)](LICENSE)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-blue?style=flat-square)](#installation)

[Download](https://github.com/flyxl/datazen/releases) · [Website](https://flyxl.github.io/datazen/) · [中文](README.zh-CN.md) · [Contributing](CONTRIBUTING.md)

</div>

![DataZen main window](site/assets/screenshots/01-main-window.png)

## Why DataZen?

DataZen is a desktop database client built with **Tauri + Rust**. It combines the everyday database tools developers expect with AI-assisted querying, visual analysis, automation, and a compile-time driver architecture.

- **Lightweight** — Tauri + Rust keeps the application small and responsive.
- **AI-native** — generate SQL, diagnose errors, understand execution plans, and work with database context through chat.
- **Visual** — turn query results into charts without exporting to another tool.
- **Automatable** — compose SQL and AI operations into reusable YAML workflows across databases.
- **Extensible** — database drivers are integrated at compile time through the DataZen Driver API.
- **Local-first** — credentials and database access stay on your machine.
- **Open source** — GPLv3, with an architecture designed for community drivers and contributions.

## A database client built around real workflows

### SQL and data exploration

Write and run SQL in a modern editor, inspect results, browse tables, and move between query results and visualizations without leaving DataZen.

![Query results and charts](site/assets/screenshots/02-query-chart.png)

### AI-assisted database work

DataZen puts AI next to the database instead of making you copy schema and errors into another application.

![AI natural-language SQL](site/assets/screenshots/03-ai-nl2sql.png)

**Natural language → SQL**

Describe what you need and DataZen uses the current database schema as context to generate executable SQL. Generated SQL can be executed immediately or inserted into the editor for further editing.

![AI error diagnosis](site/assets/screenshots/05-ai-diagnosis.png)

**SQL error diagnosis**

When a query fails, AI can combine the database error and schema context to explain the problem and propose corrected SQL.

![AI EXPLAIN analysis](site/assets/screenshots/06-ai-explain.png)

**EXPLAIN analysis**

Visualize execution plans and use AI to identify bottlenecks, scan strategies, and optimization opportunities.

![AI Chat](site/assets/screenshots/07-ai-chat.png)

**Database-aware AI Chat**

The AI sidebar can work with the current connection's schema and turn SQL from the conversation into editor-ready code.

Supported AI integrations include OpenAI, Anthropic, DeepSeek, and compatible custom endpoints.

## Turn query results into charts

You should not need to export data to Excel just to understand it. DataZen can infer useful chart configurations from query results and switch between table and chart views.

![Chart types](site/assets/screenshots/10-chart-types.png)

Supported visualizations include line, bar, pie, scatter, and area charts, with aggregation, grouping, and PNG/SVG export.

![Chart export](site/assets/screenshots/11-chart-export.png)

## Automate database work with Workflows

DataZen Workflows describe reusable database operations in YAML. A workflow can combine queries, AI steps, conditions, and loops, with each step connected to the database it needs.

![Workflow editor](site/assets/screenshots/04-workflow.png)

For example, one workflow can query orders from PostgreSQL, fetch logistics from MySQL, and let AI summarize the combined result.

![Cross-database workflow](site/assets/screenshots/12-workflow-crossdb.png)

Workflows can be started from the UI, the AI sidebar, MCP, or generated with AI.

![Workflow execution](site/assets/screenshots/13-workflow-run.png)

## MCP: connect DataZen to the AI tool ecosystem

DataZen works both as an **MCP Server** and an **MCP Client**.

### MCP Server

Expose database operations, schema inspection, EXPLAIN, and workflows to external AI agents. DataZen also provides a headless stdio mode for automation and agent integrations.

### MCP Client

Connect external MCP servers to DataZen AI Chat and bring additional tools and context into database conversations.

This makes DataZen useful not only as a GUI, but also as a database tool inside larger AI-assisted development workflows.

## Extensible database drivers

DataZen separates the application from database-specific implementation through the **DataZen Driver API**.

```text
                         DataZen
                            │
              ┌─────────────┴─────────────┐
              │       DataZen Core        │
              │  UI · Query · AI · MCP   │
              └─────────────┬─────────────┘
                            │
                    DataZen Driver API
                            │
          ┌─────────────────┼─────────────────┐
          │                 │                 │
       PostgreSQL         MySQL          External drivers
                                           │
                              ┌────────────┼────────────┐
                              │            │            │
                           MongoDB      ClickHouse    OLAP...
```

Drivers are **compiled into DataZen** rather than loaded through an unstable Rust dynamic-library ABI. This allows a driver to provide both Rust database functionality and frontend UI while still remaining in its own repository.

### Independent driver development

A driver can be developed in an independent repository next to a local DataZen checkout:

```text
workspace/
├── datazen/
└── datazen-driver-mydb/
```

During development, DataZen's driver registry can point to the local repository with `source: "path"`. The DataZen application is then built with the selected driver, giving plugin developers a real host for both backend and frontend debugging.

The Driver API itself is maintained in `packages/driver-api` and published as the MIT-licensed `datazen-driver-api` crate. Independent drivers normally consume the published crate and do not need to clone DataZen just to obtain the API.

See the complete guides:

- **[Independent Plugin Development — English](docs/independent-plugin-development.md)**
- **[独立插件开发指南 — 中文](docs/independent-plugin-development.zh-CN.md)**
- **[Driver API crate README](packages/driver-api/README.md)**
- **[Driver API dependency boundary](docs/driver-api/public-api-dependency-boundary.md)**
- **[datazen-driver-api on crates.io](https://crates.io/crates/datazen-driver-api)**

## Supported databases

DataZen ships with a small default set and can be built with additional drivers.

| Database | Default / optional | Notes |
|---|---|---|
| PostgreSQL | Default | SQL, schema browser, EXPLAIN, AI context |
| MySQL / MariaDB | Default | SQL, schema browser, EXPLAIN |
| SQLite | Default | Embedded database workflow |
| Redis | Default | Key browser, command console, monitoring, Pub/Sub |
| MongoDB | Optional | Native driver |
| ClickHouse | Optional | Native driver |
| DuckDB | Optional | Native driver |
| SQL Server | Optional | Native driver |
| Presto / Trino and other OLAP engines | Plugin | External driver architecture |

The exact driver set is controlled at build time, so a distribution does not have to ship every database engine.

## Installation

Download the latest release from **[GitHub Releases](https://github.com/flyxl/datazen/releases)**.

| Platform | Package |
|---|---|
| macOS Apple Silicon | `.dmg` |
| macOS Intel | `.dmg` |
| Windows | `.exe` / `.msi` |
| Linux x86_64 | `.deb` / `.rpm` / `.AppImage` |

DataZen is free and does not require an account.

## Build from source

### Prerequisites

- Node.js >= 20
- pnpm >= 9
- Rust >= 1.77
- Tauri v2 system dependencies

```bash
pnpm install
pnpm tauri dev
```

Build only the drivers you need:

```bash
# Default driver set
pnpm tauri:build

# All supported path drivers
DATAZEN_DRIVERS=all pnpm tauri:build

# Custom driver set
DATAZEN_DRIVERS=postgres,mongodb pnpm tauri:build
```

## Security and privacy

DataZen is designed around local database access:

- Database credentials are stored locally.
- AI requests are sent to the provider configured by the user.
- Database data is not uploaded to a DataZen cloud service.
- SSH connections can be established directly from the application.

Always review the privacy and security policies of the AI provider and endpoint you configure.

## Documentation

- [Project website](https://flyxl.github.io/datazen/)
- [Independent Plugin Development](docs/independent-plugin-development.md)
- [Chinese Plugin Development Guide](docs/independent-plugin-development.zh-CN.md)
- [Driver API crate](packages/driver-api/README.md)
- [Driver API dependency boundary](docs/driver-api/public-api-dependency-boundary.md)
- [datazen-driver-api on crates.io](https://crates.io/crates/datazen-driver-api)
- [Workflow Guide](docs/workflow-guide.en.md)
- [Contributing](CONTRIBUTING.md)

## Contributing

DataZen welcomes bug reports, feature requests, database drivers, documentation improvements, and code contributions.

Please read [CONTRIBUTING.md](CONTRIBUTING.md) before opening a pull request. Driver work should generally be developed in an independent driver repository and integrated through the DataZen driver registry.

## License

DataZen is licensed under the **GNU General Public License v3.0**. The `datazen-driver-api` crate under `packages/driver-api` is separately licensed under the **MIT License**. See [LICENSE](LICENSE) and [packages/driver-api/LICENSE-MIT](packages/driver-api/LICENSE-MIT).

<div align="center">

**DataZen — let AI handle the database work, and turn data into insight.**

</div>
