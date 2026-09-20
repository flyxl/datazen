# DataZen: The database workspace for developers and AI agents

**tl;dr:** A desktop database tool that actually understands how developers work. Query, diagnose, visualize, migrate, and automate—all in one place. Built with Rust + Tauri, free, no account required.

![DataZen demo](https://flyxl.github.io/datazen/assets/video/demo-poster.png)

---

## Hello Product Hunt! 👋

I'm the creator of DataZen. For the past year, I've been building the database tool I always wanted but couldn't find.

**The backstory:** I'm a full-stack developer who works with PostgreSQL, MySQL, and Redis daily. I got tired of:
- Switching between 3-4 different tools for one database task
- Copy-pasting schema info to ChatGPT when queries failed
- Exporting data to Excel just to make a simple chart
- Writing the same SQL scripts over and over

So I built DataZen. It's not trying to be everything—it's trying to be the database workspace that actually fits how developers work.

**What makes it different:**
- **AI is optional, not forced.** Use it without an API key. When you do need it, it knows your actual schema.
- **Workflows are practical.** Chain SQL + AI + conditions in YAML. Cross-database? No problem.
- **Open source.** GPLv3 core, MIT driver API. Build your own database drivers if you need to.

---

## The problem

We've all been there. You're debugging a slow query, switching between three different tools: one for writing SQL, another for checking the execution plan, and a third for visualizing the results. Then you need to explain the error to a teammate, so you copy-paste the schema and error message into ChatGPT. 

Database work shouldn't feel like juggling knives.

## What we built

DataZen is a database workspace that tries to keep the whole database loop in one place. Not just another SQL editor with a fancy UI, but a tool that understands the actual workflow:

![Query results and charts](https://flyxl.github.io/datazen/assets/screenshots/02-query-chart.png)

1. **Write and run SQL** in a modern editor with autocomplete that knows your schema
2. **Understand what went wrong** when queries fail—AI can explain errors and suggest fixes

![AI error diagnosis](https://flyxl.github.io/datazen/assets/screenshots/05-ai-diagnosis.png)

3. **See the data** through charts without exporting to Excel

![Chart types](https://flyxl.github.io/datazen/assets/screenshots/10-chart-types.png)

4. **Move data safely** with backup, sync, and migration tools that show you exactly what will happen before it happens
5. **Automate repetitive tasks** with YAML workflows that can chain queries, AI steps, and conditions

![Workflow editor](https://flyxl.github.io/datazen/assets/screenshots/04-workflow.png)

## The stuff we're actually proud of

**AI that stays out of your way.** The AI features are optional. You can use DataZen without ever touching an API key. But when you do need help, it's right there—connected to your actual schema, not making guesses about your table structure.

![AI natural-language SQL](https://flyxl.github.io/datazen/assets/screenshots/03-ai-nl2sql.png)

**Cross-database workflows.** Need to pull data from PostgreSQL, combine it with MySQL, and have AI summarize the results? One YAML file. One click.

**SQL Editor Pro.** Not just syntax highlighting. Statement gutters to run individual statements, hover tooltips showing column types, real-time linting, and transaction controls in the toolbar.

![Editor Pro — statement gutter](https://flyxl.github.io/datazen/assets/screenshots/pro-01-statement-gutter.png)

**MCP support.** DataZen works both as an MCP server and client, so it can fit into larger AI-assisted development workflows.

**Open source.** GPLv3 with a plugin exception that lets third-party drivers ship under their own licenses. The driver API is MIT.

**Security by design.** Passwords are AES-256-GCM encrypted. The master key lives in your OS keychain. Only schema and query context are shared with AI—and only with the provider you configure.

## Tech stack

- **Frontend:** React 18 + TypeScript + Tailwind CSS
- **Backend:** Rust (Tauri v2)
- **AI:** OpenAI, Anthropic, DeepSeek, Ollama, custom endpoints
- **Databases:** PostgreSQL, MySQL/MariaDB, SQLite, Redis, MongoDB, ClickHouse, DuckDB, SQL Server, and more

**Why Rust?** Performance matters when you're working with large datasets. Tauri v2 gives us a native feel with web technologies, and Rust ensures the backend stays fast and memory-safe. The installer is under 15 MB.

## Who it's for

Developers who work with databases daily and are tired of context-switching. DevOps engineers who need to move data between systems safely. Data analysts who want to visualize query results without leaving their database tool.

**How we compare:**
- **vs DBeaver/DataGrip:** We're lighter, faster, and have built-in AI that actually understands your schema
- **vs pgAdmin/MySQL Workbench:** We're multi-database, not tied to one vendor
- **vs VS Code extensions:** We're a dedicated workspace, not a general-purpose editor with database plugins

## What's next

We're building out more database drivers and improving the AI context window for complex schemas. The workflow engine needs more step types. And we want to make the workspace apps ecosystem actually useful.

**Our vision:** A database workspace that grows with you. Start with a simple SQL editor, add AI when you need it, automate with workflows, and extend with plugins. All in one lightweight app.

**Current status:** Version 0.2.1, actively developed. The core features are stable, but we're still iterating on the UI and adding more database drivers.

**By the numbers:**
- **Under 15 MB** installer size
- **4 core databases** included (PostgreSQL, MySQL, SQLite, Redis)
- **6+ optional drivers** available (MongoDB, ClickHouse, DuckDB, SQL Server, etc.)
- **Multiple AI providers** supported (OpenAI, Anthropic, DeepSeek, Ollama, custom)

---

## Quick start

1. **Download** from [flyxl.github.io/datazen/download.html](https://flyxl.github.io/datazen/download.html) — pick your platform (macOS, Windows, Linux)
2. **Connect** to your database — PostgreSQL, MySQL, SQLite, or Redis
3. **Try AI** — ask "show me all users inactive for 30 days" and watch it generate SQL
4. **Explore** — browse schemas, run queries, switch to charts

No account needed. AI is optional. Your data stays local.

---

**Try it:** [flyxl.github.io/datazen/download.html](https://flyxl.github.io/datazen/download.html)  
**GitHub:** [github.com/flyxl/datazen](https://github.com/flyxl/datazen)  
**Docs:** [flyxl.github.io/datazen](https://flyxl.github.io/datazen/)

We're not trying to replace DBeaver or DataGrip. We're trying to build the tool we actually want to use. Hope you find it useful too.

---

## Maker's comment

**Q: Is this just another database GUI?**
A: It's a database workspace. The difference is in the workflow—it's designed around how developers actually use databases, not just how they query them.

**Q: Why build this in Rust?**
A: Performance matters when you're working with large datasets. Tauri v2 gives us a native feel with web technologies, and Rust ensures the backend stays fast and memory-safe.

**Q: What about the AI features?**
A: They're completely optional. No API key needed for basic database work. When you do use AI, it's connected to your actual schema—not guessing about your table structure.

**Q: How do you make money?**
A: We don't—yet. DataZen is free and open source. We're exploring a Pro version with advanced features, but the core will always be free.

**Q: Can I contribute?**
A: Absolutely! We welcome bug reports, feature requests, database drivers, and code contributions. Check out CONTRIBUTING.md on GitHub. We're especially interested in new database drivers.

**Q: What's the biggest challenge you've faced?**
A: Balancing features with stability. Every database driver has its quirks, and we want to make sure everything works reliably before adding more features.

---

*Thanks for checking out DataZen. If you have any questions, I'm happy to answer them in the comments below.*