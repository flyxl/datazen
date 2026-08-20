---
name: bump-version
description: >-
  Update the DataZen application version number across all files and trigger a
  GitHub release rebuild. Use when the user asks to change, bump, or update the
  app version number.
---

# Bump Version

Run the bump script to update the version across all source files, commit,
tag, and push to all remotes in one step.

## Usage

```bash
.cursor/skills/bump-version/scripts/bump.sh <new-version>
```

The version can include a leading `v` (it is stripped automatically).

Examples:

```bash
.cursor/skills/bump-version/scripts/bump.sh 0.1.0
.cursor/skills/bump-version/scripts/bump.sh v1.2.3
```

## What the Script Does

1. Reads the current version from `package.json`.
2. Updates bare semver in config files:
   - `package.json`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`
3. Updates `DataZen vX.Y.Z` display strings in:
   - `src/windows/main/MainPage.tsx`
   - `src/windows/data-sync/DataSyncWindow.tsx`
   - `e2e/specs/homepage-features.ts`
4. Runs `cargo check` to regenerate `Cargo.lock`.
5. Commits all changes: `chore: bump version to vX.Y.Z`.
6. Creates git tag `vX.Y.Z` and pushes to `origin` (GitHub; default) and
   optional `gitee` mirror if that remote exists.
   If the tag already exists remotely, it is deleted and re-created to
   trigger a fresh GitHub Actions release build.

## Adding New Version References

If a new file starts referencing the app version, add it to the script:
- Config pattern → add to the "Config files" section
- Display string → add the path to the `DISPLAY_FILES` array
