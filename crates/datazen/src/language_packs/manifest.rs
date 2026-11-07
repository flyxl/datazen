//! 语言包 manifest 与校验规则单家。
//!
//! 安装（`install.rs` 的 `validate_pack_dir`）与启动扫描（`manager.rs`）
//! 共用这一份规则；宿主不复制任何判定。所有校验错误文案都是线契约
//! （宿主把它映射成 Validation 错误原样展示给用户）。

use serde::{Deserialize, Serialize};

/// 包格式版本；`packVersion` 不等于它则整包拒绝（前向：v2 由未来裁决）。
pub const PACK_VERSION: u32 = 1;

/// 包内 manifest 的文件名（ZIP 根层、安装目录内同名）。
pub const MANIFEST_FILE: &str = "manifest.json";

/// 程序内置、永远进 bundle 的语言代码：外包装这些 code 一律拒绝
/// （内置优先，装了也白装，直接明确报错）。
pub const BUILTIN_LOCALE_CODES: &[&str] = &["en", "zh-CN"];

/// 允许出现在包里的域字典文件名（不含 `.json` 后缀）。
///
/// 与 `packages/frontend/src/locales/domains.ts` 的权威域清单一一对应：
/// 九个常驻域 + 四个懒域。懒域在外装包里是普通文件（一次性整体注册，
/// 不参与 bundle 的代码分割）。
pub const LANGUAGE_PACK_DOMAINS: &[&str] = &[
    // ── 常驻域（EAGER_DOMAINS）──
    "core",
    "connection",
    "schema",
    "query",
    "settings",
    "chart",
    "backup",
    "ai",
    "onboarding",
    // ── 懒域（LAZY_DOMAINS）──
    "sync",
    "workflows",
    "dashboard",
    "mcp",
];

/// 语言包 manifest（`manifest.json` 反序列化目标）。
///
/// `deny_unknown_fields`：字段拼错的包按「假安装」风险处理，明确拒绝而
/// 不是静默忽略（fail-closed）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LanguagePackManifest {
    pub pack_version: u32,
    pub code: String,
    pub label: String,
    pub version: String,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
}

impl LanguagePackManifest {
    /// 全量校验（安装与扫描共用）。错误文案见各校验函数。
    pub fn validate(&self) -> Result<(), String> {
        if self.pack_version != PACK_VERSION {
            return Err(format!(
                "invalid language pack: unsupported packVersion {} (expected {})",
                self.pack_version, PACK_VERSION
            ));
        }
        validate_locale_code(&self.code)?;
        if BUILTIN_LOCALE_CODES.contains(&self.code.as_str()) {
            return Err(format!(
                "invalid language pack: locale code conflicts with built-in: {}",
                self.code
            ));
        }
        if self.label.trim().is_empty() {
            return Err("invalid language pack: empty label".to_string());
        }
        if has_control_chars(&self.label) {
            return Err("invalid language pack: label contains control characters".to_string());
        }
        if !is_valid_pack_version_string(&self.version) {
            return Err(format!(
                "invalid language pack: invalid version `{}`",
                self.version
            ));
        }
        for (field, value) in [("author", &self.author), ("description", &self.description)] {
            if let Some(text) = value {
                if has_control_chars_except_newline(text) {
                    return Err(format!(
                        "invalid language pack: {field} contains control characters"
                    ));
                }
            }
        }
        Ok(())
    }
}

/// 语言代码格式：`^[a-z]{2,3}(-[A-Za-z0-9]{2,8}){0,2}$`（语言-地区）。
///
/// 主码强制小写（`ja`、`pt-BR` 合法；`EN` 非法）。这是格式校验而非内容
/// 上限：决定安装目录名与翻译引擎里的语言标识，拼错即拒。
pub fn validate_locale_code(code: &str) -> Result<(), String> {
    if !is_valid_locale_code(code) {
        return Err(format!(
            "invalid language pack: invalid locale code `{code}`"
        ));
    }
    Ok(())
}

fn is_valid_locale_code(code: &str) -> bool {
    let mut segments = code.split('-');
    let primary = segments.next().unwrap_or_default();
    let primary_ok = (primary.len() == 2 || primary.len() == 3)
        && primary.chars().all(|c| c.is_ascii_lowercase());
    if !primary_ok {
        return false;
    }
    let mut subtags = 0usize;
    for segment in segments {
        if !(2..=8).contains(&segment.len()) || !segment.chars().all(|c| c.is_ascii_alphanumeric())
        {
            return false;
        }
        subtags += 1;
    }
    subtags <= 2
}

/// 包自身版本号：宽松 semver（`1.2.3` 或 `1.2.3-beta.1` 形）。
fn is_valid_pack_version_string(value: &str) -> bool {
    let (core, prerelease) = match value.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (value, None),
    };
    let mut core_parts = 0usize;
    for part in core.split('.') {
        if part.is_empty() || !part.chars().all(|c| c.is_ascii_digit()) {
            return false;
        }
        core_parts += 1;
    }
    if core_parts != 3 {
        return false;
    }
    if let Some(pre) = prerelease {
        if pre.is_empty()
            || !pre
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-'))
        {
            return false;
        }
    }
    true
}

/// 消息键格式：ASCII 可打印，字母/数字/下划线开头，其余位允许
/// `.` `:` `-` `_`。
///
/// 拒收清单（原型污染防线——前端注册表用 `Object.assign` 合并，`__proto__`
/// 会触发原型 setter）：精确 `__proto__` / `constructor` / `prototype`，以及
/// 一切 `__` 前缀键。现有 en 全量键（约 3.5k）均兼容本规则。
pub fn validate_message_key(key: &str) -> Result<(), String> {
    if !is_valid_message_key(key) {
        if key == "__proto__" || key == "constructor" || key == "prototype" || key.starts_with("__")
        {
            return Err(format!(
                "invalid language pack: forbidden message key `{key}`"
            ));
        }
        return Err(format!(
            "invalid language pack: invalid message key `{key}`"
        ));
    }
    Ok(())
}

fn is_valid_message_key(key: &str) -> bool {
    if key == "__proto__" || key == "constructor" || key == "prototype" {
        return false;
    }
    if key.starts_with("__") {
        return false;
    }
    let mut chars = key.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphanumeric() || first == '_') {
        return false;
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-' | '_'))
}

/// 消息译文：字符串（空串合法——运行时契约「An empty string is a real
/// translation, not a miss」，en 源字典自带 `pagination.pageOf: ''` 等空值
/// 键），允许换行，拒收其余 C0 控制符与 DEL。
pub fn validate_message_value(key: &str, value: &str) -> Result<(), String> {
    if has_control_chars_except_newline(value) {
        return Err(format!(
            "invalid language pack: message value for `{key}` contains control characters"
        ));
    }
    Ok(())
}

fn has_control_chars(text: &str) -> bool {
    text.chars().any(|c| {
        let code = c as u32;
        code < 0x20 || code == 0x7f
    })
}

fn has_control_chars_except_newline(text: &str) -> bool {
    text.chars().any(|c| {
        let code = c as u32;
        (code < 0x20 && c != '\n') || code == 0x7f
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest_json(code: &str) -> String {
        format!(r#"{{"packVersion":1,"code":"{code}","label":"Test","version":"1.0.0"}}"#)
    }

    fn parse(code: &str) -> LanguagePackManifest {
        serde_json::from_str(&manifest_json(code)).expect("valid manifest json")
    }

    #[test]
    fn valid_manifest_passes() {
        for code in [
            "ja", "ko", "pt-BR", "zh-TW", "de", "es", "fr", "ru", "en-001", "ceb",
        ] {
            assert!(parse(code).validate().is_ok(), "code {code} should pass");
        }
    }

    #[test]
    fn unknown_manifest_field_is_rejected() {
        let json = r#"{"packVersion":1,"code":"ja","label":"T","version":"1.0.0","extra":1}"#;
        let err = serde_json::from_str::<LanguagePackManifest>(json)
            .expect_err("deny_unknown_fields must reject");
        assert!(err.to_string().contains("unknown field"), "{err}");
    }

    #[test]
    fn wrong_pack_version_is_rejected() {
        let mut m = parse("ja");
        m.pack_version = 2;
        assert!(m
            .validate()
            .unwrap_err()
            .contains("unsupported packVersion"));
    }

    #[test]
    fn builtin_conflict_is_rejected() {
        for code in ["en", "zh-CN"] {
            let err = parse(code).validate().unwrap_err();
            assert!(err.contains("conflicts with built-in"), "{err}");
        }
    }

    #[test]
    fn invalid_locale_codes_are_rejected() {
        for code in [
            "",
            "E",
            "EN",
            "toolongcode",
            "j",
            "ja-",
            "ja--CN",
            "ja-CN-extra-third",
            "ja-TOOLONGSUBTAG",
            "日本語",
        ] {
            let err = validate_locale_code(code).unwrap_err();
            assert!(err.contains("invalid locale code"), "{code}: {err}");
        }
    }

    #[test]
    fn empty_label_is_rejected() {
        let mut m = parse("ja");
        m.label = "   ".into();
        assert!(m.validate().unwrap_err().contains("empty label"));
    }

    #[test]
    fn label_control_chars_are_rejected() {
        let mut m = parse("ja");
        m.label = "日本語\u{7}".into();
        assert!(m.validate().unwrap_err().contains("label contains control"));
    }

    #[test]
    fn version_matrix() {
        let mut m = parse("ja");
        for good in ["0.0.1", "1.2.3", "10.20.30", "1.2.3-beta.1", "1.2.3-rc-1"] {
            m.version = good.into();
            assert!(m.validate().is_ok(), "{good} should pass");
        }
        for bad in ["1.2", "1.2.3.4", "v1.2.3", "1.2.x", "1.2.3-", "", "latest"] {
            m.version = bad.into();
            assert!(m.validate().is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn author_description_control_chars_are_rejected() {
        let mut m = parse("ja");
        m.author = Some("ok\u{1f}".into());
        assert!(m
            .validate()
            .unwrap_err()
            .contains("author contains control"));
        m.author = None;
        m.description = Some("line1\nline2".to_string());
        assert!(m.validate().is_ok(), "newline in description is fine");
    }

    #[test]
    fn message_key_matrix() {
        for key in [
            "settings.language",
            "common.ok",
            "connWin.copyDDL",
            "grid.fk.unknownConstraint:",
            "a",
            "_private",
            "v2.key",
            "key-with-dash",
        ] {
            assert!(validate_message_key(key).is_ok(), "{key} should pass");
        }
        for key in [
            "",
            ".dot",
            "-dash",
            "has space",
            "日本語キー",
            "a\tb",
            "key#hash",
        ] {
            assert!(validate_message_key(key).is_err(), "{key} should fail");
        }
    }

    #[test]
    fn prototype_pollution_keys_are_forbidden() {
        for key in ["__proto__", "constructor", "prototype", "__defineGetter__"] {
            let err = validate_message_key(key).unwrap_err();
            assert!(err.contains("forbidden message key"), "{key}: {err}");
        }
    }

    #[test]
    fn message_value_matrix() {
        assert!(validate_message_value("k", "正常").is_ok());
        assert!(validate_message_value("k", "line1\nline2").is_ok());
        // 空串是合法译文（en 源字典自带空值键；运行时对 undefined 才判缺失）。
        assert!(validate_message_value("k", "").is_ok());
        for bad in ["\u{0}", "\u{1f}", "\u{7f}", "a\u{b}b"] {
            assert!(validate_message_value("k", bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn domain_whitelist_matches_locale_domains_ts() {
        // 与 packages/frontend/src/locales/domains.ts 的九常驻 + 四懒域对齐。
        assert_eq!(LANGUAGE_PACK_DOMAINS.len(), 13);
        for domain in [
            "core",
            "connection",
            "schema",
            "query",
            "settings",
            "chart",
            "backup",
            "ai",
            "onboarding",
            "sync",
            "workflows",
            "dashboard",
            "mcp",
        ] {
            assert!(LANGUAGE_PACK_DOMAINS.contains(&domain), "{domain}");
        }
    }
}
