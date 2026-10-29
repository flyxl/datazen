//! 静默 E2E：让本地 `webdriver` 构建在测试期间不抢走开发者的键盘焦点。
//!
//! macOS 上 DataZen 共有四处会把进程顶到前台，其中第 4 处是真正难缠的：
//!
//! 1. tao 的 `applicationDidFinishLaunching`（`AppState::launched`）里
//!    `window_activation_hack` + `activateIgnoringOtherApps:`；
//! 2. wry 每建一个 webview 都调一次 `-[NSApplication activate]`（macOS 14+，
//!    更早的系统上退化到 `activateIgnoringOtherApps:`）；
//! 3. 窗口 `show()` 走 `orderFront:`——不夺 key，但会把窗口摆到开发者屏幕上；
//! 4. 窗口 `set_focus()` 走 `makeKeyAndOrderFront:`，而 **AppKit 在窗口 becomeKey
//!    时有一条把应用整体激活的内部路径**，它既不经过 `activate` 也不经过
//!    `activateIgnoringOtherApps:`。
//!
//! 前两处发生在**第一个窗口之前**，后两处由 E2E 打开子窗口时触发（`commands/
//! window.rs` 里有 8 处 `set_focus()`）。全部都得在**事件循环启动之前**处理掉：
//! 第 1 次发生在 Tauri 的 setup 钩子之前（实测把拦截写进 setup，进程照样在启动后
//! 0.3s 被顶到前台，而那会儿第一个窗口还没建）。因此 [`install`] 由
//! `bootstrap/run.rs` 在构造 `tauri::Builder` 之前调用，做两件事：
//!
//! - 把本进程 `NSApplication` 基类上的 `activate` / `activateIgnoringOtherApps:`
//!   换成空实现（ObjC 方法替换是进程内的，系统里其它应用不受影响），并把激活
//!   策略降级为 `Accessory`——这个进程不再能被系统激活，没有 Dock 图标、
//!   不进 Cmd-Tab；
//! - 把 `NSWindow` 的 `orderFront:` / `makeKeyAndOrderFront:` 换成「透明窗口 + 只
//!   `orderFrontRegardless:`」（`makeOrderedFront:` 不用管，macOS 15 的 `NSWindow`
//!   并没有这个方法）。
//!
//! 第二件事才是根治：只做第一件时启动确实安静了，但 E2E 一开子窗口，第 4 条路径
//! 又把前台抢走。窗口不做 key，AppKit 就没有理由去激活这个进程。同时 alpha 0 +
//! 浮在最上层让窗口既看不见也不遮挡编辑器，而合成器仍在为它取帧，`WKWebView` 的
//! `takeSnapshot`、JS 合成点击与输入都照常工作（这三条都不要求窗口可见或进程
//! 激活）。想保留「窗口可见但不抢焦点」时用 `DATAZEN_E2E_QUIET_CONCEAL=0`。
//!
//! 同一台机器、同一组 spec（`data-transfer-window.ts` + `settings.ts`，约 90 秒，
//! 期间人一直在 Code / ChatGPT 之间切应用，采样 `lsappinfo front`）：conceal 开
//! 时 29/29 通过、DataZen 抢前台 **0** 次，关掉后同一组用例里被顶到前台 **8** 次。
//! 开着 conceal 时应用日志里有 68~90 条 `key request declined`，即 `set_focus()`
//! 确实一路走到 `makeKeyAndOrderFront:` 并被拦下，不是「本来就没触发」。
//!
//! 只设策略不换选择器、或者只在 setup 里做，都拦不住启动那一次；两件事一起做
//! 才稳定。
//!
//! 未设置该变量时 [`install`] 是彻底的空操作，`e2e:shots` 画廊采集与
//! `demo-recording` 演示录制因此照旧跑可见窗口。

/// 打开静默模式的开关环境变量，取值 `1` / `true`。
#[cfg(feature = "webdriver")]
pub const ENV_VAR: &str = "DATAZEN_E2E_QUIET";

/// 窗口隐藏方式的开关，取值 `0` / `false` 时保留「窗口可见但不抢焦点」的行为。
///
/// 默认开启：只拦激活时窗口仍然会浮在开发者屏幕上盖住编辑器，这不算真正的
/// headless。调试「不抢焦点但想看见窗口」时用 `DATAZEN_E2E_QUIET_CONCEAL=0`。
#[cfg(all(feature = "webdriver", target_os = "macos"))]
const CONCEAL_ENV_VAR: &str = "DATAZEN_E2E_QUIET_CONCEAL";

/// 本次进程是否处于静默 E2E 模式。
///
/// 不带 `webdriver` feature 的构建恒为 `false`，因此生产二进制里既没有这段
/// 开关逻辑，也没有下面的 macOS shim。
#[must_use]
pub fn enabled() -> bool {
    #[cfg(feature = "webdriver")]
    {
        std::env::var(ENV_VAR).is_ok_and(|v| v == "1" || v.eq_ignore_ascii_case("true"))
    }
    #[cfg(not(feature = "webdriver"))]
    {
        false
    }
}

/// 安装激活拦截。必须在 `tauri::Builder` 构造之前调用（见 `bootstrap/run.rs`）；
/// 未设置环境变量时是彻底的空操作，因此 `e2e:shots` / `e2e:demo` 这类需要
/// 真实前台窗口的运行完全不受影响。
pub fn install() {
    if !enabled() {
        return;
    }
    #[cfg(all(feature = "webdriver", target_os = "macos"))]
    refuse_activation();
}

/// 在事件循环启动之前做两件事：把本进程 `NSApplication` 的两个激活选择器替换成
/// 空实现并把激活策略降级为 `Accessory`；再把 `NSWindow` 的三个排序方法换成
/// 「透明窗口 + 不拿 key」。
///
/// 必须早于 `tauri::Builder` 构造：tao 在 `applicationDidFinishLaunching`
/// （`AppState::launched`）里就会调 `window_activation_hack` +
/// `activateIgnoringOtherApps`，wry 每建一个 webview 也会调 `activate`。那两次
/// 都早于 Tauri 的 setup 钩子——实测把拦截放在 setup 里毫无作用，启动后 0.3s
/// 就被顶到前台。
///
/// 因此这里替换的是 **`NSApplication` 基类本身**：调用点此时还没建出 tao 的
/// `TaoApp` 子类（`object_setClass` 发生在事件循环里），而 `TaoApp` 并不自己实现
/// 这两个选择器，消息仍会落到基类上。ObjC 的方法替换是**进程内**的，系统里其它
/// 应用不受影响。WebDriver 走 JS 合成事件与 `takeScreenshot`，都不要求应用激活。
#[cfg(all(feature = "webdriver", target_os = "macos"))]
fn refuse_activation() {
    use objc2::ffi;
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::{sel, ClassType, MainThreadMarker};
    use objc2_app_kit::{NSFloatingWindowLevel, NSWindow, NSWindowCollectionBehavior};

    extern "C-unwind" fn ignore(_: &AnyObject, _: Sel) {}
    extern "C-unwind" fn ignore_with_flag(_: &AnyObject, _: Sel, _: Bool) {}

    /// 把窗口改成「存在但看不见，也不参与焦点」：alpha 0、不吃鼠标事件、浮在
    /// 所有普通窗口之上（合成器仍为它取帧，`WKWebView` 快照仍有内容）、加入
    /// 所有 Space 且不打断全屏。
    ///
    /// 最后一步是整个机制的关键：只调 `orderFrontRegardless:`，**不** makeKey。
    /// 进程因此永远没有 key window，AppKit 那条「窗口 becomeKey 时把应用顶到
    /// 前台」的内部路径就没有触发条件——它并不经过 `activate` 或
    /// `activateIgnoringOtherApps:`，所以只拦那两个选择器是拦不住的（实测
    /// 启动那一次确实安静了，但 E2E 一打开子窗口、命令里的 `set_focus()` 走到
    /// `makeKeyAndOrderFront:`，前台就被顶走，同一组 spec 里被顶了 8 次）。
    fn conceal(this: &AnyObject, selector: &str) {
        let Some(window) = this.downcast_ref::<NSWindow>() else {
            return;
        };
        if selector == "makeKeyAndOrderFront:" {
            // 留一条 info：`set_focus()` 每次都会走到这里，前台万一仍被抢走，
            // 拿这条日志的时间戳和 `lsappinfo front` 的时间轴对齐即可定位。
            tracing::info!(
                window = window.windowNumber(),
                selector,
                "e2e quiet mode: key request declined (ordered front, left non-key)"
            );
        }
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
        window.setLevel(NSFloatingWindowLevel);
        window.setCollectionBehavior(
            NSWindowCollectionBehavior::CanJoinAllSpaces
                | NSWindowCollectionBehavior::FullScreenAuxiliary,
        );
        window.orderFrontRegardless();
    }

    extern "C-unwind" fn conceal_key(this: &AnyObject, _: Sel) {
        conceal(this, "makeKeyAndOrderFront:");
    }
    /// `orderFront:` 带一个 sender 参数，替换时签名必须保持一致。
    extern "C-unwind" fn conceal_plain(this: &AnyObject, _: Sel, _: &AnyObject) {
        conceal(this, "orderFront:");
    }

    /// 用 `imp` 覆盖 `class` 上 `selector` 的实现，沿用原方法的类型编码。
    /// 系统版本没有该选择器时返回 `false`，不影响其它替换。
    ///
    /// # Safety
    /// `imp` 的签名必须与 `selector` 对应的原方法一致。
    unsafe fn replace_method(class: &'static AnyClass, selector: Sel, imp: Imp) -> bool {
        let Some(method) = class.instance_method(selector) else {
            return false;
        };
        ffi::class_replaceMethod(
            std::ptr::from_ref(class).cast_mut(),
            selector,
            imp,
            ffi::method_getTypeEncoding(method),
        );
        true
    }

    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!(
            env = ENV_VAR,
            "not on the main thread — activation stays enabled"
        );
        return;
    };

    // 策略降级是第一道闸：Accessory 进程不会被系统激活，也没有 Dock 图标。
    // 提前 `sharedApplication()` 顺带把单例建出来，此时还没有任何窗口，
    // setActivationPolicy 一定被接受。
    let app = objc2_app_kit::NSApplication::sharedApplication(mtm);
    let policy_set =
        app.setActivationPolicy(objc2_app_kit::NSApplicationActivationPolicy::Accessory);

    let ns_app: &'static AnyClass = objc2_app_kit::NSApplication::class();
    let activate_overrides: [(Sel, Imp); 2] = [
        (sel!(activate), unsafe {
            std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel), Imp>(ignore)
        }),
        (sel!(activateIgnoringOtherApps:), unsafe {
            std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, Bool), Imp>(
                ignore_with_flag,
            )
        }),
    ];
    let replaced = activate_overrides
        .iter()
        .filter(|(selector, imp)| unsafe { replace_method(ns_app, *selector, *imp) })
        .count();

    // 窗口侧：两个排序入口都要接管。只拦 `makeKeyAndOrderFront:` 不够，
    // `show()` 走的是 `orderFront:`，那条路同样会把窗口摆到用户屏幕上。
    let conceal_on = std::env::var(CONCEAL_ENV_VAR)
        .map(|v| v != "0" && !v.eq_ignore_ascii_case("false"))
        .unwrap_or(true);
    let mut ordered = 0usize;
    if conceal_on {
        let ns_window: &'static AnyClass = NSWindow::class();
        let order_overrides: [(&str, Sel, Imp); 2] = [
            (
                "makeKeyAndOrderFront:",
                sel!(makeKeyAndOrderFront:),
                unsafe {
                    std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel), Imp>(conceal_key)
                },
            ),
            ("orderFront:", sel!(orderFront:), unsafe {
                std::mem::transmute::<extern "C-unwind" fn(&AnyObject, Sel, &AnyObject), Imp>(
                    conceal_plain,
                )
            }),
        ];
        for (name, selector, imp) in order_overrides {
            if unsafe { replace_method(ns_window, selector, imp) } {
                ordered += 1;
            } else {
                // 系统版本没有这个选择器，列出名字才能判断漏掉的是哪条路径。
                tracing::warn!(
                    selector = name,
                    "window ordering selector missing — that entry point stays un-concealed"
                );
            }
        }
    }

    if !policy_set {
        tracing::error!(
            env = ENV_VAR,
            "setActivationPolicy(Accessory) refused — the app may still steal focus"
        );
    }
    if replaced == 0 {
        tracing::error!(
            env = ENV_VAR,
            "e2e quiet mode: no activation selector was replaced"
        );
    }
    if conceal_on && ordered == 0 {
        tracing::error!(
            env = ENV_VAR,
            "e2e quiet mode: no window ordering selector was replaced — windows stay visible"
        );
    }
    tracing::info!(
        env = ENV_VAR,
        conceal_env = CONCEAL_ENV_VAR,
        activate_replaced = replaced,
        order_replaced = ordered,
        accessory = policy_set,
        "e2e quiet mode: app activation refused"
    );
}
