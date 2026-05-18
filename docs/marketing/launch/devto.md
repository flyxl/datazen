# Dev.to 发布稿

**Title:** DataZen: a 10 MB open-source database client built with Tauri and Rust

**Tags:** `opensource`, `rust`, `database`, `tauri`, `showdev`

**Cover image:** upload `docs/marketing/assets/og-image.png`

---

DataZen is a free, MIT-licensed desktop app for **PostgreSQL, MySQL, SQLite, and Redis**.

## Why another client?

- **TablePlus** is great but paid for many teams
- **DBeaver** is powerful but heavy on RAM and startup time

DataZen targets daily dev work: connect, browse, run SQL, export — in a **&lt;10 MB** installer.

## Stack

- **Tauri v2** + **Rust** backend (sqlx, redis, russh for SSH)
- React + CodeMirror 6 frontend
- Credentials encrypted locally (AES-256-GCM)

## Features

- Multi-window workflow
- Built-in **SSH tunnels** (no local `ssh` binary)
- SQL editor with table/column autocomplete
- Virtual scrolling for large tables
- Backup to SQL, CSV/JSON import/export
- PG ↔ MySQL schema + data sync
- Redis key browser
- Dark theme, English + Chinese UI

## Status

Early **v0.0.3**, but I use it as a daily driver for SQL + Redis.

- Download: https://github.com/flyxl/datazen/releases
- Site: https://flyxl.github.io/datazen/
- Repo: https://github.com/flyxl/datazen

**macOS:** if Gatekeeper blocks the app, run `xattr -cr /Applications/DataZen.app` after install.

Feedback: wuxiaolongklws@gmail.com — stars and issues welcome!
