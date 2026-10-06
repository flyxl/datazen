//! `src/resource/**` 的**结构性**单账审计（CM-32 `single_counter_audit`）。
//!
//! # 为什么是语法树而不是子串
//!
//! 上一版的判据是对 `src/resource/*.rs` 全文做**字面子串黑名单**
//! （`Cell<u32` / `Mutex<usize>` / `AtomicU32` 之类，再加三个猜出来的字段名）。
//! 它自称在强制「单账铁律」，实测把第二本账真的写进来也一条都命中不了：
//! `local_tunnels: AtomicU64` + `cached_refs: u32` 两个字段配上 `fetch_add` / `fetch_sub`，
//! 再把 `tunnel_refs()` 改成完全由私账供给 —— 黑名单**全绿通过**。
//! 开放世界的字面匹配挡不住「换个名字的第二本账」，所以这里改用 `syn` 解析，
//! 只认**闭合集合**。
//!
//! # 作用域刻意收窄
//!
//! 判据不撒在 `src/resource/**` 的「不得出现整数」上：该目录现在有 **27** 个合法整数
//! 字段（时间戳、generation、revision、slot、limit、metric），任何粗粒度规则都会把正常
//! 代码判红，而且照样挡不住 `AtomicU64 → u64` 这种改类型名就能滑过去的变形。
//! 作用域是 `ResourceManager` 这一个具体类型 + `tunnel_wiring.rs` 里的方法体。
//!
//! # 五条判据
//!
//! | # | 判据 | 挡住什么 |
//! |---|------|---------|
//! | R1 | `ResourceManager` 的字段名集合**恰好等于**闭合名单（多一个或少一个都红） | 任何第二本账，无论叫什么名字、用什么类型 |
//! | R2 | `tunnels` 字段的类型**恰为** `Option<TunnelLedger>` | 拿别的东西顶包，或把台账降格成裸数字 |
//! | R3 | `src/resource/*.rs` 里**一个 `static` 都没有** | 全局 / 线程级旁账 |
//! | R4 | `tunnel_wiring.rs` 里凡取用 `self.tunnels` 的方法，**只准碰 `self.tunnels`** | 把计数写进表里、写进别的字段里 |
//! | R5 | `tunnel_refs()` 把取数委托给台账、只经 `TunnelLedger::ref_count`、不碰别的 `self` 字段、没有整数字面量 | 从私账 / 常数供给返回值 |
//!
//! # R1 的名单不是随手维护的清单
//!
//! R1 是**有意**设成复核闸：往 `ResourceManager` 加任何字段——哪怕是合法的新字段——都必须
//! 先改这一行，于是「加字段」从一次无声的编辑变成一次显式的架构复核。把它写成类型黑名单
//! （靠 `idle_ttl_seconds: u64` 在册而放行 `u64`）做不到这一点，也挡不住改名换型。
//!
//! # 本审计自己也踩过的两个坑（都是实测出来的，都已修）
//!
//! 1. **R5 曾被挂在 R4 的覆盖面闸门之后**。闸门的判据是「方法体有没有碰 `self.tunnels`」，
//!    而 `tunnel_refs` 被改写成私账供给之后正好**不再碰** `self.tunnels` —— 闸门把方法跳过了，
//!    R5 跟着一起空转。漏洞的形状和它要挡的变异完全同形。现在 R5 无条件先跑，
//!    并且加了「`tunnel_refs` 必须存在」的反空洞闸。
//! 2. **syn 1 看不见 let-else**。syn 1.0.109 根本不认这个语法，整句被塞进 `Expr::Verbatim`，
//!    于是 `let Some(ledger) = self.tunnels.as_mut() else { .. };` 里的 `self.tunnels`
//!    对审计器完全不存在。而 `settle_tunnel_reference` 正是用 let-else 取台账的 ——
//!    R4 会因此整个跳过这个方法。改钉 syn 2：`LocalInit { expr, diverge }` 让
//!    `visit_local` 连 `else` 分支一起下钻。**这一条是 `Cargo.toml` 里必须写 `version = "2"`
//!    的唯一理由**，别为了「少动一次依赖」把它改回去。

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use syn::visit::{self, Visit};
use syn::{
    Expr, ExprField, ExprLit, ExprMethodCall, ExprPath, File as SynFile, Fields, GenericArgument,
    ImplItem, Item, ItemImpl, ItemStruct, Lit, Member, Pat, Path as SynPath, PathArguments, Stmt, Type,
};

/// 被审计的类型名。
const MANAGER: &str = "ResourceManager";
/// 唯一权威账字段名。
const TUNNEL_FIELD: &str = "tunnels";
/// `tunnels` 字段唯一合法的类型。
const TUNNEL_FIELD_TYPE: &str = "Option<TunnelLedger>";
/// R4 里取用台账的方法体**只能**取用的台账方法（`TunnelLedger::ref_count`）。
const REFN_COUNT: &str = "ref_count";

/// R1 的闭合名单。`ResourceManager` 的字段名集合必须**恰好等于**它。
///
/// 这不是缓存下来的现状快照，而是一道复核闸：新增/删除任何字段都要显式改这一行，
/// 改动因此被迫经过一次架构判断，而不是悄悄溜进结构体。
const MANAGER_FIELDS: [&str; 9] = [
    "table",
    "generations",
    "ledger",
    "transport",
    "clock",
    "disabled",
    "queue",
    "idle_ttl_seconds",
    "tunnels",
];

/// 跑完整套审计，返回**人类可读的违规清单**；空清单即通过。
///
/// `resource_dir` 通常是 `packages/runtime/src/resource`。
pub fn audit(resource_dir: &Path) -> Vec<String> {
    let mut findings = Vec::new();
    match parse(&resource_dir.join("manager.rs"), &mut findings) {
        Some(file) => audit_manager(&file, &resource_dir.join("manager.rs"), &mut findings),
        None => {}
    }
    match parse(&resource_dir.join("tunnel_wiring.rs"), &mut findings) {
        Some(file) => audit_wiring(&file, &resource_dir.join("tunnel_wiring.rs"), &mut findings),
        None => {}
    }
    audit_no_statics(resource_dir, &mut findings);
    findings
}

/// 单个测试用：把清单拼成一条可直接 `assert!(..., "{msg}")` 的消息。
pub fn describe(findings: &[String]) -> String {
    format!("单账铁律（CM-32）被破坏，共 {} 处：\n{}", findings.len(), findings.join("\n"))
}

// ---------------------------------------------------------------- 文件与解析

fn parse(path: &Path, findings: &mut Vec<String>) -> Option<SynFile> {
    let source = match std::fs::read_to_string(path) {
        Ok(source) => source,
        Err(error) => {
            findings.push(format!("{}: 读不到源文件（{error}），审计无法进行", path.display()));
            return None;
        }
    };
    match syn::parse_file(&source) {
        Ok(file) => Some(file),
        Err(error) => {
            findings.push(format!("{}: 解析失败（{error}），审计无法进行", path.display()));
            None
        }
    }
}

fn audit_manager(file: &SynFile, path: &Path, findings: &mut Vec<String>) {
    let structs: Vec<&ItemStruct> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Struct(item) => Some(item),
            _ => None,
        })
        .collect();
    let Some(manager) = structs.iter().find(|item| item.ident == MANAGER) else {
        findings.push(format!(
            "{}: 找不到 `struct {MANAGER}`，R1/R2 的审计面已经消失",
            path.display()
        ));
        return;
    };

    let Fields::Named(named) = &manager.fields else {
        findings.push(format!(
            "{}: `{MANAGER}` 不是具名字段结构体，R1 无法审计",
            path.display()
        ));
        return;
    };

    // R1：字段名集合必须恰好等于闭合名单。
    let actual: BTreeSet<String> = named
        .named
        .iter()
        .map(|field| field.ident.as_ref().map(|ident| ident.to_string()).unwrap_or_default())
        .collect();
    let expected: BTreeSet<String> = MANAGER_FIELDS.iter().map(|name| (*name).to_owned()).collect();
    let extra: Vec<&String> = actual.difference(&expected).collect();
    let missing: Vec<&String> = expected.difference(&actual).collect();
    if !extra.is_empty() || !missing.is_empty() {
        findings.push(format!(
            "{}: `{MANAGER}` 的字段名集合必须**恰好**等于 {expected:?}。\
             多出 {extra:?}（第二本账最常见的藏身处就是这里），缺少 {missing:?}。\
             这是一道复核闸：改名单之前先确认新字段没有引入第二份隧道计数。",
            path.display()
        ));
    }

    // R2：`tunnels` 字段的类型必须恰为 `Option<TunnelLedger>`。
    for field in &named.named {
        let Some(ident) = &field.ident else { continue };
        if ident != TUNNEL_FIELD {
            continue;
        }
        let rendered = render_type(&field.ty);
        if rendered != TUNNEL_FIELD_TYPE {
            findings.push(format!(
                "{}: `{MANAGER}.{TUNNEL_FIELD}` 的类型必须是 `{TUNNEL_FIELD_TYPE}`（台账本身就是\
                 唯一权威），实际是 `{rendered}`",
                path.display()
            ));
        }
    }
}

fn audit_wiring(file: &SynFile, path: &Path, findings: &mut Vec<String>) {
    let impls: Vec<&ItemImpl> = file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Impl(item) => Some(item),
            _ => None,
        })
        .filter(|item| render_type(&item.self_ty) == MANAGER)
        .collect();
    if impls.is_empty() {
        findings.push(format!(
            "{}: 找不到 `impl {MANAGER}`，R4/R5 的审计面已经消失",
            path.display()
        ));
        return;
    }

    let mut audited = 0usize;
    let mut refs_seen = false;
    for item in impls {
        for entry in &item.items {
            // syn 2 把 syn 1 的 `ImplItem::Method` 改名叫 `ImplItem::Fn`（字段 `sig`/`block` 不变）。
            let ImplItem::Fn(method) = entry else { continue };
            let name = method.sig.ident.to_string();
            let body = scan(&method.block);

            // R5 先跑，且**刻意不受 R4 覆盖面闸门约束**。
            // 上一版把它挂在闸门之后，恰好让「把 `tunnel_refs` 整个改写成私账供给」
            // 这类变异绕开全部判据：改写后方法体不再碰 `self.tunnels`，R4 认定它
            // 「不取用台账」而跳过，R5 也跟着一起跳过——漏洞的形状和被测的变异完全同形，
            // 是实测出来的（见模块文档「两个真实漏洞」第 1 条）。
            if name == "tunnel_refs" {
                refs_seen = true;
                audit_tunnel_refs(&body, path, findings);
            }

            // R4 的覆盖面是「凡取用台账的方法」——不看名单，只看方法体有没有碰
            // `self.tunnels`。碰了台账却还去读别的字段，就是把计数分到了第二处。
            if !body.reads.iter().any(|(field, _)| field == TUNNEL_FIELD) {
                continue;
            }
            audited += 1;
            for (field, _) in &body.reads {
                if field != TUNNEL_FIELD {
                    findings.push(format!(
                        "{}: `{name}` 读了 `self.{field}`。凡取用隧道台账的方法只准碰\
                         `self.{TUNNEL_FIELD}`：计数一旦落到别的字段/表里，台账就不再是唯一权威，\
                         CM-32 的单账铁律随之失效。",
                        path.display()
                    ));
                }
            }
            if body.bare_self > 0 {
                findings.push(format!(
                    "{}: `{name}` 里有 {} 处 `self` 不是 `{TUNNEL_FIELD}` 字段访问的基\
                     （取整份 `self`、整体传递等），无法归因到台账。",
                    path.display(),
                    body.bare_self
                ));
            }
        }
    }

    // 反空洞闸：一个方法都不取用台账，说明接线被拆了或改名了，此时上面全是空转。
    if audited == 0 {
        findings.push(format!(
            "{}: 没有任何方法取用 `self.{TUNNEL_FIELD}`，单账审计变成空转（接线被拆了？\
             字段被改名了？）",
            path.display()
        ));
    }
    // R5 的反空洞闸：`tunnel_refs` 本身就是这个接线模块对外的投影面，它改名或被挪走，
    // 意味着「引用计数快照」不再由这个模块直出——R5 会静默变成空转。
    if !refs_seen {
        findings.push(format!(
            "{}: 找不到 `tunnel_refs`，R5 的审计面已经消失（改名了，或被挪出了接线模块？）",
            path.display()
        ));
    }
}

fn audit_tunnel_refs(body: &Body, path: &Path, findings: &mut Vec<String>) {
    // R5a：取数必须真的委托给台账。
    if body.ledger_calls.is_empty() {
        findings.push(format!(
            "{}: `tunnel_refs` 没有把取数委托给 `self.{TUNNEL_FIELD}` 上的任何方法——\
             返回值必然来自别处（私账、局部变量、字面量、函数参数）。",
            path.display()
        ));
    }

    // R5b：`self.tunnels` 只能作为方法接收者出现，不得被读取进局部变量 / 直接返回 /
    // 取地址 / 解引用 —— 一旦读出来，调用方就可能拿它当第二份快照。
    let roots: BTreeSet<usize> = body
        .ledger_calls
        .iter()
        .map(|(root, _, _)| *root)
        .collect();
    for (field, site) in &body.reads {
        if field == TUNNEL_FIELD && !roots.contains(site) {
            findings.push(format!(
                "{}: `tunnel_refs` 里的 `self.{TUNNEL_FIELD}` 有一处不是台账方法的接收者。\
                 它只准出现在 `self.{TUNNEL_FIELD}.<台账方法>(…)` 的接收者位（可链式）。",
                path.display()
            ));
        }
    }

    // R5c：一个纯投影函数里不该出现任何整数字面量（`Some(0)`、`map_or(1, …)` 之类）。
    if body.int_literals > 0 {
        findings.push(format!(
            "{}: `tunnel_refs` 里出现了 {} 处整数字面量。投影函数的返回值只能来自台账，\
             凭空出现的数字就是「计数搬到了本地」的信号。",
            path.display(),
            body.int_literals
        ));
    }

    // R5d：从台账取出的值只准经 `TunnelLedger::ref_count` 投影。
    // 台账经 `.as_ref()` / `.and_then(…)` 交出来的绑定名，链路上只准再调这一个方法。
    let derived: BTreeSet<String> = body
        .ledger_calls
        .iter()
        .flat_map(|(_, _, bound)| bound.iter().cloned())
        .collect();
    let projected: BTreeSet<String> = body
        .calls
        .iter()
        .filter_map(|(receiver, method)| match receiver {
            Some(name) if derived.contains(name) => Some(method.clone()),
            _ => None,
        })
        .collect();
    let allowed: BTreeSet<String> = std::iter::once(REFN_COUNT.to_owned()).collect();
    // `derived` 为空时本条无话可说：要么台账方法一个都没调（R5a 已经报了），
    // 要么投影写成路径限定形式 `TunnelLedger::ref_count`——那种写法 `calls` 里
    // 本来就没有记录，对它判红是**误报**。只有真拿到了台账绑定名才继续比。
    if !derived.is_empty() && projected != allowed {
        findings.push(format!(
            "{}: `tunnel_refs` 从台账取值时调用的方法是 {projected:?}，只允许 [{REFN_COUNT:?}]。\
             `tunnel_refs` 是 `TunnelLedger::{REFN_COUNT}` 的直通投影，不该改投影别的量。",
            path.display()
        ));
    }
}

fn audit_no_statics(resource_dir: &Path, findings: &mut Vec<String>) {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(resource_dir) {
        Ok(entries) => entries
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "rs"))
            .collect(),
        Err(error) => {
            findings.push(format!(
                "{}: 列不出目录（{error}），R3 的审计面已经消失",
                resource_dir.display()
            ));
            return;
        }
    };
    files.sort();
    if files.is_empty() {
        findings.push(format!(
            "{}: 一个 *.rs 都没扫到，R3 的审计面已经消失",
            resource_dir.display()
        ));
        return;
    }
    for file in files {
        let Some(syntax) = parse(&file, findings) else { continue };
        for item in &syntax.items {
            if let Item::Static(item) = item {
                findings.push(format!(
                    "{}: 出现了全局旁账 `static {}: {}`。单账铁律下 `src/resource` 里一个\
                     `static` 都不该有——它是全模块之外、审计面之外的计数。",
                    file.display(),
                    item.ident,
                    render_type(&item.ty)
                ));
            }
        }
    }
}

// ------------------------------------------------------------------ 方法体扫描

/// 一个方法体里与单账有关的事实。
#[derive(Default)]
struct Body {
    /// `self.<field>` 读点：字段名 + 节点地址（用来判断它是不是某次调用的接收者）。
    reads: Vec<(String, usize)>,
    /// 不作为 `self.tunnels` 基的裸 `self` 出现次数。
    bare_self: usize,
    /// 整数字面量出现次数。
    int_literals: usize,
    /// 方法调用：接收者若是单标识符路径则记下该标识符，否则 `None`。
    calls: Vec<(Option<String>, String)>,
    /// 接收者根是 `self.tunnels` 的调用：根节点地址 + 方法名 + 参数里被闭包绑定的标识符。
    ledger_calls: Vec<(usize, String, Vec<String>)>,
}

fn scan(block: &syn::Block) -> Body {
    let mut body = Body::default();
    for statement in &block.stmts {
        body.visit_stmt(statement);
    }
    // 建造者模式（`with_tunnel_transport`）的块尾巴上一个裸 `self` 是**返回值**
    // （`mut self → Self`），不是「把整份 `self` 递出去」，不计入逃逸。
    // syn 2 的 `Stmt::Expr` 带一个分号槽位，这里用 `_` 忽略。
    if let Some(Stmt::Expr(Expr::Path(path), _)) = block.stmts.last() {
        if is_self_path(&path.path) {
            body.bare_self = body.bare_self.saturating_sub(1);
        }
    }
    body
}

impl<'ast> Visit<'ast> for Body {
    fn visit_expr_path(&mut self, node: &'ast ExprPath) {
        if is_self_path(&node.path) {
            self.bare_self += 1;
            // `self` 路径没有子节点；不下钻。
            return;
        }
        visit::visit_expr_path(self, node);
    }

    fn visit_expr_field(&mut self, node: &'ast ExprField) {
        if is_self_expr(&node.base) {
            let field = match &node.member {
                Member::Named(ident) => ident.to_string(),
                Member::Unnamed(index) => index.index.to_string(),
            };
            self.reads.push((field, node as *const ExprField as usize));
            // 不下钻 `base`：它就是那个 `self`，已经在上面归因完了。
            return;
        }
        visit::visit_expr_field(self, node);
    }

    fn visit_expr_lit(&mut self, node: &'ast ExprLit) {
        if matches!(node.lit, Lit::Int(_)) {
            self.int_literals += 1;
        }
        visit::visit_expr_lit(self, node);
    }

    fn visit_expr_method_call(&mut self, node: &'ast ExprMethodCall) {
        let method = node.method.to_string();
        let receiver = match node.receiver.as_ref() {
            Expr::Path(path)
                if path.qself.is_none() && path.path.get_ident().is_some() =>
            {
                path.path.get_ident().map(|ident| ident.to_string())
            }
            _ => None,
        };
        self.calls.push((receiver, method.clone()));
        if let Some(root) = ledger_root(&node.receiver) {
            let mut bound = Vec::new();
            for argument in &node.args {
                collect_closure_params(argument, &mut bound);
            }
            self.ledger_calls.push((root as usize, method, bound));
        }
        visit::visit_expr_method_call(self, node);
    }
}

fn is_self_expr(expr: &Expr) -> bool {
    matches!(expr, Expr::Path(path) if is_self_path(&path.path))
}

fn is_self_path(path: &SynPath) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() == 1
        && path.segments[0].ident == "self"
}

/// 沿接收者链往下找到 `self.<TUNNEL_FIELD>`，返回那个节点的地址。
fn ledger_root(receiver: &Expr) -> Option<*const Expr> {
    match receiver {
        Expr::Field(field) => {
            let is_tunnels = matches!(&field.member, Member::Named(ident) if ident == TUNNEL_FIELD);
            if is_tunnels && is_self_expr(&field.base) {
                // `Expr::Field(ExprField)` 是按值持有的，地址与 `*const ExprField` 相同。
                Some(field as *const ExprField as *const Expr)
            } else {
                None
            }
        }
        Expr::MethodCall(call) => ledger_root(&call.receiver),
        Expr::Paren(paren) => ledger_root(&paren.expr),
        Expr::Group(group) => ledger_root(&group.expr),
        Expr::Try(try_) => ledger_root(&try_.expr),
        Expr::Await(await_) => ledger_root(&await_.base),
        _ => None,
    }
}

fn collect_closure_params(expr: &Expr, out: &mut Vec<String>) {
    let Expr::Closure(closure) = expr else { return };
    for input in &closure.inputs {
        match input {
            Pat::Ident(ident) => out.push(ident.ident.to_string()),
            Pat::Type(typed) => {
                if let Pat::Ident(ident) = typed.pat.as_ref() {
                    out.push(ident.ident.to_string());
                }
            }
            _ => {}
        }
    }
}

// ------------------------------------------------------------------ 类型渲染

/// 把 `syn::Type` 渲染成人类可读的紧凑串（`Option<TunnelLedger>`、`&u32` …）。
///
/// 覆盖面刻意只做「本仓库真会出现的形状」；其余形状返回一个显式标记而不是
/// 悄悄猜——R2 拿它和 `Option<TunnelLedger>` 比对，标记会如实判红。
pub fn render_type(ty: &Type) -> String {
    match ty {
        Type::Path(path) => render_path(&path.path),
        Type::Reference(reference) => {
            let prefix = if reference.mutability.is_some() { "&mut " } else { "&" };
            format!("{prefix}{}", render_type(&reference.elem))
        }
        Type::Paren(paren) => format!("({})", render_type(&paren.elem)),
        Type::Tuple(tuple) => join(tuple.elems.iter().map(render_type), ", "),
        Type::Slice(slice) => format!("[{}]", render_type(&slice.elem)),
        Type::Array(array) => match &array.len {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Int(int) => format!("[{}; {}]", render_type(&array.elem), int.base10_digits()),
                _ => format!("[{}; «len»]", render_type(&array.elem)),
            },
            _ => format!("[{}; «len»]", render_type(&array.elem)),
        },
        _ => "«unsupported type shape»".to_owned(),
    }
}

fn render_path(path: &SynPath) -> String {
    join(
        path.segments.iter().map(|segment| {
            let ident = segment.ident.to_string();
            match &segment.arguments {
                PathArguments::None => ident,
                PathArguments::AngleBracketed(arguments) => format!(
                    "{ident}<{}>",
                    join(arguments.args.iter().map(render_generic_argument), ", ")
                ),
                PathArguments::Parenthesized(arguments) => format!(
                    "{ident}({})",
                    join(arguments.inputs.iter().map(render_type), ", ")
                ),
            }
        }),
        "::",
    )
}

fn render_generic_argument(argument: &GenericArgument) -> String {
    match argument {
        GenericArgument::Type(ty) => render_type(ty),
        GenericArgument::Lifetime(lifetime) => lifetime.to_string(),
        GenericArgument::Const(expr) => match expr {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Int(int) => int.base10_digits().to_string(),
                _ => "«const»".to_owned(),
            },
            _ => "«const»".to_owned(),
        },
        _ => "«generic»".to_owned(),
    }
}

fn join(items: impl Iterator<Item = String>, separator: &str) -> String {
    items.collect::<Vec<String>>().join(separator)
}
