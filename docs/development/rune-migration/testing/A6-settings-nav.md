# A6 settings navigation grouping

- Task: `A6` (plan-native-5) — settings navigation grouping, 12 sections × 2 groups, deep-link form.
- Date: 2026-11-15
- Branch: `feature/a6-settings-nav`

## Section / group mapping

React-aligned structure (`packages/frontend/src/windows/settings/settingsSections.ts`):

| Group (2) | Section key (12) | Label |
|---|---|---|
| `app` (`Application`) | `general` | General |
| `app` | `appearance` | Appearance |
| `app` | `dataBrowsing` | Data Browsing |
| `app` | `editor` | Editor |
| `app` | `behavior` | Behavior |
| `app` | `logging` | Logging |
| `integration` (`AI & Integrations`) | `ai` | AI Assistant |
| `integration` | `prompts` | Prompts |
| `integration` | `mcpServer` | MCP Server |
| `integration` | `mcpClient` | MCP Client |
| `integration` | `tunnels` | Tunnels |
| `integration` | `extensions` | Extensions |

Existing Rune settings functionality preserved and mapped:
- `general`: favorites (save/open/rename/delete current query)
- `appearance`: theme toggle
- `dataBrowsing`: rows-per-page + query max rows settings, export, import, backup
- `editor`: SQL snippets
- `behavior/logging/ai/prompts/mcpServer/mcpClient/tunnels/extensions`: honest placeholder sections (not yet migrated in Rune), but navigable/rendered by key

## Deep-link form

- State: `AppState.settings_section: String` (default `"general"`), addressable section key within the settings page.
- API: `AppState::settings_section() -> &'static str` (parsed/normalized) and `AppState::set_settings_section(&str)` accept deep-link ids (`appearance`, `mcpServer`, etc.); unknown ids normalize to `general`.
- UI: selecting a nav item calls `set_settings_section(id)`; `settings_page` renders grouped nav (`settings-nav-group-app/integration`, `settings-nav-<id>`) and only the active section body (`settings-section-<id>`). Programmatic/state deep-link opens the addressed section on render.

## Tests

- `settings_nav_groups_render_all_twelve_sections`: both groups and all 12 nav items render.
- `settings_navigation_switches_rendered_section`: switching active section updates state and renders the selected section/content markers.
- `settings_section_is_deep_link_addressable_by_key`: every section key is addressable and renders; unknown key normalizes to `general`.
- Existing settings-related tests updated to select the new sub-section before interacting with controls moved out of the default view (appearance/dataBrowsing/editor).

## Gates

- GR2: `CARGO_TARGET_DIR="$PWD/target" cargo test -p datazen-ui --offline --lib -- --test-threads=1` — recorded in commit/report output.
- GR4: not applicable (no `datazen-rune-host` changes).
