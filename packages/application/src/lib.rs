//! `datazen-application`：平台用例层。
//!
//! ## 边界
//!
//! * 依赖：`datazen-platform-api`（+ 可选 `datazen-driver-api`）。**不依赖** `datazen-runtime`、
//!   **不依赖** Tauri、**不依赖** 任何 HTTP 框架（§2 依赖图：`APP → PA, DAPI`）。
//!   业务因此可以在没有 Tauri 的测试进程里调用。
//! * 职责：请求 DTO、用例签名、§4.3 目标计算、身份/授权判定顺序、对外错误形状
//!   [`ApiError`]、前端桥注入端口（[`frontend_bridge`]）。
//! * 不负责：把端口接上实现、HTTP/Tauri 状态映射、把端口错误翻译成业务错误。
//!   这些属于组装层（§6.2 第 7 步之后）。
//!
//! ## 模块
//!
//! | 模块 | 内容 |
//! |---|---|
//! | [`error`] | [`ApiError`]、[`ApiErrorCode`]、[`RetryDisposition`] |
//! | [`frontend_bridge`] | 前端桥窄端口 [`FrontendBridge`](frontend_bridge::FrontendBridge)、
//!   [`BridgeInjection`](frontend_bridge::BridgeInjection) 与进程级注入缝隙（§7.4 失败关闭） |
//! | [`dto`] | 用例请求 DTO、持久化记录，以及 platform-api 契约 DTO 的再导出 |
//! | [`target`] | §4.3 六步目标计算 |
//! | [`identity_policy`] | §5.1 身份与授权判定顺序 |
//! | [`sessions`] | [`ConnectionUseCases`](sessions::ConnectionUseCases) 用例面 |
//! | [`capability`] | 计划第 93 行的可选能力注册与运行时能力降低机制 |
//!
//! [`ApplicationServices`] 按 §6.2 第 7 步的顺序组装：
//! `target → identity_policy → sessions`，能力注册表由
//! [`with_capabilities`](ApplicationServices::with_capabilities) 在其后补挂，
//! 缺省是**全不可用**的注册表。构造函数对桌面与团队两种形态**完全相同**，
//! 只有传入的 `Arc<dyn …>` 实现不同——不允许为团队形态另写一套用例。

pub mod capability;
pub mod dto;
pub mod error;
pub mod frontend_bridge;
pub mod identity_policy;
pub mod sessions;
pub mod target;

use std::sync::Arc;

use crate::capability::CapabilityRegistry;
use crate::identity_policy::IdentityPolicy;
use crate::sessions::ConnectionUseCases;
use crate::target::TargetResolver;

/// 用例层服务聚合。字段顺序即 §6.2 第 7 步的组装顺序。
#[derive(Clone)]
pub struct ApplicationServices {
    /// §4.3 目标计算。
    pub target: TargetResolver,
    /// §5.1 身份与授权判定顺序。
    pub identity_policy: IdentityPolicy,
    /// 连接用例面（实现在组装层注入）。
    pub sessions: Arc<dyn ConnectionUseCases>,
    /// 计划第 93 行的运行时能力注册表。
    ///
    /// 缺省是全不可用的空注册表（[`CapabilityRegistry::new`]）。这意味着
    /// **忘记装配**的后果是所有能力取用都显式报
    /// [`ApiErrorCode::CapabilityUnsupported`](error::ApiErrorCode::CapabilityUnsupported)，
    /// 而不是「看起来都支持」。
    pub capabilities: CapabilityRegistry,
}

impl ApplicationServices {
    /// 能力注册表留空（全不可用）。随后用
    /// [`with_capabilities`](Self::with_capabilities) 挂上真实注册表。
    pub fn new(
        target: TargetResolver,
        identity_policy: IdentityPolicy,
        sessions: Arc<dyn ConnectionUseCases>,
    ) -> Self {
        Self {
            target,
            identity_policy,
            sessions,
            capabilities: CapabilityRegistry::new("unbound"),
        }
    }

    /// 挂上真实的能力注册表。
    pub fn with_capabilities(mut self, capabilities: CapabilityRegistry) -> Self {
        self.capabilities = capabilities;
        self
    }
}

#[cfg(test)]
mod tests {
    /// 去掉注释行后的源码。
    fn code_only(source: &str) -> String {
        source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn crate_root_code() -> String {
        code_only(
            include_str!("lib.rs")
                .split("#[cfg(test)]")
                .next()
                .unwrap_or_default(),
        )
    }

    /// 把注释与字符串/字符字面量的**内容**抹成空格（换行保留）。
    ///
    /// 之后的括号计数与语句切分因此不会被注释或字面量里的 `;` `{` `}` 干扰：
    /// `let s = "x; pub mod evil;";` 不会切出一条假的 `pub mod` 语句，
    /// 字符串里的 `{` 也不会把大括号深度带偏（深度一偏，整个守卫就失明）。
    fn mask_literals_and_comments(source: &str) -> String {
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(source.len());
        let mut i = 0usize;
        let mut block_depth = 0usize;

        while i < chars.len() {
            let c = chars[i];
            let next = chars.get(i + 1).copied();

            if c == '\n' {
                out.push('\n');
                i += 1;
                continue;
            }

            if block_depth > 0 {
                if c == '/' && next == Some('*') {
                    block_depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if c == '*' && next == Some('/') {
                    block_depth -= 1;
                    out.push_str("  ");
                    i += 2;
                } else {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }

            if c == '/' && next == Some('/') {
                while i < chars.len() && chars[i] != '\n' {
                    out.push(' ');
                    i += 1;
                }
                continue;
            }

            if c == '/' && next == Some('*') {
                block_depth = 1;
                out.push_str("  ");
                i += 2;
                continue;
            }

            if c == '"' {
                out.push(' ');
                i += 1;
                while i < chars.len() {
                    let d = chars[i];
                    if d == '\n' {
                        out.push('\n');
                        i += 1;
                        continue;
                    }
                    if d == '\\' {
                        out.push(' ');
                        out.push(' ');
                        i += 2;
                        continue;
                    }
                    out.push(' ');
                    i += 1;
                    if d == '"' {
                        break;
                    }
                }
                continue;
            }

            // 字符字面量 `'x'` / `'\\n'`；`'a` 这种生命周期不是字面量。
            if c == '\'' {
                let close = if chars.get(i + 1) == Some(&'\\') {
                    i + 3
                } else {
                    i + 2
                };
                if chars.get(close) == Some(&'\'') {
                    for _ in i..=close {
                        out.push(' ');
                    }
                    i = close + 1;
                    continue;
                }
            }

            out.push(c);
            i += 1;
        }

        out
    }

    /// 若 `s` 以**独立关键字** `kw` 开头，返回其后的剩余部分（`module` 不算 `mod`）。
    fn strip_keyword<'a>(s: &'a str, kw: &str) -> Option<&'a str> {
        let rest = s.strip_prefix(kw)?;
        match rest.chars().next() {
            None => Some(rest),
            Some(c) if c.is_alphanumeric() || c == '_' => None,
            Some(_) => Some(rest),
        }
    }

    /// 从 `s` 的开括号**之后**开始扫描，返回配对闭括号之后的部分。
    fn matching_delimiter<'a>(s: &'a str, open: char, close: char) -> Option<&'a str> {
        let mut depth = 1usize;
        for (offset, c) in s.char_indices() {
            if c == open {
                depth += 1;
            } else if c == close {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[offset + c.len_utf8()..]);
                }
            }
        }
        None
    }

    /// 取开头的标识符（含 `r#type` 这类原生标识符前缀），返回（标识符, 剩余部分）。
    fn split_identifier(s: &str) -> Option<(&str, &str)> {
        let start = if s.starts_with("r#") { 2 } else { 0 };
        let rest = &s[start..];
        let len = rest
            .bytes()
            .take_while(|b| b.is_ascii_alphanumeric() || *b == b'_')
            .count();
        if len == 0 || rest.as_bytes()[0].is_ascii_digit() {
            return None;
        }
        Some((&s[..start + len], &s[start + len..]))
    }

    /// crate 顶层一条语句的归类。
    #[derive(Debug, PartialEq, Eq)]
    enum RootStatement {
        /// 顶层 `pub mod`：计入 crate 的对外面。
        PublicModule(String),
        /// 顶层非 pub 模块声明（私有 / `pub(crate)` / `pub(super)` …）：同样改变模块结构。
        NonPublicModule(String),
        /// 可归类的非模块语句（`use` / `struct` / `impl` / 属性行 …）。
        KnownNoise,
        /// **解析器不认识** —— 必须 fail closed，绝不静默丢弃。
        Unrecognized,
    }

    /// 归类一条顶层语句。
    ///
    /// 本轨已经出现三次「换一种惯用写法就漏」，所以这里的取向是**宁可判成
    /// Unrecognized 让守卫变红，也不放过一条看不懂的语句**：新增惯用写法时守卫会
    /// 报错并打印那一条语句，而不是悄悄漏过。
    fn classify_root_statement(statement: &str) -> RootStatement {
        let mut rest = statement.trim();

        // 先剥掉开头的属性组 `#[…]` / `#![…]`（可多个）。
        loop {
            let trimmed = rest.trim_start();
            let Some(body) = trimmed
                .strip_prefix("#![")
                .or_else(|| trimmed.strip_prefix("#["))
            else {
                break;
            };
            match matching_delimiter(body, '[', ']') {
                Some(after) => rest = after,
                None => return RootStatement::Unrecognized,
            }
        }

        let rest = rest.trim();
        if rest.is_empty() {
            return RootStatement::KnownNoise; // 纯属性行 / 空行
        }

        // 可见性：`pub` / `pub(crate)` / `pub(super)` / `pub(in …)` / `pub(self)`。
        let mut is_plain_pub = false;
        let mut body = rest;
        if let Some(after_pub) = strip_keyword(body, "pub") {
            if let Some(group) = after_pub.strip_prefix('(') {
                match matching_delimiter(group, '(', ')') {
                    Some(after) => body = after.trim_start(),
                    None => return RootStatement::Unrecognized,
                }
            } else if after_pub.is_empty() || after_pub.starts_with(char::is_whitespace) {
                is_plain_pub = true;
                body = after_pub.trim_start();
            }
        }

        if let Some(after_mod) = strip_keyword(body, "mod") {
            let Some((name, tail)) = split_identifier(after_mod.trim_start()) else {
                return RootStatement::Unrecognized;
            };
            let tail = tail.trim();
            // `pub mod x;`（分号已在切分时去掉）或内联 `pub mod x { … }`。
            if tail.is_empty() || tail.starts_with('{') {
                return if is_plain_pub {
                    RootStatement::PublicModule(name.to_string())
                } else {
                    RootStatement::NonPublicModule(name.to_string())
                };
            }
            return RootStatement::Unrecognized;
        }

        // 其余可归类的顶层条目关键字。刻意**不含** `include` / 其它宏调用：
        // 它们能凭空展开出模块声明，属于必须显式表态的写法。
        for keyword in [
            "let",
            "use",
            "struct",
            "enum",
            "union",
            "trait",
            "impl",
            "fn",
            "const",
            "static",
            "type",
            "extern",
            "unsafe",
            "async",
            "macro_rules",
        ] {
            if strip_keyword(body, keyword).is_some() {
                return RootStatement::KnownNoise;
            }
        }

        RootStatement::Unrecognized
    }

    /// crate 顶层的模块声明。
    #[derive(Debug, PartialEq, Eq)]
    struct ModuleDeclarations {
        /// 顶层 `pub mod`（crate 的对外面）。
        public: Vec<String>,
        /// 顶层非 pub 模块声明；当前唯一允许的是 `#[cfg(test)] mod tests`。
        non_public: Vec<String>,
    }

    /// 抽出 crate **顶层**（大括号深度 0）的模块声明，**全文**扫描。
    ///
    /// 三条硬约束：
    /// * 不做 `split("#[cfg(test)]").next()` 式的整段截断 —— 追加在测试模块之后的声明也要看见；
    /// * 只有深度 0 的语句才算顶层条目 —— 宏体、函数体、测试模块里的同形语句不是根模块
    ///   （`macro_rules!` 体内孤立的 `pub mod x;` 因此不会误报）；
    /// * **fail closed**：顶层每一条语句都必须能归类，看不懂就返回 Err（带行号与语句原文），
    ///   绝不静默丢弃 —— 新增一种惯用写法时守卫变红，而不是悄悄漏过。
    fn declared_modules(source: &str) -> Result<ModuleDeclarations, String> {
        let masked = mask_literals_and_comments(source);
        let mut declarations = ModuleDeclarations {
            public: Vec::new(),
            non_public: Vec::new(),
        };
        let mut depth: i32 = 0;

        for (index, line) in masked.lines().enumerate() {
            if depth == 0 {
                for statement in line.split(';').map(str::trim).filter(|s| !s.is_empty()) {
                    match classify_root_statement(statement) {
                        RootStatement::PublicModule(name) => declarations.public.push(name),
                        RootStatement::NonPublicModule(name) => declarations.non_public.push(name),
                        RootStatement::KnownNoise => {}
                        RootStatement::Unrecognized => {
                            return Err(format!("第 {} 行：{statement}", index + 1));
                        }
                    }
                }
            }

            depth += line.chars().filter(|c| *c == '{').count() as i32;
            depth -= line.chars().filter(|c| *c == '}').count() as i32;
            if depth < 0 {
                depth = 0;
            }
        }

        declarations.public.sort();
        declarations.non_public.sort();
        Ok(declarations)
    }

    #[test]
    fn the_service_aggregate_follows_the_assembly_order() {
        let code = crate_root_code();
        let target = code
            .find("pub target: TargetResolver")
            .expect("target 字段");
        let identity = code
            .find("pub identity_policy: IdentityPolicy")
            .expect("identity_policy 字段");
        let sessions = code
            .find("pub sessions: Arc<dyn ConnectionUseCases>")
            .expect("sessions 字段");
        assert!(target < identity, "顺序必须是 target → identity_policy");
        assert!(identity < sessions, "顺序必须是 identity_policy → sessions");
    }

    #[test]
    fn the_crate_does_not_depend_on_the_runtime_or_on_tauri() {
        let manifest = include_str!("../Cargo.toml");
        assert!(!manifest.contains("datazen-runtime"), "APP 不得依赖 RT");
        assert!(!manifest.contains("tauri"), "APP 不得依赖 tauri");
        for banned in ["axum", "actix-web", "warp", "tonic", "reqwest", "hyper"] {
            assert!(
                !manifest.contains(banned),
                "APP 不得依赖 HTTP 框架：{banned}"
            );
        }
        assert!(
            manifest.contains("datazen-platform-api"),
            "必须依赖 platform-api"
        );
    }

    #[test]
    fn the_module_list_is_exactly_the_seven_documented_units() {
        let declarations = declared_modules(include_str!("lib.rs")).unwrap_or_else(|unclassified| {
            panic!(
                "模块清单守卫 fail closed：crate 顶层出现一条无法归类的语句，\
                 请显式表态，而不是让它悄悄改变本 crate 的模块结构。{unclassified}"
            )
        });
        assert_eq!(
            declarations.public,
            [
                "capability",
                "dto",
                "error",
                "frontend_bridge",
                "identity_policy",
                "sessions",
                "target",
            ],
            "本 crate 的对外模块面是封闭的七单元：多一个或少一个 `pub mod` 都必须在这里显式表态，\
             并同步 lib.rs 顶部的模块表与架构文档"
        );
        assert_eq!(
            declarations.non_public,
            ["tests"],
            "顶层非 pub 模块只允许 `#[cfg(test)] mod tests`：私有 `mod x;` 或 `pub(crate) mod x;` \
             同样会改变 crate 的模块结构，必须显式表态"
        );
    }

    fn parse_modules(lines: &[&str]) -> Vec<String> {
        declared_modules(&lines.join("\n"))
            .expect("自测输入必须全部可归类")
            .public
    }

    /// 解析器自测探针（惯用写法）：行尾注释必须被剥掉，声明不能被静默丢弃。
    /// 曾经的实现直接 `strip_suffix(';')`，`pub mod probe; // c` 整行返回 None 被丢掉。
    #[test]
    fn the_module_parser_sees_through_a_trailing_line_comment() {
        assert_eq!(parse_modules(&["pub mod probe; // c"]), ["probe"]);
        assert_eq!(
            parse_modules(&["pub mod a;", "pub mod b; // 说明"]),
            ["a", "b"]
        );
    }

    /// 解析器自测探针（尾部位置）：追加在 `#[cfg(test)] mod tests` **之后**的声明
    /// 也必须被看见 —— 曾经用 `split("#[cfg(test)]").next()` 整段截断，尾部新增全漏。
    #[test]
    fn the_module_parser_does_not_stop_at_the_test_module() {
        let source = [
            "pub mod alpha;",
            "#[cfg(test)]",
            "mod tests {",
            "const S: &str = 'pub mod capability;';",
            "}",
            "pub mod omega; // 追在测试模块之后",
        ];
        assert_eq!(parse_modules(&source), ["alpha", "omega"]);
    }

    /// 解析器自测探针（块注释）：块注释里的 `pub mod` 不是声明 ——
    /// 不剥块注释会让守卫对一个纯注释文件误报（实测该方法曾 EXIT=101）。
    #[test]
    fn the_module_parser_ignores_block_comments() {
        assert!(parse_modules(&["/* pub mod fake; */"]).is_empty());
        assert_eq!(
            parse_modules(&["/*", "pub mod fake;", "*/", "pub mod real;"]),
            ["real"]
        );
    }

    /// 解析器自测探针：把解析器**自身的盲区**纳入覆盖。
    /// 正例覆盖惯用写法（行尾注释 / **属性前缀** / `pub` 与 `mod` 之间多空格 / 一行两条）；
    /// 反例确保字符串字面量与注释里的同形文本不被误认。
    #[test]
    fn the_module_parser_covers_its_own_blind_spots() {
        assert_eq!(parse_modules(&["pub mod probe; // c"]), ["probe"]);
        assert_eq!(
            parse_modules(&["pub mod alpha; pub mod omega;"]),
            ["alpha", "omega"]
        );
        assert_eq!(
            parse_modules(&["#[allow(dead_code)] pub mod attributed;"]),
            ["attributed"]
        );
        assert_eq!(
            parse_modules(&["#[path = \"x.rs\"] pub mod pathed;"]),
            ["pathed"]
        );
        assert_eq!(parse_modules(&["pub  mod spaced;"]), ["spaced"]);

        for non_declaration in [
            "let s = \"pub mod in_string;\";",
            "let s = \"x; pub mod evil;\";",
            "// pub mod commented_out;",
            "/* pub mod fake; */",
        ] {
            assert!(
                parse_modules(&[non_declaration]).is_empty(),
                "不应被当成声明：{non_declaration}"
            );
        }
    }

    /// 解析器自测探针：`macro_rules!` 体内的孤立声明不是 crate 顶层的模块声明
    /// （顶层 = 大括号深度 0；宏体、函数体、测试模块都在深度 > 0）。
    #[test]
    fn the_module_parser_does_not_cross_into_a_macro_body() {
        let source = [
            "pub mod real;",
            "macro_rules! fake_macro {",
            "    () => {",
            "        pub mod fake_macro;",
            "    };",
            "}",
        ];
        assert_eq!(parse_modules(&source), ["real"]);
    }

    /// 解析器自测探针：**无法归类的顶层语句必须 fail closed**（返回 Err 并带上语句原文），
    /// 而不是被当空行跳过。本轨已经出现三次「换一种惯用写法就漏」，这条是那个止损点。
    #[test]
    fn the_module_parser_fails_closed_on_a_statement_it_cannot_classify() {
        for (source, needle) in [
            ("make_modules!();", "make_modules!"),
            ("include!(\"mods.rs\");", "include!"),
            ("pub mod a b;", "pub mod a b"),
        ] {
            let err = declared_modules(source)
                .expect_err(&format!("这条语句必须 fail closed，而不是被静默丢弃：{source}"));
            assert!(
                err.contains(needle),
                "错误里必须带上那一条语句原文（{needle}）：{err}"
            );
        }

        for known_noise in [
            "use std::sync::Arc;",
            "pub struct S {",
            "impl S {",
            "#[derive(Clone)]",
        ] {
            assert!(
                declared_modules(known_noise).is_ok(),
                "已知噪声不应 fail closed：{known_noise}"
            );
        }
    }

    /// 真实缺陷：忘了装配能力注册表，于是所有能力取用都变成 no-op 或 panic。
    /// `new` 必须挂上**全不可用**的注册表，让遗漏当场变成 `CapabilityUnsupported`；
    /// 真实注册表只能由 `with_capabilities` 显式补挂。
    #[test]
    fn the_aggregate_starts_with_a_fail_closed_capability_registry() {
        let code = crate_root_code();
        let capability_field = code
            .find("pub capabilities: CapabilityRegistry")
            .expect("capabilities 字段");
        let sessions_field = code
            .find("pub sessions: Arc<dyn ConnectionUseCases>")
            .expect("sessions 字段");
        assert!(
            sessions_field < capability_field,
            "能力注册表排在既有用例面之后，不打断 target → identity_policy → sessions 的组装顺序"
        );
        assert!(
            code.contains("capabilities: CapabilityRegistry::new(\"unbound\")"),
            "缺省必须是全不可用注册表"
        );
        assert!(
            code.contains("pub fn with_capabilities"),
            "真实注册表只能显式补挂"
        );
    }

    #[test]
    fn the_error_type_is_constructible_from_the_code_enum() {
        // 宿主映射层只需要 `datazen_application::error::ApiError`，不必知道模块内部形状。
        let error = crate::error::ApiError::new(
            crate::error::ApiErrorCode::TargetRequired,
            "缺少 database 层".to_string(),
        );
        assert_eq!(error.code, crate::error::ApiErrorCode::TargetRequired);
    }
}
