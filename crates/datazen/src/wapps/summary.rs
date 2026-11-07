//! The installed-wapp summary projection served over IPC.
//!
//! B2 wapps 域刀下沉物（原桌面 `commands/wapps/mod.rs` 的 `WappSummary`
//! 三兄弟）：字段键（serde camelCase）与「从 `LoadedWapp` 投影」的规则是
//! 线契约——桌面与将来的 Web 宿主对同一份注册表投影出同一个 JSON。
//! 序列化形状的钉子测试（原宿主 `summary_serializes_camel_case_payload`）
//! 随逻辑归位本模块。

use serde::Serialize;

use super::manager::LoadedWapp;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappPageSummary {
    pub id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappThemeSummary {
    pub id: String,
    pub name: String,
    pub modes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappAiToolSummary {
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WappSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub enabled: bool,
    pub permissions: Vec<String>,
    pub pages: Vec<WappPageSummary>,
    pub themes: Vec<WappThemeSummary>,
    /// AI tools declared by the manifest, for the host's opt-in settings UI.
    /// The tools are still unavailable until the plugin id is enabled there.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<WappAiToolSummary>,
}

impl From<&LoadedWapp> for WappSummary {
    fn from(wapp: &LoadedWapp) -> Self {
        let manifest = &wapp.manifest;
        Self {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            api_version: manifest.api_version,
            author: manifest.author.clone(),
            description: manifest.description.clone(),
            icon: manifest.icon.clone(),
            enabled: wapp.enabled,
            permissions: manifest
                .permissions
                .iter()
                .map(|permission| permission.as_str().to_string())
                .collect(),
            pages: manifest
                .contributes
                .pages
                .iter()
                .map(|page| WappPageSummary {
                    id: page.id.clone(),
                    title: page.title.clone(),
                    icon: page.icon.clone(),
                })
                .collect(),
            themes: manifest
                .contributes
                .themes
                .iter()
                .map(|theme| WappThemeSummary {
                    id: theme.id.clone(),
                    name: theme.name.clone(),
                    modes: theme.modes.clone(),
                })
                .collect(),
            ai_tools: manifest
                .ai
                .as_ref()
                .map(|ai| {
                    ai.tools
                        .iter()
                        .map(|tool| WappAiToolSummary {
                            name: tool.name.clone(),
                            description: tool.description.clone(),
                            read_only: tool.read_only,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    const DEMO_MANIFEST: &str = r#"{
      "id": "acme.demo",
      "name": "Demo Wapp",
      "version": "1.0.0",
      "apiVersion": 2,
      "author": "Acme",
      "entry": "index.html",
      "contributes": {
        "pages": [{ "id": "main", "title": "Main", "icon": "assets/icon.svg" }],
        "themes": [{
          "id": "demo-dark",
          "name": "Demo Dark",
          "tokensCss": "themes/demo-dark/tokens.css",
          "modes": ["dark"]
        }]
      },
      "permissions": ["storage:local", "command:invoke"]
    }"#;

    fn write_demo_dir(dir: &Path) {
        for (name, content) in [
            ("manifest.json", DEMO_MANIFEST),
            ("index.html", "<html>demo</html>"),
            (
                "assets/icon.svg",
                "<svg xmlns='http://www.w3.org/2000/svg'/>",
            ),
            ("themes/demo-dark/tokens.css", ":root { --c-accent: red; }"),
        ] {
            let path = dir.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, content).unwrap();
        }
    }

    /// 迁自桌面 `commands/wapps/tests.rs`（B2 域刀：DTO 投影与线格式随逻辑
    /// 归位 modules；桌面/服务端共用同一份 camelCase 载荷）。
    #[test]
    fn summary_serializes_camel_case_payload() {
        let dir = tempfile::TempDir::new().unwrap();
        let pack = dir.path().join("acme.demo");
        write_demo_dir(&pack);
        let manifest = super::super::validate_wapp_dir(&pack).unwrap();
        let summary = WappSummary::from(&LoadedWapp {
            manifest,
            enabled: true,
        });
        let json = serde_json::to_value(&summary).unwrap();
        assert_eq!(json["apiVersion"], 2);
        assert_eq!(json["enabled"], true);
        assert_eq!(json["author"], "Acme");
        assert_eq!(json["permissions"][0], "storage:local");
        assert_eq!(json["pages"][0]["id"], "main");
        assert_eq!(json["pages"][0]["icon"], "assets/icon.svg");
        assert_eq!(json["themes"][0]["id"], "demo-dark");
        assert_eq!(json["themes"][0]["modes"][0], "dark");
        // description is None in the manifest → omitted from the payload.
        assert!(json.get("description").is_none());
    }
}
