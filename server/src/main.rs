//! 进程入口：子命令选择（team-server §2.4 的「一个二进制、按参数选择模式」）。
//!
//! 对齐桌面 `main.rs` 的形态：入口只做模式选择，一切组装在 `bootstrap`。
//! `serve` 是唯一监听端口的模式；`migrate` / `check-schema` 随 WR-D2 第一切片
//! 落地：`migrate` 真实执行 expand 迁移（逐版本输出），`check-schema` 只读
//! 报告版本差距——落后退出码 3（需迁移），超前退出码 4（未知 schema，
//! fail-closed 拒绝运行）。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let subcommand = args.get(1).map(String::as_str);

    match subcommand {
        Some("serve") => datazen_server::bootstrap::serve(),
        Some("migrate") => datazen_server::bootstrap::migrate(),
        Some("check-schema") => datazen_server::bootstrap::check_schema(),
        Some(other) => {
            eprintln!("datazen-server: unknown subcommand '{other}'");
            print_usage();
            ExitCode::from(2)
        }
        None => {
            print_usage();
            ExitCode::from(2)
        }
    }
}

fn print_usage() {
    eprintln!("usage: datazen-server <serve | migrate | check-schema>");
}
