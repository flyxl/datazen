# Hacker News — Show HN

**Title** (pick one):

- Show HN: DataZen – MIT-licensed database GUI in a 10 MB Tauri app
- Show HN: DataZen – open-source TablePlus alternative (Rust + Tauri)

**URL**: https://github.com/flyxl/datazen

**Text** (optional first comment):

Hi HN — I built DataZen, a desktop client for PostgreSQL, MySQL, SQLite, and Redis.

Why: I wanted something lighter than DBeaver and free unlike TablePlus. It's Tauri + Rust, installer under 10 MB, with built-in SSH tunneling (pure Rust via russh, no local ssh binary), SQL editor with autocomplete, virtualized table browsing, backups, and PG↔MySQL sync.

Credentials are encrypted locally (AES-256-GCM); no cloud account.

MIT licensed, macOS/Windows/Linux builds on Releases. Early (v0.0.3) but I use it daily. Feedback welcome: wuxiaolongklws@gmail.com or GitHub Issues.

macOS note: not Apple-notarized; if blocked, `xattr -cr /Applications/DataZen.app`.

---

## Posting tips

- Post URL = GitHub repo (not landing page) — HN prefers primary source
- Be online 2-3 hours after posting to answer questions
- Stay factual; avoid marketing tone in comments

## Checklist

- [ ] Posted at US morning (9-11am ET)
- [ ] Responded to top comments same day
