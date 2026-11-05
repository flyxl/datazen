mod app_data;
mod cli;
mod commands;
mod desktop_launch;
mod error;
mod output;
mod platform_vault;
mod query;
mod skill;

use clap::Parser;
use cli::{Cli, Command};

// The driver crates register themselves through `inventory`; referencing them
// here keeps the linker from dropping those registrations from the binary.
#[allow(unused_imports)]
use datazen_driver_mysql as _;
#[allow(unused_imports)]
use datazen_driver_postgres as _;
#[allow(unused_imports)]
use datazen_driver_sqlite as _;

#[tokio::main]
async fn main() -> std::process::ExitCode {
    let cli = Cli::parse();
    let json = command_wants_json(&cli.command);
    match commands::run(cli).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            output::emit_error(&error.payload(), json);
            std::process::ExitCode::from(error.exit_code() as u8)
        }
    }
}

fn command_wants_json(command: &Command) -> bool {
    match command {
        Command::Doctor(output) | Command::Capabilities(output) | Command::Connections(output) => {
            output.json
        }
        Command::Schema(args) => args.output.json,
        Command::Query(args) => args.output.json,
        Command::Context(args) => args.output.json,
        Command::Open(args) => args.output.json,
        Command::Agent(args) => match &args.action {
            cli::AgentAction::Setup(output) => output.json,
        },
    }
}
