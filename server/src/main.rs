//! 进程入口：子命令选择（team-server §2.4 的「一个二进制、按参数选择模式」）。
//!
//! 对齐桌面 `main.rs` 的形态：入口只做模式选择，一切组装在 `bootstrap`。
//! `serve` 是唯一监听端口的模式；`migrate` / `check-schema` 是 WR-D2 的
//! 交付物，在它们落地前这两个子命令**拒绝执行**而不是静默空跑——一个
//! 「看起来迁移成功了」的空操作比明确的拒绝危险得多。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let subcommand = args.get(1).map(String::as_str);

    match subcommand {
        Some("serve") => datazen_server::bootstrap::serve(),
        Some(sub @ ("migrate" | "check-schema")) => {
            eprintln!(
                "datazen-server: '{sub}' is not implemented in this build — \
                 schema migrations arrive with WR-D2 (web-ready plan §6). \
                 Refusing instead of doing nothing."
            );
            ExitCode::from(2)
        }
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
