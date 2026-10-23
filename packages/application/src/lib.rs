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
//! | [`settings`] | 跨形态设置校验与仓储用例 |
//! | [`sessions`] | [`ConnectionUseCases`](sessions::ConnectionUseCases) 用例面 |
//! | [`capability`] | 计划第 93 行的可选能力注册与运行时能力降低机制 |
//!
//! [`ApplicationServices`] 按 §6.2 第 7 步的顺序组装：
//! `target → identity_policy → sessions → settings → tunnels → known_hosts`，能力注册表由
//! [`with_capabilities`](ApplicationServices::with_capabilities) 在其后补挂，
//! 缺省是**全不可用**的注册表。构造函数对桌面与团队两种形态**完全相同**，
//! 只有传入的 `Arc<dyn …>` 实现不同——不允许为团队形态另写一套用例。
//!
//! ## 注入的存储端口
//!
//! `settings`、`tunnels` 与 `known_hosts` 是**形态中立的存储端口**（契约住在
//! `datazen-platform-api` 的 `ports/`）：应用层**持有**它们，而不是反过来
//! 借用宿主的 store。端口是**构造参数**而不是可选字段——漏注入是**编译错误**，
//! 不是运行时的 `None`。桌面实现是 `DesktopRepository`（组装期注入同一个实例），
//! 将来的 web 形态换成服务端 host 的实现，**用例代码一行不改**。
//!
//! 端口契约只能来自 `datazen-platform-api`：领域包（schema-diff / data-sync /
//! data-transfer）的端口**进不来**——`datazen-runtime` 依赖本包，领域包又依赖
//! `datazen-runtime`，本包一旦依赖领域包就是 Cargo 环（`cargo metadata` 直接
//! 报 `cyclic package dependency`）。需要领域端口时由宿主在**组装期**把它包成
//! `platform-api` 层的窄端口再注入。

pub mod capability;
pub mod dto;
pub mod error;
pub mod frontend_bridge;
pub mod identity_policy;
pub mod sessions;
pub mod settings;
pub mod target;

use std::sync::Arc;

use datazen_platform_api::ports::known_hosts::KnownHostsRepository;
use datazen_platform_api::ports::{SettingsRepository, TunnelRepository};

use datazen_feature_core::services::named_views::NamedViewRepository;

use crate::capability::CapabilityRegistry;
use crate::identity_policy::IdentityPolicy;
use crate::sessions::ConnectionUseCases;
use crate::settings::SettingsService;
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
    /// 跨形态设置仓储端口（组装层注入）。
    ///
    /// 桌面实现是 `DesktopRepository`（加密落盘 `{appData}`）；将来的 web 形态
    /// 由服务端 host 实现。**事实只有一份**：本字段与宿主持有的必须是同一个实例。
    pub settings: Arc<dyn SettingsRepository>,
    /// 隧道定义的存储端口（组装层注入）。
    ///
    /// 隧道定义是独立聚合：连接侧只存 `tunnelId`，解引用发生在建连时。
    /// 桌面实现同样是 `DesktopRepository`；web 形态由服务端 host 实现。
    pub tunnels: Arc<dyn TunnelRepository>,
    /// SSH known-hosts 的存储端口（组装层注入）。
    ///
    /// 端口与词汇表住在 `datazen_platform_api::ports::known_hosts`；**文件 I/O** 是
    /// 形态的事（桌面读 `{appData}/ssh_known_hosts.json`）。事实只有一份：本字段与
    /// 注给会话后端的是同一个实例。
    pub known_hosts: Arc<dyn KnownHostsRepository>,
    /// 命名视图的存储端口（组装层注入）。
    ///
    /// 这是 P3「按命令族搬编排」的第一条领域端口：端口与规则住在
    /// `datazen_feature_core::services::named_views`，**桌面宿主只提供落盘**。
    /// 聚合持有它之后，命令壳不再直读宿主 store——形态中立的用例层因此可以在
    /// 没有 Tauri 的进程里跑同一份规则。
    pub named_views: Arc<dyn NamedViewRepository>,
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
    ///
    /// 端口是**必填构造参数**：漏注入是编译错误，而不是运行时的 `None`。
    /// 参数会随 P3 的命令族逐步增长；增长到难以阅读时再分组（届时仍保持
    /// 「漏一个就是编译错误」，不退化成 `Option` 或 builder 的静默默认）。
    /// 桌面与团队两种形态调用的是**同一个**构造函数，只有传入的 `Arc<dyn …>`
    /// 实现不同。
    pub fn new(
        target: TargetResolver,
        identity_policy: IdentityPolicy,
        sessions: Arc<dyn ConnectionUseCases>,
        settings: Arc<dyn SettingsRepository>,
        tunnels: Arc<dyn TunnelRepository>,
        known_hosts: Arc<dyn KnownHostsRepository>,
        named_views: Arc<dyn NamedViewRepository>,
    ) -> Self {
        Self {
            target,
            identity_policy,
            sessions,
            settings,
            tunnels,
            known_hosts,
            named_views,
            capabilities: CapabilityRegistry::new("unbound"),
        }
    }

    /// 挂上真实的能力注册表。
    pub fn with_capabilities(mut self, capabilities: CapabilityRegistry) -> Self {
        self.capabilities = capabilities;
        self
    }

    /// 跨形态设置用例入口。
    ///
    /// 仓库由本聚合**持有**（不是借用宿主的 store），因此设置用例可以在没有
    /// Tauri 的进程里跑。桌面与将来的 web 形态走**同一个**入口，只有注入的
    /// [`SettingsRepository`] 实现不同。
    pub fn settings_service(&self) -> SettingsService<'_> {
        SettingsService::new(self.settings.as_ref())
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

    /// 源码里第 `line` 行（1-based）的原文，用于把诊断钉到具体那一句。
    fn source_line_text(source: &str, line: usize) -> String {
        source
            .lines()
            .nth(line.saturating_sub(1))
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// span 的**精确源码行号**（1-based）。fallback 模式下 proc-macro2 打开
    /// `span-locations` 后 `start().line` 就是真实行号；拿不到位置时返回 0。
    fn span_line(span: proc_macro2::Span) -> usize {
        span.start().line
    }

    /// crate 顶层的模块声明。
    #[derive(Debug, PartialEq, Eq)]
    struct ModuleDeclarations {
        /// 顶层 `pub mod`（crate 的对外面）。
        public: Vec<String>,
        /// 顶层非 pub 模块声明；当前唯一允许的是 `#[cfg(test)] mod tests`。
        non_public: Vec<String>,
    }

    /// 用 `syn` 取 crate **顶层**的模块声明。
    ///
    /// 前四轮都在手写词法/语句切分，每一轮都被新的边界打穿：行尾注释、属性前缀、
    /// 多空格、块注释、`#[cfg(test)]` 之后的尾部、片段里**并排多个 item**、
    /// `}` 行尾随、花括号内的分号……边界属于真正的 Rust 解析器，不属于我们猜。
    /// 所以这里直接 `syn::parse_file` 拿顶层 items，再按 `Item::Mod` 归类，
    /// 并对 syn 给的 span 取**精确源码行号**。
    ///
    /// 保守面（fail closed，宁可红也不静默）：
    /// * **顶层宏调用**（`make_mod!();`）可能凭空展开出模块声明 —— 必须显式表态；
    /// * `Item::Verbatim`（syn 无法建模的 item）同理；
    /// * 源码解析失败时直接报错，并给出 syn 报的源码行号与那一行原文。
    /// 注：`macro_rules!` **定义**不是宏调用（`Item::Macro.ident.is_some()`），
    /// 其宏体不在此展开、也不新增根模块，归为噪声。
    fn declared_modules(source: &str) -> Result<ModuleDeclarations, String> {
        let file = syn::parse_file(source).map_err(|e| {
            let line = span_line(e.span());
            format!(
                "第 {line} 行：{}（syn 解析失败：{e}）",
                source_line_text(source, line)
            )
        })?;

        let mut declarations = ModuleDeclarations {
            public: Vec::new(),
            non_public: Vec::new(),
        };

        for item in &file.items {
            if let syn::Item::Mod(module) = item {
                let name = module.ident.to_string();
                if matches!(module.vis, syn::Visibility::Public(_)) {
                    declarations.public.push(name);
                } else {
                    declarations.non_public.push(name);
                }
                continue;
            }

            let may_expand_to_a_module = match item {
                // `foo!();`：宏调用可能展开出 `pub mod …`；`macro_rules!` 定义有 ident，不算。
                syn::Item::Macro(mac) => mac.ident.is_none(),
                // syn 建模不了的 item，同样不敢替它担保。
                syn::Item::Verbatim(_) => true,
                _ => false,
            };
            if may_expand_to_a_module {
                let line = span_line(syn::spanned::Spanned::span(item));
                return Err(format!(
                    "第 {line} 行：{}（这句可能在编译期展开出模块声明，必须显式表态）",
                    source_line_text(source, line)
                ));
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

    /// 本包**可以**依赖共享 feature 层，但仍不得依赖连接运行时。
    ///
    /// 这条断言曾经是反的（「不得依赖任何领域包」），理由确是硬的：那时
    /// `datazen-runtime` 依赖本包、领域包又依赖 `datazen-runtime`，于是
    /// `APP → 领域包 → RT → APP` 是一个 Cargo 环，`cargo metadata` 直接拒绝。
    ///
    /// P0 断环（把端口与 `ApiError` 迁到 platform-api）与 P2 收官（领域包摘掉
    /// `datazen-runtime`，机制内核拆成 `datazen-kernel`）之后那个前提消失了：
    /// 现在 `APP → feature-core → kernel/platform-api` 是**正确方向**——用例层
    /// 编排领域服务，领域服务不认识用例层。P3「按命令族搬编排」正是沿这条边走。
    ///
    /// 仍然钉住的是**反向**那条：本包不得把连接运行时拉回来（`datazen-runtime`），
    /// 也不得依赖宿主形态（`tauri` 与 HTTP 框架）。环的判定交给
    /// `scripts/check-ddd-layers.mjs`——它按**当前**依赖图求值，而不是靠这条
    /// manifest 禁令的记忆。
    #[test]
    fn the_crate_may_depend_on_the_shared_feature_layer_but_not_on_the_runtime() {
        let manifest = include_str!("../Cargo.toml");
        assert!(
            manifest.contains("datazen-feature-core"),
            "P3 起用例层要能从共享 feature 层取领域端口；删掉这条依赖等于把编排\
             又推回宿主命令壳"
        );
        for banned in ["datazen-runtime", "tauri"] {
            assert!(
                !manifest.contains(banned),
                "APP 不得依赖 {banned}：前者会让「用例层 → 连接运行时」这条边回来，\
                 后者把形态带进用例层"
            );
        }
    }

    /// 注入的存储端口必须是 `platform-api` 的**那一份**，不是本包另抄的一份。
    ///
    /// 另抄一份端口 = 另抄一份事实：桌面实现与 web 实现各实现一套形状，差异
    /// 只在运行期暴露。所以这里断言两件事——端口类型来自
    /// `datazen_platform_api::ports`，且本包**没有**自己定义同名 trait。
    #[test]
    fn the_storage_ports_are_the_platform_api_ones_not_a_local_copy() {
        let code = crate_root_code();
        assert!(
            code.contains(
                "use datazen_platform_api::ports::{SettingsRepository, TunnelRepository};"
            ),
            "存储端口必须直接引 platform-api 的那一份"
        );
        for port in [
            "SettingsRepository",
            "TunnelRepository",
            "KnownHostsRepository",
            "NamedViewRepository",
        ] {
            assert!(
                !code.contains(&format!("pub trait {port}")),
                "{port} 不得在本包另定义一份：那是两份事实"
            );
        }
    }

    /// 聚合**持有**注入的存储端口：字段是 `Arc<dyn …>`，不是 `Option`。
    ///
    /// `Option` 会让「忘记注入」变成运行时的 `None`，而调用点会照样编译通过、
    /// 在第一次取用时才炸。必填构造参数把它变成**编译错误**。
    #[test]
    fn the_aggregate_holds_the_form_neutral_storage_ports() {
        let code = crate_root_code();
        for field in [
            "pub settings: Arc<dyn SettingsRepository>",
            "pub tunnels: Arc<dyn TunnelRepository>",
            "pub known_hosts: Arc<dyn KnownHostsRepository>",
        ] {
            assert!(code.contains(field), "聚合必须持有该存储端口字段：{field}");
        }
        assert!(
            !code.contains("pub settings: Option<"),
            "settings 端口不得是 Option：漏注入必须是编译错误"
        );
        assert!(
            !code.contains("pub tunnels: Option<"),
            "tunnels 端口不得是 Option：漏注入必须是编译错误"
        );
        assert!(
            !code.contains("pub known_hosts: Option<"),
            "known_hosts 端口不得是 Option：漏注入必须是编译错误"
        );
        // 构造签名必须**要求**这两个端口。
        let signature = code
            .split("pub fn new(")
            .nth(1)
            .and_then(|rest| rest.split(") -> Self").next())
            .expect("必须能取到 new 的参数表");
        for parameter in [
            "settings: Arc<dyn SettingsRepository>",
            "tunnels: Arc<dyn TunnelRepository>",
            "known_hosts: Arc<dyn KnownHostsRepository>",
        ] {
            assert!(
                signature.contains(parameter),
                "构造参数必须要求注入端口：{parameter}（实际：{}）",
                signature.split_whitespace().collect::<Vec<_>>().join(" ")
            );
        }
    }

    #[test]
    fn the_module_list_is_exactly_the_seven_documented_units() {
        let declarations =
            declared_modules(include_str!("lib.rs")).unwrap_or_else(|unclassified| {
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
                "settings",
                "target",
            ],
            "本 crate 的对外模块面是封闭的八单元：多一个或少一个 `pub mod` 都必须在这里显式表态，\
             并同步 lib.rs 顶部的模块表与架构文档"
        );
        assert_eq!(
            declarations.non_public,
            ["storage_ports_tests", "tests"],
            "顶层非 pub 模块只允许 `#[cfg(test)]` 模块：私有 `mod x;` 或 `pub(crate) mod x;` \
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
            "    const S: &str = \"pub mod capability;\";",
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
            "const NOISE: &str = \"pub mod in_string;\";",
            "const EVIL: &str = \"x; pub mod evil;\";",
            "// pub mod commented_out;",
            "/* pub mod fake; */",
        ] {
            assert!(
                parse_modules(&[non_declaration]).is_empty(),
                "不应被当成声明：{non_declaration}"
            );
        }
    }

    /// 解析器自测探针：`macro_rules!` 体内的孤立声明**不是** crate 顶层的模块声明 ——
    /// 宏体的 token 不在此展开，syn 也不会把它当成顶层 item。
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
            let err = declared_modules(source).expect_err(&format!(
                "这条语句必须 fail closed，而不是被静默丢弃：{source}"
            ));
            assert!(
                err.contains(needle),
                "错误里必须带上那一条语句原文（{needle}）：{err}"
            );
        }

        for known_noise in [
            "use std::sync::Arc;",
            "pub struct S { a: u8 }",
            "impl S { }",
            "#[derive(Clone)] pub struct T;",
        ] {
            assert!(
                declared_modules(known_noise).is_ok(),
                "已知噪声不应 fail closed：{known_noise}"
            );
        }
    }

    /// 解析器自测探针（第四轮 F-1 / H1–H4）：**同一个 item 行上并排的模块声明**不能被静默丢弃。
    /// 手写分词器曾把「第一个 item 是花括号结尾的已知项」当作整片噪声，
    /// 或因为「行首深度不为 0」整行跳过，于是同一行后面的 `pub mod` 无声消失。
    #[test]
    fn the_module_parser_sees_a_declaration_beside_another_item() {
        let cases: [(&[&str], &str); 4] = [
            (
                &["pub struct Sneaky { pub a: u8 } pub mod tenth_unit;"],
                "结构体之后并排",
            ),
            (
                &["pub fn probe_f() { } pub mod tenth_unit;"],
                "函数之后并排",
            ),
            (
                &["macro_rules! m2 { () => {} } pub mod tenth_unit;"],
                "macro_rules 之后并排",
            ),
            (
                &["mod inner {", "} pub mod tenth_unit;"],
                "收尾大括号行尾随",
            ),
        ];
        for (source, what) in cases {
            assert_eq!(
                parse_modules(source),
                ["tenth_unit"],
                "{what} 的模块声明被静默丢弃了：{source:?}"
            );
        }
    }

    /// 解析器自测探针（第四轮 F-2）：**单行 item 体内的分号不是顶层边界** ——
    /// 合法的一行函数/宏不该让守卫无故变红。
    #[test]
    fn the_module_parser_is_not_fooled_by_a_semicolon_inside_an_item() {
        for single_line_item in [
            "pub fn extra_probe() -> u8 { let v = 7; v }",
            "macro_rules! m3 { () => { foo(); } }",
            "pub const PAIR: (u8, u8) = (1, 2);",
        ] {
            assert!(
                parse_modules(&[single_line_item]).is_empty(),
                "单行 item 不该被当成模块声明：{single_line_item}"
            );
        }
    }

    /// 解析器自测探针（第四轮 F-3）：fail-closed 报的必须是**源码行号**。
    /// 曾经的掩码器在字符串续行处把换行吃掉，报出的行号比源码小 3 行。
    #[test]
    fn the_module_parser_reports_the_real_source_line() {
        let mut source = String::new();
        for n in 1..=6 {
            source.push_str(&format!("// 第 {n} 行占位\n"));
        }
        source.push_str("make_modules!();\n");
        let err = declared_modules(&source).expect_err("顶层宏调用必须 fail closed");
        assert!(err.contains("第 7 行"), "行号必须与源码一致：{err}");
        assert!(err.contains("make_modules!();"), "必须点名那一句：{err}");

        let mut bad = String::new();
        bad.push_str("// 第 1 行占位\n");
        bad.push_str("pub mod a b;\n");
        let err = declared_modules(&bad).expect_err("解析失败必须 fail closed");
        assert!(err.contains("第 2 行"), "行号必须与源码一致：{err}");
        assert!(err.contains("pub mod a b;"), "必须点名那一句：{err}");
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

/// 注入的存储端口的**行为**证据（形状证据在 `mod tests` 里）。
///
/// 单独成文件是为了不把 `lib.rs` 顶成超大单文件；它只在测试期存在，
/// 因此不进入对外模块面。
///
/// 位置在文件**末尾**是有意的：`mod tests` 里的 `crate_root_code()` 按第一个
/// `#[cfg(test)]` 截断源码来取生产半边，这个声明若排在它前面，生产半边会被
/// 从中间砍断。
#[cfg(test)]
mod storage_ports_tests;
